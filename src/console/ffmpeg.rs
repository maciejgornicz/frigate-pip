//! Decoding with the `ffmpeg` program: our RTSP client feeds it H.264/H.265 through a
//! pipe (or ffmpeg reads Frigate's JPEG/MJPEG itself) and it returns raw BGRA frames
//! already scaled to the window. Audio is played by ffmpeg straight to PulseAudio
//! (PipeWire) or ALSA.

use super::canvas::Image;
use crate::view::Zoom;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdout, Command, Stdio};

pub struct Ffmpeg {
    path: String,
    /// `-fps_mode` (ffmpeg 5.1+) or `-vsync` (older): frames are passed on as they come.
    fps_flag: &'static str,
    /// Audio output: (format, device).
    audio_out: (&'static str, &'static str),
    hwaccel: HwAccel,
}

/// `[console] hwaccel`: video decoding on the graphics card (VAAPI, VDPAU, NVDEC… whatever
/// ffmpeg finds; without one it decodes on the processor as before).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HwAccel {
    /// Only the main stream: the sub stream is decoded faster by the processor.
    Auto,
    On,
    Off,
}

impl HwAccel {
    pub fn parse(v: &str) -> HwAccel {
        match v.to_ascii_lowercase().as_str() {
            "on" | "true" | "yes" | "1" => HwAccel::On,
            "off" | "false" | "no" | "0" => HwAccel::Off,
            _ => HwAccel::Auto,
        }
    }

    /// `main` = the camera's main (high-resolution) stream.
    pub fn wanted(self, main: bool) -> bool {
        match self {
            HwAccel::Auto => main,
            HwAccel::On => true,
            HwAccel::Off => false,
        }
    }
}

