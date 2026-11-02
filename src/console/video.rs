//! One preview window: Frigate's snapshot at once, then live video — RTSP from go2rtc
//! (our client, decoded by ffmpeg), MJPEG from Frigate or refreshed snapshots.

use super::canvas::Image;
use super::ffmpeg::{adts_header, read_frames, Ffmpeg, Proc};
use crate::audio::AudioCodec;
use crate::config::LiveMode;
use crate::rtp::Codec;
use crate::rtsp::{self, Media};
use crate::util::Cancel;
use std::cell::RefCell;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// State shared between the window's threads and the screen loop.
#[derive(Default)]
pub struct Shared {
    pub frame: Mutex<Option<Image>>,
    /// A new frame (or caption) waits to be drawn.
    pub dirty: AtomicBool,
    /// The frame comes from live video (red dot).
    pub live: AtomicBool,
    /// Camera audio muted by the user (key m): its frames are not played.
    pub muted: AtomicBool,
    pub cancel: Cancel,
}

pub struct Params {
    pub camera: String,
    pub frigate_url: String,
    pub rtsp_url: Option<String>,
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
            let result = match &p.rtsp_url {
                Some(url) => rtsp(url, &p, &ff, &s, &wake),
                None => Err("no RTSP stream".into()),
            };
            if let Err(e) = result {
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

#[derive(Default)]
struct Pipes {
    video: Option<(Arc<Mutex<Option<Proc>>>, std::process::ChildStdin)>,
    audio: Option<(Proc, std::process::ChildStdin)>,
    /// AAC: AudioSpecificConfig for the ADTS headers.
    asc: Option<Vec<u8>>,
    aud: &'static [u8],
    started: bool,
}

/// RTSP from go2rtc: our client receives the stream, ffmpeg decodes it.
fn rtsp(url: &str, p: &Params, ff: &Ffmpeg, shared: &Arc<Shared>, wake: &Wake) -> Result<(), String> {
    let pipes = RefCell::new(Pipes::default());
    let on_track = |track: &crate::rtp::VideoTrack, audio: Option<&crate::audio::AudioTrack>| {
        let mut pp = pipes.borrow_mut();
        let format = match track.codec {
            Codec::H264 => "h264",
            Codec::H265 => "hevc",
        };
        pp.aud = if track.codec == Codec::H264 { &AUD_H264 } else { &AUD_HEVC };
        crate::log!("{}: RTSP ready after {} ms ({format})", p.camera, p.t0.elapsed().as_millis());
        match ff.video_pipe(format, p.size) {
            Ok(mut proc) => {
                if let (Some(stdin), Some(out)) = (proc.0.stdin.take(), proc.0.stdout.take()) {
                    let proc = Arc::new(Mutex::new(Some(proc)));
                    kill_on_cancel(shared, proc.clone());
                    pp.video = Some((proc, stdin));
                    let (size, cam, t0, shared, wake) = (p.size, p.camera.clone(), p.t0, shared.clone(), wake.clone());
                    let _ = std::thread::Builder::new().name(format!("decode-{cam}")).spawn(move || {
                        let mut first = true;
                        read_frames(out, size, |img| {
                            if first {
                                first = false;
                                crate::log!("{cam}: video {}x{} after {} ms", img.w, img.h, t0.elapsed().as_millis());
                            }
                            store(&shared, img, true, &wake);
                            !shared.cancel.is_set()
                        });
                    });
                }
            }
            Err(e) => crate::log!("{}: cannot start ffmpeg: {e}", p.camera),
        }
        if let Some(a) = audio {
            let input: Vec<String> = match &a.codec {
                AudioCodec::Aac { config, .. } => {
                    pp.asc = Some(config.clone());
                    vec!["-f".into(), "aac".into()]
                }
                AudioCodec::Pcmu => vec!["-f".into(), "mulaw".into(), "-ar".into(), a.rate.to_string(), "-ac".into(), a.channels.to_string()],
                AudioCodec::Pcma => vec!["-f".into(), "alaw".into(), "-ar".into(), a.rate.to_string(), "-ac".into(), a.channels.to_string()],
            };
            match ff.audio_pipe(&input) {
                Ok(mut proc) => {
                    if let Some(stdin) = proc.0.stdin.take() {
                        crate::log!("{}: audio {} Hz", p.camera, a.rate);
                        pp.audio = Some((proc, stdin));
                    }
                }
                Err(e) => crate::log!("{}: no audio: {e}", p.camera),
            }
        }
    };
    let on_media = |m: Media| {
        let mut pp = pipes.borrow_mut();
        match m {
            Media::Video(au) => {
                if !pp.started && !au.key {
                    return !shared.cancel.is_set();
                }
                if !pp.started {
                    crate::log!("{}: key frame after {} ms", p.camera, p.t0.elapsed().as_millis());
                }
                pp.started = true;
                let aud = pp.aud;
                match pp.video.as_mut() {
                    Some((_, stdin)) => stdin.write_all(&au.data).and_then(|_| stdin.write_all(aud)).is_ok() && !shared.cancel.is_set(),
                    None => false,
                }
            }
            Media::Audio(_) if shared.muted.load(Ordering::Relaxed) => !shared.cancel.is_set(),
            Media::Audio(frame) => {
                let header = pp.asc.as_deref().map(|asc| adts_header(asc, frame.len()));
                if let Some((_, stdin)) = pp.audio.as_mut() {
                    let ok = match header {
                        Some(Some(h)) => stdin.write_all(&h).and_then(|_| stdin.write_all(&frame)).is_ok(),
                        Some(None) => false,
                        None => stdin.write_all(&frame).is_ok(),
                    };
                    if !ok {
                        pp.audio = None;
                    }
                }
                !shared.cancel.is_set()
            }
        }
    };
    let result = rtsp::run(url, &shared.cancel, p.audio, on_track, on_media);
    let pp = pipes.into_inner();
    let had_video = pp.started;
    drop(pp);
    match result {
        Err(e) if !had_video => Err(e),
        _ => Ok(()),
    }
}
