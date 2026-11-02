//! One preview window: Frigate's snapshot at once, then live video — RTSP from go2rtc
//! (our client, decoded by ffmpeg), MJPEG from Frigate or refreshed snapshots.

use super::canvas::Image;
use super::ffmpeg::{adts_header, read_frames, Ffmpeg, Proc};
use crate::audio::{AudioCodec, AudioTrack};
use crate::config::LiveMode;
use crate::hub::{Feed, Hub};
use crate::rtp::{AccessUnit, Codec, VideoTrack};
use crate::rtsp::{self, Media};
use crate::util::Cancel;
use std::cell::RefCell;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// State shared between the window's threads and the screen loop.
#[derive(Default)]
pub struct Shared {
    pub frame: Mutex<Option<Image>>,
    /// A new frame (or caption) waits to be drawn.
    pub dirty: AtomicBool,
    /// The frame comes from live video (red dot).
    pub live: AtomicBool,
    /// Camera audio muted by the user (key m, sound button): its frames are not played.
    pub muted: AtomicBool,
    /// The camera audio is being played.
    pub audio: AtomicBool,
    pub cancel: Cancel,
}

pub struct Params {
    pub camera: String,
    pub frigate_url: String,
    pub rtsp_url: Option<String>,
    /// The camera's prebuffer (`prebuffer`), if it runs.
    pub hub: Option<Arc<Hub>>,
    /// `rtsp_url` is the camera's main stream (for `hwaccel = auto`).
    pub main: bool,
    pub live: LiveMode,
    /// Picture size in pixels.
    pub size: (u32, u32),
    pub audio: bool,
    pub t0: Instant,
}

type Wake = Arc<dyn Fn() + Send + Sync>;

pub fn start(p: Params, ff: Arc<Ffmpeg>, wake: Wake) -> Arc<Shared> {
    let shared = Arc::new(Shared::default());
    let p = Arc::new(p);
    // The snapshot runs on its own thread so it never delays the live video.
    {
        let (p, ff, shared, wake) = (p.clone(), ff.clone(), shared.clone(), wake.clone());
        let _ = std::thread::Builder::new().name(format!("snap-{}", p.camera)).spawn(move || snapshot(&p, &ff, &shared, &wake));
    }
    let s = shared.clone();
    let _ = std::thread::Builder::new().name(format!("live-{}", p.camera)).spawn(move || match p.live {
        LiveMode::Snapshot => {
            while !s.cancel.sleep(1000) {
                snapshot(&p, &ff, &s, &wake);
            }
        }
        LiveMode::Mjpeg => mjpeg(&p, &ff, &s, &wake),
        LiveMode::Rtsp => {
            if let Err(e) = rtsp_live(&p, &ff, &s, &wake) {
                if !s.cancel.is_set() {
                    crate::log!("{}: RTSP video unavailable ({e}) - MJPEG from Frigate instead", p.camera);
                    mjpeg(&p, &ff, &s, &wake);
                }
            }
        }
    });
    shared
}

fn store(shared: &Shared, img: Image, live: bool, wake: &Wake) {
    *shared.frame.lock().unwrap() = Some(img);
    shared.live.store(live, Ordering::Relaxed);
    shared.dirty.store(true, Ordering::Relaxed);
    wake();
}

/// Frigate's latest picture of the camera (shown until live video starts).
fn snapshot(p: &Params, ff: &Ffmpeg, shared: &Shared, wake: &Wake) {
    let url = format!("{}/api/{}/latest.jpg?h={}", p.frigate_url, p.camera, p.size.1.max(1));
    let Ok(mut proc) = ff.url(&url, None, p.size, true) else { return };
    let Some(out) = proc.0.stdout.take() else { return };
    read_frames(out, p.size, |img| {
        if !shared.cancel.is_set() && !shared.live.load(Ordering::Relaxed) {
            store(shared, img, false, wake);
        }
        false
    });
}

