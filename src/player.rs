//! PiP window thread: an instant snapshot, then live video.

use crate::audio::{AudioMsg, AudioTrack, Sound};
use crate::config::LiveMode;
use crate::decoder::{Decoder, Frame};
use crate::hub::{Feed, Hub, Sink};
use crate::render::{Overlay, Renderer};
use crate::rtp::{AccessUnit, VideoTrack};
use crate::util::Cancel;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const WM_APP_ASPECT: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 10;
pub const WM_APP_PLAYER_DONE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 11;

/// Video source: the prebuffer (always sub) or direct RTSP.
pub const SRC_PREBUF: usize = 0;
pub const SRC_DIRECT: usize = 1;

pub enum Input {
    Jpeg(Vec<u8>),
    JpegFailed,
    Track(usize, VideoTrack, Option<AudioTrack>),
    Au(usize, Arc<AccessUnit>),
    Audio(usize, Arc<Vec<u8>>),
    LiveFailed(usize, String),
}

/// State shared with the UI thread.
pub struct Shared {
    pub cancel: Cancel,
    overlay: Mutex<String>,
    overlay_on: AtomicBool,
    /// Close button state, see `render::Overlay::close`.
    close_button: AtomicU8,
    dirty: AtomicBool,
    /// Camera audio of this window and its mute button.
    pub sound: Arc<Sound>,
}

impl Shared {
    pub fn new(text: String, overlay_on: bool) -> Arc<Shared> {
        Arc::new(Shared {
            cancel: Cancel::new(),
            overlay: Mutex::new(text),
            overlay_on: AtomicBool::new(overlay_on),
            close_button: AtomicU8::new(0),
            dirty: AtomicBool::new(false),
            sound: Arc::default(),
        })
    }

