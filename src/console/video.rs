//! One preview window: Frigate's snapshot at once, then live video — RTSP from go2rtc
//! (our client, decoded by ffmpeg), MJPEG from Frigate or refreshed snapshots.

use super::canvas::Image;
use super::ffmpeg::{adts_header, read_frames, Ffmpeg, Proc};
use crate::audio::{AudioCodec, AudioTrack};
use crate::config::LiveMode;
use crate::history::Recorder;
use crate::hub::{Feed, Hub};
use crate::rtp::{AccessUnit, Codec, VideoTrack};
use crate::rtsp::{self, Media};
use crate::timing::Timing;
use crate::util::Cancel;
use crate::view::Zoom;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// State shared between the window's threads and the screen loop.
#[derive(Default)]
pub struct Shared {
    /// The newest picture and the part of the camera picture it shows.
    pub frame: Mutex<Option<(Image, Zoom)>>,
    /// A new frame (or caption) waits to be drawn.
    pub dirty: AtomicBool,
    /// The frame comes from live video (red dot).
    pub live: AtomicBool,
    /// Camera audio muted by the user (key m, sound button): its frames are not played.
    pub muted: AtomicBool,
    /// The camera audio is being played.
    pub audio: AtomicBool,
    /// Volume of the camera audio in percent.
    pub volume: AtomicU8,
    /// Replay: 0 = nothing to replay, 1 = possible, 2 = replaying.
    pub replay: AtomicU8,
    /// The replay button or key was pressed: start or stop a replay.
    pub replay_pressed: AtomicBool,
    /// How far behind live the replayed picture is, in milliseconds.
    pub behind: AtomicU32,
    pub cancel: Cancel,
}

pub struct Params {
    pub camera: String,
    pub frigate: crate::client::Frigate,
    pub rtsp_url: Option<String>,
    /// The camera's prebuffer (`prebuffer`), if it runs.
    pub hub: Option<Arc<Hub>>,
    /// `rtsp_url` is the camera's main stream (for `hwaccel = auto`).
    pub main: bool,
    pub live: LiveMode,
    /// Picture size in pixels.
    pub size: (u32, u32),
    /// The part of the camera picture to show (Ctrl + mouse wheel).
    pub zoom: Zoom,
    pub audio: bool,
    /// Volume of the camera audio in percent.
    pub volume: u8,
    pub t0: Instant,
    /// When the event came: a replay from the prebuffer starts before it.
    pub event: Instant,
    /// What the window recorded without the prebuffer (kept when the decoder restarts).
    pub own: Arc<Mutex<Recorder>>,
}

type Wake = Arc<dyn Fn() + Send + Sync>;