fn mjpeg(p: &Params, ff: &Ffmpeg, shared: &Shared, wake: &Wake) {
    let url = format!("{}/api/{}?fps=5&h={}", p.frigate_url, p.camera, p.size.1.max(1));
    let Ok(mut proc) = ff.url(&url, Some("mpjpeg"), p.size, false) else { return };
    let Some(out) = proc.0.stdout.take() else { return };
    let proc = Arc::new(Mutex::new(Some(proc)));
    kill_on_cancel(shared, proc.clone());
    let mut first = true;
    read_frames(out, p.size, |img| {
        if first {
            first = false;
            crate::log!("{}: MJPEG {}x{} after {} ms", p.camera, img.w, img.h, p.t0.elapsed().as_millis());
        }
        store(shared, img, true, wake);
        !shared.cancel.is_set()
    });
}

/// Kills a process as soon as the window closes (that also ends a blocked read).
fn kill_on_cancel(shared: &Shared, proc: Arc<Mutex<Option<Proc>>>) {
    let cancel = shared.cancel.clone();
    let _ = std::thread::Builder::new().name("ffmpeg-watch".into()).spawn(move || {
        while !cancel.sleep(200) {
            let done = proc.lock().unwrap().as_mut().is_none_or(|p| p.0.try_wait().ok().flatten().is_some());
            if done {
                return;
            }
        }
        proc.lock().unwrap().take();
    });
}

/// Access unit delimiters: written after each frame, they tell ffmpeg's parser at once
/// that the frame is complete (otherwise it waits for the start of the next one).
const AUD_H264: [u8; 6] = [0, 0, 0, 1, 0x09, 0xF0];
const AUD_HEVC: [u8; 7] = [0, 0, 0, 1, 0x46, 0x01, 0x50];

/// Who shows the picture when the prebuffer and a direct stream run side by side.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// The window's only video source.
    Only,
    /// The prebuffer (sub stream) while the wanted stream connects: no audio, it stops
    /// at the first frame of the other one.
    Bridge,
    /// The wanted stream next to the prebuffer: its first frame ends the bridge.
    Takeover,
}

/// Feeds one RTSP stream (direct or from the prebuffer) to the ffmpeg decoder and player.
struct Feeder<'a> {
    p: &'a Params,
    ff: &'a Ffmpeg,
    shared: &'a Arc<Shared>,
    wake: &'a Wake,
    role: Role,
    /// Set by the first frame of the `Takeover` stream.
    handover: Arc<AtomicBool>,
    /// For the log: where the stream comes from.
    via: &'static str,
    video: Option<(Arc<Mutex<Option<Proc>>>, std::process::ChildStdin)>,
    audio: Option<(Proc, std::process::ChildStdin)>,
    /// AAC: AudioSpecificConfig for the ADTS headers.
    asc: Option<Vec<u8>>,
    aud: &'static [u8],
    started: bool,
}

impl<'a> Feeder<'a> {
    fn new(p: &'a Params, ff: &'a Ffmpeg, shared: &'a Arc<Shared>, wake: &'a Wake, role: Role, handover: Arc<AtomicBool>, via: &'static str) -> Self {
        Feeder { p, ff, shared, wake, role, handover, via, video: None, audio: None, asc: None, aud: &[], started: false }
    }

    /// Still wanted: the window is open and (for the bridge) the other stream has no picture yet.
    fn alive(&self) -> bool {
        !self.shared.cancel.is_set() && !(self.role == Role::Bridge && self.handover.load(Ordering::Relaxed))
    }