    pub fn set_overlay(&self, text: String) {
        *self.overlay.lock().unwrap() = text;
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Turns the on-video text overlay on / off (live, e.g. from the settings window).
    pub fn set_overlay_on(&self, on: bool) {
        if self.overlay_on.swap(on, Ordering::Relaxed) != on {
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    /// Shows / highlights / hides the close button (from the mouse position).
    pub fn set_close_button(&self, state: u8) {
        if self.close_button.swap(state, Ordering::Relaxed) != state {
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    pub fn request_redraw(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }
}

pub struct Params {
    pub hwnd: isize,
    pub camera: String,
    pub frigate_url: String,
    pub rtsp_url: Option<String>,
    pub hub: Option<Arc<Hub>>,
    pub live: LiveMode,
    /// Play audio from the camera.
    pub audio: bool,
    pub t0: Instant,
}

/// A window into which a frame can be drawn immediately via GDI until
/// the D3D11 renderer is ready.
struct QuickPaint {
    hwnd: isize,
    renderer_ready: Arc<AtomicBool>,
    camera: String,
    t0: Instant,
}

fn spawn_snapshot(url: String, tx: Sender<Input>, cancel: Cancel, quick: Option<QuickPaint>) {
    let _ = std::thread::Builder::new().name("snapshot".into()).spawn(move || {
        match crate::net::get_bytes(&url, Duration::from_secs(3), Some(&cancel), 16 << 20) {
            Ok(b) => {
                if let Some(q) = quick.filter(|q| !q.renderer_ready.load(Ordering::Acquire)) {
                    unsafe {
                        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    }
                    if crate::render::gdi_paint_jpeg(HWND(q.hwnd as _), &b).is_ok() {
                        crate::log!("{}: image (GDI) after {} ms", q.camera, q.t0.elapsed().as_millis());
                    }
                }
                let _ = tx.send(Input::Jpeg(b));
            }
            Err(e) => {
                if !cancel.is_set() {
                    crate::log!("Snapshot {url}: {e}");
                }
                let _ = tx.send(Input::JpegFailed);
            }
        }
    });
}

fn spawn_rtsp(url: String, audio: bool, tx: Sender<Input>, cancel: Cancel, camera: String, t0: Instant) {
    let _ = std::thread::Builder::new().name("rtsp".into()).spawn(move || {
        let r = crate::rtsp::run(
            &url,
            &cancel,
            audio,
            |v, a| {
                let extra = a.map(|a| format!(" + audio {} Hz", a.rate)).unwrap_or_default();
                crate::log!("{camera}: RTSP ready after {} ms ({}{extra})", t0.elapsed().as_millis(), v.codec.name());
                let _ = tx.send(Input::Track(SRC_DIRECT, v.clone(), a.cloned()));
            },
            |m| match m {
                crate::rtsp::Media::Video(au) => tx.send(Input::Au(SRC_DIRECT, Arc::new(au))).is_ok(),
                crate::rtsp::Media::Audio(f) => tx.send(Input::Audio(SRC_DIRECT, Arc::new(f))).is_ok(),
            },
        );
        if let Err(e) = r {
            if !cancel.is_set() {
                let _ = tx.send(Input::LiveFailed(SRC_DIRECT, e));
            }
        }
    });
}

/// Forwards the prebuffer to this window's input as source `SRC_PREBUF`.
fn prebuf_sink(tx: Sender<Input>) -> Sink {
    Box::new(move |feed| {
        let input = match feed {
            Feed::Track(v, a) => Input::Track(SRC_PREBUF, v, a),
            Feed::Au(au) => Input::Au(SRC_PREBUF, au),
            Feed::Audio(f) => Input::Audio(SRC_PREBUF, f),
            Feed::Failed(e) => Input::LiveFailed(SRC_PREBUF, e),
        };
        tx.send(input).is_ok()
    })
}

/// Finds consecutive JPEG images (SOI…EOI) in an MJPEG stream.
fn spawn_mjpeg(url: String, tx: Sender<Input>, cancel: Cancel) {
    let _ = std::thread::Builder::new().name("mjpeg".into()).spawn(move || {
        while !cancel.is_set() {
            match crate::net::get(&url, Duration::from_secs(5), Some(&cancel)) {
                Ok(mut resp) if resp.status == 200 => {
                    let mut buf: Vec<u8> = Vec::with_capacity(512 * 1024);
                    let mut chunk = vec![0u8; 64 * 1024];
                    let mut scan = 0;
                    loop {
                        let n = match resp.body.read(&mut chunk) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => n,
                        };
                        buf.extend_from_slice(&chunk[..n]);
                        loop {
                            let Some(soi) = buf.windows(2).position(|w| w == [0xFF, 0xD8]) else {
                                buf.clear();
                                scan = 0;
                                break;
                            };
                            let from = scan.max(soi + 2);
                            match buf[from..].windows(2).position(|w| w == [0xFF, 0xD9]) {
                                Some(e) => {
                                    let end = from + e + 2;
                                    if tx.send(Input::Jpeg(buf[soi..end].to_vec())).is_err() {
                                        return;
                                    }
                                    buf.drain(..end);
                                    scan = 0;
                                }
                                None => {
                                    scan = buf.len().saturating_sub(1);
                                    if buf.len() > 8 << 20 {
                                        buf.clear();
                                        scan = 0;
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                Ok(resp) => crate::log!("MJPEG {url}: HTTP {}", resp.status),
                Err(e) => {
                    if !cancel.is_set() {
                        crate::log!("MJPEG {url}: {e}");
                    }
                }
            }
            cancel.sleep(1000);
        }
    });
}

pub fn run(p: Params, shared: Arc<Shared>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    play(&p, &shared);
    shared.cancel.cancel();
    unsafe {
        let _ = PostMessageW(HWND(p.hwnd as _), WM_APP_PLAYER_DONE, WPARAM(0), LPARAM(0));
        CoUninitialize();
    }
}

fn post_aspect(hwnd: HWND, size: (u32, u32)) {
    unsafe {
        let _ = PostMessageW(hwnd, WM_APP_ASPECT, WPARAM(size.0 as usize), LPARAM(size.1 as isize));
    }
}

/// State of a single video source (prebuffer or direct RTSP).
#[derive(Default)]
struct Source {
    track: Option<VideoTrack>,
    dec: Option<Decoder>,
    got_key: bool,
    active: bool,
    failed: bool,
}

fn play(p: &Params, shared: &Arc<Shared>) {
    let hwnd = HWND(p.hwnd as _);
    let cancel = shared.cancel.clone();
    let cam = &p.camera;
    let (tx, rx) = mpsc::channel::<Input>();
    let snapshot_url = format!("{}/api/{}/latest.jpg", p.frigate_url, cam);
    let mjpeg_url = format!("{}/api/{}?fps=5&h=720", p.frigate_url, cam);

    let renderer_ready = Arc::new(AtomicBool::new(false));
    let quick = QuickPaint { hwnd: p.hwnd, renderer_ready: renderer_ready.clone(), camera: cam.clone(), t0: p.t0 };
    spawn_snapshot(snapshot_url.clone(), tx.clone(), cancel.clone(), Some(quick));
    // Audio lives on its own thread; here we only hand it frames without waiting.
    let audio_tx = p.audio.then(|| crate::audio::spawn(cam.clone(), cancel.clone(), shared.sound.clone()));

    let mut src: [Source; 2] = Default::default();
    let mut mjpeg_active = false;
    match p.live {
        // Without Media Foundation (Windows N) RTSP video cannot be decoded.
        LiveMode::Rtsp if !crate::decoder::mf_startup() => {
            crate::log!("{cam}: MJPEG from Frigate instead of RTSP");
            spawn_mjpeg(mjpeg_url.clone(), tx.clone(), cancel.clone());
            mjpeg_active = true;
        }
        LiveMode::Rtsp => {
            // The prebuffer (sub) gives a picture at once; if the window wants another stream
            // (main), we connect directly in parallel and switch over after the first frame.
            src[SRC_PREBUF].active = p.hub.as_ref().is_some_and(|h| h.subscribe(prebuf_sink(tx.clone())));
            let hub_is_target = src[SRC_PREBUF].active && p.hub.as_ref().map(|h| h.url()) == p.rtsp_url.as_deref();
            if !hub_is_target {
                if let Some(url) = &p.rtsp_url {
                    spawn_rtsp(url.clone(), p.audio, tx.clone(), cancel.clone(), cam.clone(), p.t0);
                    src[SRC_DIRECT].active = true;
                }
            }
            if !src[SRC_PREBUF].active && !src[SRC_DIRECT].active {
                spawn_mjpeg(mjpeg_url.clone(), tx.clone(), cancel.clone());
                mjpeg_active = true;
            }
        }
        LiveMode::Mjpeg => {
            spawn_mjpeg(mjpeg_url.clone(), tx.clone(), cancel.clone());
            mjpeg_active = true;
        }
        LiveMode::Snapshot => {}
    }

    let mut r = match Renderer::new(hwnd) {
        Ok(r) => r,
        Err(e) => {
            crate::log!("{cam}: cannot create renderer: {e}");
            return;
        }
    };
    renderer_ready.store(true, Ordering::Release);
    crate::log!("{cam}: renderer ready after {} ms", p.t0.elapsed().as_millis());
    let overlay_text = |live: bool| Overlay {
        text: if shared.overlay_on.load(Ordering::Relaxed) { shared.overlay.lock().unwrap().clone() } else { String::new() },
        live,
        close: shared.close_button.load(Ordering::Relaxed),
        sound: match (shared.sound.playing.load(Ordering::Relaxed), shared.sound.muted.load(Ordering::Relaxed)) {
            (false, _) => 0,
            (true, false) => 1,
            (true, true) => 2,
        },
    };
    let _ = r.redraw(&overlay_text(false));

    let mut force_sw = !r.video_ok;
    let mut shown: Option<usize> = None;
    let mut have_image = false;
    let mut snapshot_inflight = true;
    let mut last_snapshot = Instant::now();
    let mut aspect_sent: Option<(u32, u32)> = None;
    let names = ["prebuffer", "rtsp"];

    while !cancel.is_set() {
        let mut batch = Vec::new();
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(m) => batch.push(m),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        while let Ok(m) = rx.try_recv() {
            batch.push(m);
        }
        let mut new_frame: Option<(usize, Frame)> = None;
        let mut new_jpeg: Option<Vec<u8>> = None;
        for m in batch {
            match m {
                Input::Jpeg(j) => {
                    snapshot_inflight = false;
                    if shown.is_none() {
                        new_jpeg = Some(j);
                    }
                }
                Input::JpegFailed => snapshot_inflight = false,
                Input::Track(s, t, a) => {
                    src[s].dec = None;
                    src[s].got_key = false;
                    src[s].track = Some(t);
                    if let (Some(atx), Some(a)) = (&audio_tx, a) {
                        let _ = atx.send(AudioMsg::Track(s, a));
                    }
                }
                Input::Audio(s, f) => {
                    if let (Some(atx), true) = (&audio_tx, shown == Some(s)) {
                        let _ = atx.send(AudioMsg::Frame(s, f));
                    }
                }
                Input::Au(s, au) => {
                    // After switching to the direct stream, the prebuffer is ignored.
                    if shown.is_some_and(|c| s < c) {
                        continue;
                    }
                    let st = &mut src[s];
                    let Some(codec) = st.track.as_ref().map(|t| t.codec) else { continue };
                    if !st.got_key && !au.key {
                        continue;
                    }
                    if st.dec.is_none() {
                        let hw = if force_sw { None } else { Some(&r.device) };
                        let created = Decoder::new(codec, hw).or_else(|e| {
                            if hw.is_some() {
                                crate::log!("{cam}: hardware decoder unavailable ({e}), using software");
                                force_sw = true;
                                Decoder::new(codec, None)
                            } else {
                                Err(e)
                            }
                        });
                        match created {
                            Ok(d) => st.dec = Some(d),
                            Err(e) => {
                                crate::log!("{cam}: no decoder for {}: {e}", codec.name());
                                st.failed = true;
                                st.track = None;
                                continue;
                            }
                        }
                    }
                    st.got_key = true;
                    let d = st.dec.as_mut().unwrap();
                    if let Err(e) = d.decode(&au.data, &mut |f| new_frame = Some((s, f))) {
                        crate::log!("{cam}: decoding error ({}): {e}", if d.hw { "hardware" } else { "software" });
                        if d.hw {
                            force_sw = true;
                        }
                        st.dec = None;
                        st.got_key = false;
                    }
                }
                Input::LiveFailed(s, e) => {
                    crate::log!("{cam}: video ({}) unavailable: {e}", names[s]);
                    src[s].failed = true;
                }
            }
        }

        let alive = src.iter().any(|s| s.active && !s.failed);
        if shown.is_none() && !alive && !mjpeg_active && p.live != LiveMode::Snapshot {
            crate::log!("{cam}: no RTSP video - MJPEG from Frigate instead");
            spawn_mjpeg(mjpeg_url.clone(), tx.clone(), cancel.clone());
            mjpeg_active = true;
        }

        if let Some((s, f)) = new_frame {
            let gpu = matches!(f, Frame::Gpu { .. });
            match r.show_frame(f, &overlay_text(true)) {
                Ok(size) => {
                    if shown != Some(s) {
                        let kind = if gpu { "hardware" } else { "software" };
                        let codec = src[s].track.as_ref().map(|t| t.codec.name()).unwrap_or("?");
                        let what = if shown.is_none() { "video" } else { "switched to" };
                        crate::log!("{cam}: {what} {}x{} {codec} ({kind}, {}) after {} ms", size.0, size.1, names[s], p.t0.elapsed().as_millis());
                        shown = Some(s);
                        if let Some(atx) = &audio_tx {
                            let _ = atx.send(AudioMsg::Select(s));
                        }
                        if s == SRC_DIRECT {
                            src[SRC_PREBUF].dec = None;
                        }
                    }
                    if aspect_sent != Some(size) {
                        aspect_sent = Some(size);
                        post_aspect(hwnd, size);
                    }
                }
                Err(e) => {
                    crate::log!("{cam}: cannot show GPU frame ({e}), switching to software decoding");
                    force_sw = true;
                    for st in src.iter_mut() {
                        st.dec = None;
                        st.got_key = false;
                    }
                }
            }
        } else if let Some(j) = new_jpeg {
            match r.show_jpeg(&j, &overlay_text(mjpeg_active)) {
                Ok(size) => {
                    if !have_image {
                        have_image = true;
                        crate::log!("{cam}: image {}x{} after {} ms", size.0, size.1, p.t0.elapsed().as_millis());
                    }
                    if aspect_sent.is_none() {
                        aspect_sent = Some(size);
                        post_aspect(hwnd, size);
                    }
                }
                Err(e) => crate::log!("{cam}: bad JPEG: {e}"),
            }
        } else if shared.dirty.swap(false, Ordering::Relaxed) || r.check_resize().unwrap_or(false) {
            let _ = r.redraw(&overlay_text(shown.is_some() || mjpeg_active));
        }

        if shown.is_none() && !mjpeg_active && !snapshot_inflight && last_snapshot.elapsed() > Duration::from_millis(400) {
            snapshot_inflight = true;
            last_snapshot = Instant::now();
            spawn_snapshot(snapshot_url.clone(), tx.clone(), cancel.clone(), None);
        }
    }
    // Decoders are created before the renderer (they share the D3D11 device).
    drop(src);
}