/// A running ffmpeg, killed when dropped.
pub struct Proc(pub Child);

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Ffmpeg {
    /// Checks that ffmpeg can be started and reads its version.
    pub fn find(path: &str, hwaccel: HwAccel) -> Result<Ffmpeg, String> {
        let out = Command::new(path)
            .args(["-hide_banner", "-version"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("cannot start {path}: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let first = text.lines().next().unwrap_or_default().to_string();
        let version = first.split_whitespace().nth(2).unwrap_or_default();
        let mut parts = version.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse::<u32>().unwrap_or(0));
        let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
        // Builds from git report "N-12345-g..." (no number): assume a recent version.
        let modern = major == 0 || (major, minor) >= (5, 1);
        let pulse = std::env::var_os("PULSE_SERVER").is_some()
            || std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|d| std::path::Path::new(&d).join("pulse/native").exists());
        crate::log!("{first}");
        // FRIGATEPIP_AUDIO_OUT=format:target (e.g. wav:/tmp/a.wav) records the audio instead (tests).
        let audio_out = match std::env::var("FRIGATEPIP_AUDIO_OUT").ok().and_then(|v| v.split_once(':').map(|(f, t)| (f.to_string(), t.to_string())))
        {
            Some((f, t)) => (&*Box::leak(f.into_boxed_str()), &*Box::leak(t.into_boxed_str())),
            None if pulse => ("pulse", "FrigatePiP"),
            None => ("alsa", "default"),
        };
        Ok(Ffmpeg { path: path.to_string(), fps_flag: if modern { "-fps_mode" } else { "-vsync" }, audio_out, hwaccel })
    }

    fn command(&self) -> Command {
        let mut c = Command::new(&self.path);
        // FRIGATEPIP_FFMPEG_LOG=file collects ffmpeg's messages (diagnostics).
        let log = std::env::var_os("FRIGATEPIP_FFMPEG_LOG").and_then(|p| std::fs::OpenOptions::new().create(true).append(true).open(p).ok());
        c.args(["-hide_banner", "-loglevel", if log.is_some() { "info" } else { "error" }]);
        c.stderr(log.map_or_else(Stdio::null, Stdio::from));
        // ffmpeg is killed together with the thread that started it, also when
        // FrigatePiP itself ends (e.g. a tmux popup is closed).
        unsafe {
            c.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        c
    }

    /// Output options: raw BGRA frames of `size` on stdout, without delay.
    fn raw_out(&self, c: &mut Command, size: (u32, u32), zoom: Zoom) {
        let filter = video_filter(size, zoom);
        c.args(["-an", "-vf", &filter, "-pix_fmt", "bgra", self.fps_flag, "passthrough", "-flush_packets", "1", "-f", "rawvideo", "pipe:1"]);
    }

    /// Decoder fed through stdin with an elementary stream (`h264` or `hevc`); `main` = the
    /// camera's main stream (see `HwAccel`).
    pub fn video_pipe(&self, format: &str, size: (u32, u32), main: bool, zoom: Zoom) -> std::io::Result<Proc> {
        let mut c = self.command();
        // No "-fflags nobuffer": it drops what was read while probing — the first key frame.
        c.args(["-flags", "low_delay", "-probesize", "32", "-analyzeduration", "0", "-threads", "1"]);
        if self.hwaccel.wanted(main) {
            c.args(["-hwaccel", "auto"]);
        }
        c.args(["-f", format, "-i", "pipe:0"]);
        self.raw_out(&mut c, size, zoom);
        c.stdin(Stdio::piped()).stdout(Stdio::piped());
        c.spawn().map(Proc)
    }

    /// ffmpeg reading a URL itself (Frigate's snapshot or MJPEG stream).
    pub fn url(&self, url: &str, format: Option<&str>, size: (u32, u32), one_frame: bool, zoom: Zoom) -> std::io::Result<Proc> {
        let mut c = self.command();
        // A Frigate that stops answering must not leave the process hanging (5 s, in µs).
        c.args(["-probesize", "32768", "-analyzeduration", "0", "-rw_timeout", "5000000"]);
        if let Some(f) = format {
            c.args(["-f", f]);
        }
        c.args(["-i", url]);
        if one_frame {
            c.args(["-frames:v", "1"]);
        }
        self.raw_out(&mut c, size, zoom);
        c.stdin(Stdio::null()).stdout(Stdio::piped());
        c.spawn().map(Proc)
    }

    /// The desktop's notification sound (`sound`): libcanberra's player when installed (it
    /// follows the sound theme and its volume), otherwise the freedesktop sound file played
    /// by ffmpeg. Never waits.
    pub fn play_notification_sound(&self) {
        let canberra = Command::new("canberra-gtk-play")
            .args(["--id=message-new-instant", "--description=FrigatePiP"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let child = canberra.ok().or_else(|| {
            let sounds = ["message-new-instant.oga", "message.oga", "bell.oga"];
            let file = sounds.iter().map(|f| format!("/usr/share/sounds/freedesktop/stereo/{f}")).find(|p| std::path::Path::new(p).exists());
            let Some(file) = file else {
                crate::log!("No notification sound (install sound-theme-freedesktop or libcanberra-gtk3-module)");
                return None;
            };
            let mut c = self.command();
            c.args(["-nostdin", "-i", &file, "-vn", "-f", self.audio_out.0, self.audio_out.1]);
            c.stdin(Stdio::null()).stdout(Stdio::null()).spawn().ok()
        });
        if let Some(mut child) = child {
            let _ = std::thread::Builder::new().name("sound".into()).spawn(move || child.wait());
        }
    }

    /// Audio player fed through stdin; `input` = input format options, `volume` in percent.
    pub fn audio_pipe(&self, input: &[String], volume: u8) -> std::io::Result<Proc> {
        let mut c = self.command();
        c.args(["-flags", "low_delay", "-probesize", "32", "-analyzeduration", "0"]);
        c.args(input);
        c.args(["-i", "pipe:0", "-vn"]);
        if volume < 100 {
            c.args(["-af", &format!("volume={:.4}", crate::view::gain(volume))]);
        }
        c.args(["-f", self.audio_out.0, self.audio_out.1]);
        c.stdin(Stdio::piped()).stdout(Stdio::null());
        c.spawn().map(Proc)
    }

    /// Audio player with the wind filter: this ffmpeg only decodes, the samples go through
    /// the filter on a thread and then to a second ffmpeg that plays them. Fed through
    /// stdin like `audio_pipe`; when the returned process ends, so do the thread and the player.
    pub fn audio_pipe_filtered(&self, input: &[String], (rate, channels): (u32, u16), volume: u8, camera: &str) -> std::io::Result<Proc> {
        let (rate, channels) = (rate.max(1), channels.max(1));
        let pcm = ["-f".to_string(), "s16le".into(), "-ar".into(), rate.to_string(), "-ac".into(), channels.to_string()];
        let mut c = self.command();
        c.args(["-flags", "low_delay", "-probesize", "32", "-analyzeduration", "0"]);
        c.args(input);
        c.args(["-i", "pipe:0", "-vn"]);
        c.args(&pcm);
        c.args(["-flush_packets", "1", "pipe:1"]);
        c.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut decoder = c.spawn().map(Proc)?;
        let mut player = self.audio_pipe(&pcm, volume)?;
        if let (Some(mut samples), Some(mut out)) = (decoder.0.stdout.take(), player.0.stdin.take()) {
            let _ = std::thread::Builder::new().name(format!("wind-{camera}")).spawn(move || {
                use std::io::Write;
                let _player = player;
                let mut filter = crate::wind::WindFilter::new(rate, channels);
                let frame = channels as usize * 2;
                let mut bytes = vec![0u8; 4096];
                let mut have = 0;
                let mut pcm: Vec<i16> = Vec::new();
                loop {
                    match samples.read(&mut bytes[have..]) {
                        Ok(n) if n > 0 => have += n,
                        _ => return,
                    }
                    // Whole frames only; the rest waits for the next read.
                    let whole = have - have % frame;
                    pcm.clear();
                    pcm.extend(bytes[..whole].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
                    filter.process(&mut pcm);
                    for (b, s) in bytes[..whole].chunks_exact_mut(2).zip(&pcm) {
                        b.copy_from_slice(&s.to_le_bytes());
                    }
                    if out.write_all(&bytes[..whole]).is_err() {
                        return;
                    }
                    bytes.copy_within(whole..have, 0);
                    have -= whole;
                }
            });
        }
        Ok(decoder)
    }
}

/// The video filter: only the part `zoom` of the picture (cut at the stream's resolution),
/// scaled to `size`.
fn video_filter(size: (u32, u32), zoom: Zoom) -> String {
    let crop =
        if zoom.is_full() { String::new() } else { format!("crop=iw*{s:.6}:ih*{s:.6}:iw*{x:.6}:ih*{y:.6},", s = zoom.size, x = zoom.x, y = zoom.y) };
    format!("{crop}scale={}:{}:flags=bilinear", size.0, size.1)
}

/// Reads frames of `size` until the end of the stream or until `on_frame` returns `false`.
pub fn read_frames(mut out: ChildStdout, size: (u32, u32), mut on_frame: impl FnMut(Image) -> bool) {
    let n = (size.0 * size.1 * 4) as usize;
    loop {
        let mut data = vec![0u8; n];
        if out.read_exact(&mut data).is_err() {
            return;
        }
        if !on_frame(Image { w: size.0, h: size.1, data }) {
            return;
        }
    }
}

/// ADTS header for one raw AAC frame (ffmpeg reads AAC from a pipe only as ADTS).
/// `asc` = AudioSpecificConfig from the SDP.
pub fn adts_header(asc: &[u8], frame_len: usize) -> Option<[u8; 7]> {
    if asc.len() < 2 {
        return None;
    }
    let object = (asc[0] >> 3) as u32;
    let freq = (((asc[0] & 7) << 1) | (asc[1] >> 7)) as u32;
    let channels = ((asc[1] >> 3) & 0x0F) as u32;
    if !(1..=4).contains(&object) || freq > 12 {
        return None;
    }
    let len = (frame_len + 7) as u32;
    Some([
        0xFF,
        0xF1,
        (((object - 1) << 6) | (freq << 2) | (channels >> 2)) as u8,
        (((channels & 3) << 6) | (len >> 11)) as u8,
        ((len >> 3) & 0xFF) as u8,
        (((len & 7) << 5) | 0x1F) as u8,
        0xFC,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_filter_crops_before_scaling() {
        assert_eq!(video_filter((480, 270), Zoom::FULL), "scale=480:270:flags=bilinear");
        assert_eq!(
            video_filter((480, 270), Zoom { x: 0.25, y: 0.5, size: 0.5 }),
            "crop=iw*0.500000:ih*0.500000:iw*0.250000:ih*0.500000,scale=480:270:flags=bilinear"
        );
    }

    #[test]
    fn hwaccel_auto_means_the_main_stream_only() {
        assert_eq!(HwAccel::parse("AUTO"), HwAccel::Auto);
        assert_eq!(HwAccel::parse("off"), HwAccel::Off);
        assert_eq!(HwAccel::parse("on"), HwAccel::On);
        assert!(HwAccel::Auto.wanted(true) && !HwAccel::Auto.wanted(false));
        assert!(HwAccel::On.wanted(false) && !HwAccel::Off.wanted(true));
    }

    #[test]
    fn adts_header_matches_a_reolink_frame() {
        // AAC-LC, 16 kHz, mono (config 0x1408), frame length 519 = 7 + 512.
        assert_eq!(adts_header(&[0x14, 0x08], 512), Some([0xFF, 0xF1, 0x60, 0x40, 0x40, 0xFF, 0xFC]));
        assert_eq!(adts_header(&[0x14], 10), None);
    }
}