    fn track(&mut self, track: &VideoTrack, audio: Option<&AudioTrack>) {
        let p = self.p;
        let format = match track.codec {
            Codec::H264 => "h264",
            Codec::H265 => "hevc",
        };
        self.aud = if track.codec == Codec::H264 { &AUD_H264 } else { &AUD_HEVC };
        crate::log!("{}: RTSP ready after {} ms ({format}){}", p.camera, p.t0.elapsed().as_millis(), self.via);
        // The bridge carries the prebuffer's sub stream, not the window's.
        let main = p.main && self.role != Role::Bridge;
        match self.ff.video_pipe(format, p.size, main) {
            Ok(mut proc) => {
                if let (Some(stdin), Some(out)) = (proc.0.stdin.take(), proc.0.stdout.take()) {
                    let proc = Arc::new(Mutex::new(Some(proc)));
                    kill_on_cancel(self.shared, proc.clone());
                    self.video = Some((proc, stdin));
                    let (size, cam, t0, via) = (p.size, p.camera.clone(), p.t0, self.via);
                    let (shared, wake, role, handover) = (self.shared.clone(), self.wake.clone(), self.role, self.handover.clone());
                    let _ = std::thread::Builder::new().name(format!("decode-{cam}")).spawn(move || {
                        let mut first = true;
                        read_frames(out, size, |img| {
                            if role == Role::Bridge && handover.load(Ordering::Relaxed) {
                                return false;
                            }
                            if first {
                                first = false;
                                crate::log!("{cam}: video {}x{} after {} ms{via}", img.w, img.h, t0.elapsed().as_millis());
                                if role == Role::Takeover {
                                    handover.store(true, Ordering::Relaxed);
                                }
                            }
                            store(&shared, img, true, &wake);
                            !shared.cancel.is_set()
                        });
                    });
                }
            }
            Err(e) => crate::log!("{}: cannot start ffmpeg: {e}", p.camera),
        }
        if let Some(a) = audio.filter(|_| self.role != Role::Bridge) {
            let input: Vec<String> = match &a.codec {
                AudioCodec::Aac { config, .. } => {
                    self.asc = Some(config.clone());
                    vec!["-f".into(), "aac".into()]
                }
                AudioCodec::Pcmu => vec!["-f".into(), "mulaw".into(), "-ar".into(), a.rate.to_string(), "-ac".into(), a.channels.to_string()],
                AudioCodec::Pcma => vec!["-f".into(), "alaw".into(), "-ar".into(), a.rate.to_string(), "-ac".into(), a.channels.to_string()],
            };
            match self.ff.audio_pipe(&input) {
                Ok(mut proc) => {
                    if let Some(stdin) = proc.0.stdin.take() {
                        crate::log!("{}: audio {} Hz", p.camera, a.rate);
                        self.audio = Some((proc, stdin));
                        self.shared.audio.store(true, Ordering::Relaxed);
                    }
                }
                Err(e) => crate::log!("{}: no audio: {e}", p.camera),
            }
        }
    }

    /// `false` = stop (window closed, decoder gone, or the bridge is no longer needed).
    fn video(&mut self, au: &AccessUnit) -> bool {
        if !self.started && !au.key {
            return self.alive();
        }
        if !self.started {
            crate::log!("{}: key frame after {} ms{}", self.p.camera, self.p.t0.elapsed().as_millis(), self.via);
        }
        self.started = true;
        let aud = self.aud;
        match self.video.as_mut() {
            Some((_, stdin)) => stdin.write_all(&au.data).and_then(|_| stdin.write_all(aud)).is_ok() && self.alive(),
            None => false,
        }
    }

    fn audio(&mut self, frame: &[u8]) -> bool {
        if self.shared.muted.load(Ordering::Relaxed) {
            return self.alive();
        }
        let header = self.asc.as_deref().map(|asc| adts_header(asc, frame.len()));
        if let Some((_, stdin)) = self.audio.as_mut() {
            let ok = match header {
                Some(Some(h)) => stdin.write_all(&h).and_then(|_| stdin.write_all(frame)).is_ok(),
                Some(None) => false,
                None => stdin.write_all(frame).is_ok(),
            };
            if !ok {
                self.audio = None;
            }
        }
        self.alive()
    }
}