pub fn start(p: Params, ff: Arc<Ffmpeg>, wake: Wake) -> Arc<Shared> {
    let shared = Arc::new(Shared { volume: p.volume.into(), ..Default::default() });
    let p = Arc::new(p);
    // The snapshot runs on its own thread so it never delays the live video.
    {
        let (p, ff, shared, wake) = (p.clone(), ff.clone(), shared.clone(), wake.clone());
        let _ = std::thread::Builder::new().name(format!("snap-{}", p.camera)).spawn(move || snapshot(&p, &ff, &shared, &wake));
    }
    // Replays wait on their own thread for the button or key.
    {
        let (p, ff, shared, wake) = (p.clone(), ff.clone(), shared.clone(), wake.clone());
        let _ = std::thread::Builder::new().name(format!("replay-{}", p.camera)).spawn(move || {
            while !shared.cancel.sleep(100) {
                if shared.replay_pressed.swap(false, Ordering::Relaxed) {
                    replay(&p, &ff, &shared, &wake);
                }
            }
        });
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

/// A picture of the live video or of Frigate; none is shown while a replay runs.
fn store(shared: &Shared, img: Image, zoom: Zoom, live: bool, wake: &Wake) {
    if shared.replay.load(Ordering::Relaxed) != 2 {
        put(shared, img, zoom, live, wake);
    }
}

fn put(shared: &Shared, img: Image, zoom: Zoom, live: bool, wake: &Wake) {
    *shared.frame.lock().unwrap() = Some((img, zoom));
    shared.live.store(live, Ordering::Relaxed);
    shared.dirty.store(true, Ordering::Relaxed);
    wake();
}

/// ffmpeg decoding Frigate's picture or MJPEG stream at `path`. ffmpeg reads a plain
/// `http://` address itself; with HTTPS or a login FrigatePiP fetches it and pipes the bytes.
fn frigate_input(p: &Params, ff: &Ffmpeg, shared: &Shared, path: &str, format: &str, one_frame: bool) -> Option<Proc> {
    let api = &p.frigate;
    if !api.needs_client() {
        let f = (format != "jpeg_pipe").then_some(format);
        return ff.url(&api.url(path), f, p.size, one_frame, p.zoom).ok();
    }
    let mut body = match api.get(path, Duration::from_secs(5), Some(&shared.cancel)) {
        Ok(r) if r.status == 200 => r.body,
        Ok(r) => {
            crate::log!("{}: HTTP {} for {}", p.camera, r.status, api.url(path));
            return None;
        }
        Err(e) => {
            if !shared.cancel.is_set() {
                crate::log!("{}: {}: {e}", p.camera, api.url(path));
            }
            return None;
        }
    };
    let mut proc = ff.video_pipe(format, p.size, false, p.zoom).ok()?;
    let mut stdin = proc.0.stdin.take()?;
    let _ = std::thread::Builder::new().name(format!("http-{}", p.camera)).spawn(move || {
        let _ = std::io::copy(&mut body, &mut stdin);
    });
    Some(proc)
}

/// Frigate's latest picture of the camera (shown until live video starts).
fn snapshot(p: &Params, ff: &Ffmpeg, shared: &Shared, wake: &Wake) {
    let path = format!("/api/{}/latest.jpg?h={}", p.camera, source_height(p));
    let Some(mut proc) = frigate_input(p, ff, shared, &path, "jpeg_pipe", true) else { return };
    let Some(out) = proc.0.stdout.take() else { return };
    read_frames(out, p.size, |img| {
        if !shared.cancel.is_set() && !shared.live.load(Ordering::Relaxed) {
            store(shared, img, p.zoom, false, wake);
        }
        false
    });
}

fn mjpeg(p: &Params, ff: &Ffmpeg, shared: &Shared, wake: &Wake) {
    let path = format!("/api/{}?fps=5&h={}", p.camera, source_height(p));
    let Some(mut proc) = frigate_input(p, ff, shared, &path, "mpjpeg", false) else { return };
    let Some(out) = proc.0.stdout.take() else { return };
    let proc = Arc::new(Mutex::new(Some(proc)));
    kill_on_cancel(shared, proc.clone());
    let mut first = true;
    read_frames(out, p.size, |img| {
        if first {
            first = false;
            crate::log!("{}: MJPEG {}x{} after {} ms", p.camera, img.w, img.h, p.t0.elapsed().as_millis());
        }
        store(shared, img, p.zoom, true, wake);
        !shared.cancel.is_set()
    });
}

/// Height of the pictures to ask Frigate for: more when zoomed in, so that the part still
/// has detail (Frigate does not go beyond its own resolution).
fn source_height(p: &Params) -> u32 {
    ((p.size.1.max(1) as f32 / p.zoom.size).round() as u32).min(2160)
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

fn aud(codec: Codec) -> &'static [u8] {
    match codec {
        Codec::H264 => &AUD_H264,
        Codec::H265 => &AUD_HEVC,
    }
}

/// ffmpeg's name of the video format.
fn format(codec: Codec) -> &'static str {
    match codec {
        Codec::H264 => "h264",
        Codec::H265 => "hevc",
    }
}

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

/// The timing of a window's video and when the frames now inside ffmpeg were received.
#[derive(Default)]
struct Measure {
    timing: Timing,
    in_ffmpeg: VecDeque<Instant>,
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
    /// Input format options of the audio player and the volume it plays at.
    audio_input: Vec<String>,
    played_volume: u8,
    /// AAC: AudioSpecificConfig for the ADTS headers.
    asc: Option<Vec<u8>>,
    aud: &'static [u8],
    started: bool,
    measure: Arc<Mutex<Measure>>,
}

impl Drop for Feeder<'_> {
    fn drop(&mut self) {
        if let Some(line) = self.measure.lock().unwrap().timing.summary() {
            crate::log!("{}: timing{} - {line}", self.p.camera, self.via);
        }
    }
}

