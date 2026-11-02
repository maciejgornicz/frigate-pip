//! Decoding with the `ffmpeg` program: our RTSP client feeds it H.264/H.265 through a
//! pipe (or ffmpeg reads Frigate's JPEG/MJPEG itself) and it returns raw BGRA frames
//! already scaled to the window. Audio is played by ffmpeg straight to PulseAudio
//! (PipeWire) or ALSA.

use super::canvas::Image;
use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};

pub struct Ffmpeg {
    path: String,
    /// `-fps_mode` (ffmpeg 5.1+) or `-vsync` (older): frames are passed on as they come.
    fps_flag: &'static str,
    /// Audio output: (format, device).
    audio_out: (&'static str, &'static str),
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
    pub fn find(path: &str) -> Result<Ffmpeg, String> {
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
        Ok(Ffmpeg { path: path.to_string(), fps_flag: if modern { "-fps_mode" } else { "-vsync" }, audio_out })
    }

    fn command(&self) -> Command {
        let mut c = Command::new(&self.path);
        // FRIGATEPIP_FFMPEG_LOG=file collects ffmpeg's messages (diagnostics).
        let log = std::env::var_os("FRIGATEPIP_FFMPEG_LOG").and_then(|p| std::fs::OpenOptions::new().create(true).append(true).open(p).ok());
        c.args(["-hide_banner", "-loglevel", if log.is_some() { "info" } else { "error" }]);
        c.stderr(log.map_or_else(Stdio::null, Stdio::from));
        c
    }

    /// Output options: raw BGRA frames of `size` on stdout, without delay.
    fn raw_out(&self, c: &mut Command, size: (u32, u32)) {
        let scale = format!("scale={}:{}:flags=bilinear", size.0, size.1);
        c.args(["-an", "-vf", &scale, "-pix_fmt", "bgra", self.fps_flag, "passthrough", "-flush_packets", "1", "-f", "rawvideo", "pipe:1"]);
    }

    /// Decoder fed through stdin with an elementary stream (`h264` or `hevc`).
    pub fn video_pipe(&self, format: &str, size: (u32, u32)) -> std::io::Result<Proc> {
        let mut c = self.command();
        // No "-fflags nobuffer": it drops what was read while probing — the first key frame.
        c.args(["-flags", "low_delay", "-probesize", "32", "-analyzeduration", "0", "-threads", "1"]);
        c.args(["-f", format, "-i", "pipe:0"]);
        self.raw_out(&mut c, size);
        c.stdin(Stdio::piped()).stdout(Stdio::piped());
        c.spawn().map(Proc)
    }

    /// ffmpeg reading a URL itself (Frigate's snapshot or MJPEG stream).
    pub fn url(&self, url: &str, format: Option<&str>, size: (u32, u32), one_frame: bool) -> std::io::Result<Proc> {
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
        self.raw_out(&mut c, size);
        c.stdin(Stdio::null()).stdout(Stdio::piped());
        c.spawn().map(Proc)
    }

    /// Audio player fed through stdin; `input` = input format options.
    pub fn audio_pipe(&self, input: &[String]) -> std::io::Result<Proc> {
        let mut c = self.command();
        c.args(["-flags", "low_delay", "-probesize", "32", "-analyzeduration", "0"]);
        c.args(input);
        c.args(["-i", "pipe:0", "-vn", "-f", self.audio_out.0, self.audio_out.1]);
        c.stdin(Stdio::piped()).stdout(Stdio::null());
        c.spawn().map(Proc)
    }
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
    fn adts_header_matches_a_reolink_frame() {
        // AAC-LC, 16 kHz, mono (config 0x1408), frame length 519 = 7 + 512.
        assert_eq!(adts_header(&[0x14, 0x08], 512), Some([0xFF, 0xF1, 0x60, 0x40, 0x40, 0xFF, 0xFC]));
        assert_eq!(adts_header(&[0x14], 10), None);
    }
}