/// Live RTSP video. With the prebuffer the picture is there at once: its stream alone when
/// the window wants it, otherwise it bridges the time until the wanted stream's first frame.
fn rtsp_live(p: &Arc<Params>, ff: &Arc<Ffmpeg>, shared: &Arc<Shared>, wake: &Wake) -> Result<(), String> {
    let Some(url) = p.rtsp_url.as_deref() else { return Err("no RTSP stream".into()) };
    let handover = Arc::new(AtomicBool::new(false));
    match &p.hub {
        Some(hub) if hub.url() == url => match prebuffered(hub, p, ff, shared, wake, Role::Only, handover.clone()) {
            Err(e) if !shared.cancel.is_set() => {
                crate::log!("{}: prebuffer: {e} - direct RTSP", p.camera);
                direct(url, p, ff, shared, wake, Role::Only, handover)
            }
            _ => Ok(()),
        },
        Some(hub) => {
            let bridge = {
                let (hub, p, ff, shared, wake, handover) = (hub.clone(), p.clone(), ff.clone(), shared.clone(), wake.clone(), handover.clone());
                std::thread::Builder::new()
                    .name(format!("bridge-{}", p.camera))
                    .spawn(move || prebuffered(&hub, &p, &ff, &shared, &wake, Role::Bridge, handover))
                    .ok()
            };
            let result = direct(url, p, ff, shared, wake, Role::Takeover, handover);
            // The wanted stream failed: the prebuffer's picture stays while it lasts. It may
            // still be on its way (the wanted stream can fail within milliseconds), so the
            // bridge decides — MJPEG only when it ends without a picture, never next to it.
            if result.is_err() {
                if let Some(b) = bridge {
                    let _ = b.join();
                }
                if shared.live.load(Ordering::Relaxed) {
                    return Ok(());
                }
            }
            result
        }
        None => direct(url, p, ff, shared, wake, Role::Only, handover),
    }
}

/// RTSP from go2rtc: our client receives the stream, ffmpeg decodes it.
fn direct(url: &str, p: &Params, ff: &Ffmpeg, shared: &Arc<Shared>, wake: &Wake, role: Role, handover: Arc<AtomicBool>) -> Result<(), String> {
    let feeder = RefCell::new(Feeder::new(p, ff, shared, wake, role, handover, ""));
    let result = rtsp::run(
        url,
        &shared.cancel,
        p.audio,
        |video, audio| feeder.borrow_mut().track(video, audio),
        |m| match m {
            Media::Video(au) => feeder.borrow_mut().video(&au),
            Media::Audio(frame) => feeder.borrow_mut().audio(&frame),
        },
    );
    match result {
        Err(e) if !feeder.into_inner().started => Err(e),
        _ => Ok(()),
    }
}

/// The stream from the prebuffer: its last GOP at once (decoded in a moment), then live.
/// `Err` when the prebuffer is down, now or later.
fn prebuffered(hub: &Hub, p: &Params, ff: &Ffmpeg, shared: &Arc<Shared>, wake: &Wake, role: Role, handover: Arc<AtomicBool>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    if !hub.subscribe(Box::new(move |feed| tx.send(feed).is_ok())) {
        return Err("not connected yet".into());
    }
    let mut feeder = Feeder::new(p, ff, shared, wake, role, handover, " (prebuffer)");
    while feeder.alive() {
        let more = match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Feed::Track(video, audio)) => {
                feeder.track(&video, audio.as_ref());
                true
            }
            Ok(Feed::Au(au)) => feeder.video(&au),
            Ok(Feed::Audio(frame)) => feeder.audio(&frame),
            Ok(Feed::Failed(e)) => return Err(e),
            Err(RecvTimeoutError::Timeout) => true,
            Err(RecvTimeoutError::Disconnected) => return Err("stopped".into()),
        };
        if !more {
            break;
        }
    }
    Ok(())
}