impl<'a> Feeder<'a> {
    fn new(p: &'a Params, ff: &'a Ffmpeg, shared: &'a Arc<Shared>, wake: &'a Wake, role: Role, handover: Arc<AtomicBool>, via: &'static str) -> Self {
        Feeder {
            p,
            ff,
            shared,
            wake,
            role,
            handover,
            via,
            video: None,
            audio: None,
            audio_input: Vec::new(),
            played_volume: 100,
            asc: None,
            aud: &[],
            started: false,
            measure: Arc::default(),
        }
    }

    /// Still wanted: the window is open and (for the bridge) the other stream has no picture yet.
    fn alive(&self) -> bool {
        !self.shared.cancel.is_set() && !(self.role == Role::Bridge && self.handover.load(Ordering::Relaxed))
    }

    fn track(&mut self, track: &VideoTrack, audio: Option<&AudioTrack>) {
        let p = self.p;
        let format = format(track.codec);
        self.aud = aud(track.codec);
        if p.hub.is_none() {
            p.own.lock().unwrap().track(track);
        }
        crate::log!("{}: RTSP ready after {} ms ({format}){}", p.camera, p.t0.elapsed().as_millis(), self.via);
        // The bridge carries the prebuffer's sub stream, not the window's.
        let main = p.main && self.role != Role::Bridge;
        match self.ff.video_pipe(format, p.size, main, p.zoom) {
            Ok(mut proc) => {
                if let (Some(stdin), Some(out)) = (proc.0.stdin.take(), proc.0.stdout.take()) {
                    let proc = Arc::new(Mutex::new(Some(proc)));
                    kill_on_cancel(self.shared, proc.clone());
                    self.video = Some((proc, stdin));
                    let (size, zoom, cam, t0, via) = (p.size, p.zoom, p.camera.clone(), p.t0, self.via);
                    let (shared, wake, role, handover) = (self.shared.clone(), self.wake.clone(), self.role, self.handover.clone());
                    let measure = self.measure.clone();
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
                            let mut m = measure.lock().unwrap();
                            if let Some(arrived) = m.in_ffmpeg.pop_front() {
                                m.timing.shown(arrived, Instant::now());
                            }
                            drop(m);
                            store(&shared, img, zoom, true, &wake);
                            !shared.cancel.is_set()
                        });
                    });
                }
            }
            Err(e) => crate::log!("{}: cannot start ffmpeg: {e}", p.camera),
        }
        if let Some(a) = audio.filter(|_| self.role != Role::Bridge) {
            self.audio_input = match &a.codec {
                AudioCodec::Aac { config, .. } => {
                    self.asc = Some(config.clone());
                    vec!["-f".into(), "aac".into()]
                }
                AudioCodec::Pcmu => vec!["-f".into(), "mulaw".into(), "-ar".into(), a.rate.to_string(), "-ac".into(), a.channels.to_string()],
                AudioCodec::Pcma => vec!["-f".into(), "alaw".into(), "-ar".into(), a.rate.to_string(), "-ac".into(), a.channels.to_string()],
            };
            self.start_audio();
            if self.audio.is_some() {
                crate::log!("{}: audio {} Hz", p.camera, a.rate);
                self.shared.audio.store(true, Ordering::Relaxed);
            }
        }
    }

    /// The audio player, at the window's volume (the old one ends first: both would play).
    fn start_audio(&mut self) {
        self.audio = None;
        self.played_volume = self.shared.volume.load(Ordering::Relaxed);
        match self.ff.audio_pipe(&self.audio_input, self.played_volume) {
            Ok(mut proc) => {
                if let Some(stdin) = proc.0.stdin.take() {
                    self.audio = Some((proc, stdin));
                }
            }
            Err(e) => crate::log!("{}: no audio: {e}", self.p.camera),
        }
    }

    /// `false` = stop (window closed, decoder gone, or the bridge is no longer needed).
    fn video(&mut self, au: Arc<AccessUnit>) -> bool {
        if !self.started && !au.key {
            return self.alive();
        }
        if !self.started {
            crate::log!("{}: key frame after {} ms{}", self.p.camera, self.p.t0.elapsed().as_millis(), self.via);
            // Live video is kept from now on (prebuffer or the window): a replay is possible.
            let _ = self.shared.replay.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
        }
        self.started = true;
        if self.p.hub.is_none() {
            self.p.own.lock().unwrap().push(au.clone());
        }
        let mut m = self.measure.lock().unwrap();
        m.timing.received(au.arrived, au.late);
        m.in_ffmpeg.push_back(au.arrived);
        drop(m);
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
        // ffmpeg cannot change the volume while it plays.
        if self.audio.is_some() && self.shared.volume.load(Ordering::Relaxed) != self.played_volume {
            self.start_audio();
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
            // bridge decides: its end is the result — MJPEG follows only when the prebuffer
            // fails (at once or later), never next to it.
            match (result, bridge) {
                (Err(_), Some(b)) => b.join().unwrap_or_else(|_| Err("prebuffer bridge stopped".into())),
                (result, _) => result,
            }
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
            Media::Video(au) => feeder.borrow_mut().video(Arc::new(au)),
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
            Ok(Feed::Au(au)) => feeder.video(au),
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

/// Plays the kept video again at its own pace (replay button, key r), then live video comes
/// back: it went on decoding meanwhile, unseen. Another press stops the replay.
fn replay(p: &Params, ff: &Ffmpeg, shared: &Arc<Shared>, wake: &Wake) {
    // With the prebuffer from before the event, otherwise from the window's opening.
    let material = match &p.hub {
        Some(hub) => hub.replay_from(crate::replay::start(p.event)),
        None => p.own.lock().unwrap().replay(),
    };
    let Some((track, units)) = material else {
        crate::log!("{}: nothing to replay yet", p.camera);
        return;
    };
    // The prebuffer carries the sub stream, the window's own recording its stream.
    let main = p.main && p.hub.is_none();
    let mut proc = match ff.video_pipe(format(track.codec), p.size, main, p.zoom) {
        Ok(proc) => proc,
        Err(e) => {
            crate::log!("{}: cannot start ffmpeg for the replay: {e}", p.camera);
            return;
        }
    };
    let (Some(mut stdin), Some(out)) = (proc.0.stdin.take(), proc.0.stdout.take()) else { return };
    let proc = Arc::new(Mutex::new(Some(proc)));
    kill_on_cancel(shared, proc.clone());
    crate::log!("{}: replay, {}", p.camera, crate::replay::describe(&units, Instant::now()));
    shared.replay.store(2, Ordering::Relaxed);
    // When the frames now inside ffmpeg were received live (how far behind the picture is).
    let sent: Arc<Mutex<VecDeque<Instant>>> = Arc::default();
    let decoder = {
        let (size, zoom, shared, wake, sent) = (p.size, p.zoom, shared.clone(), wake.clone(), sent.clone());
        std::thread::Builder::new()
            .name(format!("replay-decode-{}", p.camera))
            .spawn(move || {
                read_frames(out, size, |img| {
                    if let Some(arrived) = sent.lock().unwrap().pop_front() {
                        shared.behind.store(arrived.elapsed().as_millis() as u32, Ordering::Relaxed);
                    }
                    if shared.replay.load(Ordering::Relaxed) == 2 {
                        put(&shared, img, zoom, false, &wake);
                    }
                    !shared.cancel.is_set()
                });
            })
            .ok()
    };
    let aud = aud(track.codec);
    let mut due = Instant::now();
    let mut prev: Option<u32> = None;
    let mut stopped = false;
    for au in &units {
        if let Some(before) = prev {
            due += crate::replay::gap(before, au.ts);
        }
        prev = Some(au.ts);
        let wait = due.saturating_duration_since(Instant::now()).as_millis() as u64;
        if shared.cancel.sleep(wait) || shared.replay_pressed.swap(false, Ordering::Relaxed) {
            stopped = true;
            break;
        }
        sent.lock().unwrap().push_back(au.arrived);
        if stdin.write_all(&au.data).and_then(|_| stdin.write_all(aud)).is_err() {
            break;
        }
    }
    drop(stdin);
    if stopped {
        // At once: the frames still inside ffmpeg are not shown.
        shared.replay.store(1, Ordering::Relaxed);
        proc.lock().unwrap().take();
    }
    // At the end ffmpeg shows its last frames, then stops.
    if let Some(d) = decoder {
        let _ = d.join();
    }
    shared.replay.store(1, Ordering::Relaxed);
    crate::log!("{}: replay {}", p.camera, if stopped { "stopped" } else { "done" });
    shared.dirty.store(true, Ordering::Relaxed);
    wake();
}
