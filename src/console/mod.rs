//! Linux console version: preview windows drawn on the framebuffer (a console without
//! X or Wayland, e.g. a Raspberry Pi with a monitor) or inside a terminal (Kitty
//! graphics, Sixel or coloured half blocks). Video is decoded by the `ffmpeg` program.

mod canvas;
mod fb;
mod ffmpeg;
mod term;
mod video;

use crate::config::{self, Config, Corner, IniDoc};
use crate::filter::Detection;
use crate::frigate::{self, CameraInfo, Notice};
use crate::i18n::{self, t};
use crate::util::Cancel;
use crate::view::{choose_slot, choose_stream, Slot};
use canvas::Image;
use ffmpeg::Ffmpeg;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Where the windows are drawn.
pub trait Output {
    /// Usable area in pixels.
    fn size(&self) -> (u32, u32);
    /// Height of the caption line above the picture (terminal); 0 = drawn on the picture.
    fn caption_height(&self) -> u32 {
        0
    }
    /// Draws window `id`: the picture, the caption (empty = none) and the live dot.
    fn draw(&mut self, id: u64, rect: Rect, img: &Image, caption: &str, live: bool);
    /// Removes window `id` from the screen.
    fn clear(&mut self, id: u64);
    /// Status line (terminal only).
    fn status(&mut self, _text: &str) {}
    /// Re-reads the screen size; `true` if it changed (then everything is redrawn).
    fn refresh(&mut self) -> bool {
        false
    }
    /// Shortest time between two pictures of one window (terminals are slow).
    fn min_interval(&self) -> Duration {
        Duration::ZERO
    }
}

/// Settings of the console version (`[console]` in config.ini, overridable on the command line).
struct ConsoleCfg {
    output: String,
    graphics: String,
    framebuffer: String,
    framebuffer_size: Option<String>,
    ffmpeg: String,
    font: Option<String>,
}

impl ConsoleCfg {
    fn read(doc: &IniDoc, args: &[String]) -> ConsoleCfg {
        let get = |key: &str, default: &str| {
            arg(args, &format!("--{}", key.replace('_', "-")))
                .or_else(|| doc.get("console", key))
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        ConsoleCfg {
            output: get("output", "auto"),
            graphics: get("graphics", "auto"),
            framebuffer: get("framebuffer", "/dev/fb0"),
            framebuffer_size: Some(get("framebuffer_size", "")).filter(|v| !v.is_empty()),
            ffmpeg: get("ffmpeg", "ffmpeg"),
            font: Some(get("font", "")).filter(|v| !v.is_empty()),
        }
    }
}

/// Value after a command-line option (`--name value` or `--name=value`).
fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().enumerate().find_map(|(i, a)| {
        if a == name {
            args.get(i + 1).cloned()
        } else {
            a.strip_prefix(name).and_then(|v| v.strip_prefix('=')).map(str::to_string)
        }
    })
}

const HELP: &str = "FrigatePiP for the Linux console: instant picture-in-picture camera windows for Frigate NVR.

Usage: frigate-pip [options]

  --output auto|framebuffer|terminal   where to draw (default: auto)
  --graphics auto|kitty|sixel|blocks   pictures in a terminal (default: auto)
  --framebuffer /dev/fb0               framebuffer device
  --test CAMERA                        show a camera right after the start
  --test-connection                    check the connection to Frigate and exit
  --version                            print the version
  --help                               this help

Keys (when started in a terminal or on a console): q quit, x close windows,
p pause / resume, 1-9 show camera 1-9.
Settings: ~/.config/frigate-pip/config.ini - user guide:
https://github.com/maciejgornicz/frigate-pip/blob/main/docs/en/user-guide.md";

enum Ev {
    Notice(Notice),
    Key(u8),
    /// A window has a new picture.
    Wake,
}

static QUIT: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    QUIT.store(true, Ordering::Relaxed);
}

fn is_tty(fd: libc::c_int) -> bool {
    unsafe { libc::isatty(fd) == 1 }
}

pub fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("FrigatePiP {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // Without a terminal (e.g. a systemd service) the log also goes to stderr (the journal).
    crate::util::init_log(!is_tty(libc::STDOUT_FILENO));
    crate::log!("FrigatePiP {} (Linux console) - start", env!("CARGO_PKG_VERSION"));
    let path = config::config_path();
    let first_start = !path.exists();
    let (cfg, doc) = config::load(&path).unwrap_or_else(|e| {
        crate::log!("Cannot read {}: {e} - using defaults", path.display());
        let doc = IniDoc::parse(config::DEFAULT_INI);
        (Config::from_ini(&doc), doc)
    });
    i18n::set(i18n::resolve(&cfg.language));
    if first_start {
        eprintln!("FrigatePiP: created {} - set the Frigate address (url) there.", path.display());
    }
    if args.iter().any(|a| a == "--test-connection") {
        frigate::connection_test(&cfg, &Cancel::new(), &mut |line| println!("{line}"));
        return;
    }
    let console = ConsoleCfg::read(&doc, &args);
    let ff = match Ffmpeg::find(&console.ffmpeg) {
        Ok(f) => Arc::new(f),
        Err(e) => {
            eprintln!("FrigatePiP: {e}\nffmpeg is needed for video - install it, e.g.: sudo apt install ffmpeg");
            std::process::exit(1);
        }
    };
    let (out, terminal) = match open_output(&console) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("FrigatePiP: {e}");
            std::process::exit(1);
        }
    };
    unsafe {
        for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(sig, on_signal as *const () as libc::sighandler_t);
        }
    }
    let (tx, rx) = mpsc::channel();
    let keys = is_tty(libc::STDIN_FILENO).then(|| {
        let tx = tx.clone();
        // The terminal output switches the terminal to raw mode itself.
        term::Keys::start(!terminal, move |k| {
            let _ = tx.send(Ev::Key(k));
        })
    });
    let mut app = App::new(cfg, path, out, ff, tx);
    app.pending_test = arg(&args, "--test");
    app.start_listener();
    app.run(rx);
    drop(app);
    drop(keys);
    crate::log!("Stopped");
}

/// Framebuffer on a console without graphics, terminal otherwise; `true` = terminal.
fn open_output(c: &ConsoleCfg) -> Result<(Box<dyn Output>, bool), String> {
    let on_console = std::env::var("TERM").is_ok_and(|t| t == "linux") || !is_tty(libc::STDOUT_FILENO);
    let framebuffer = match c.output.as_str() {
        "framebuffer" | "fb" => true,
        "terminal" => false,
        _ => on_console && std::path::Path::new(&c.framebuffer).exists(),
    };
    if framebuffer {
        let font = canvas::Font::load(c.font.as_deref());
        let fb = fb::Framebuffer::open(&c.framebuffer, c.framebuffer_size.as_deref(), font)?;
        Ok((Box::new(fb), false))
    } else {
        Ok((Box::new(term::Terminal::open(&c.graphics)?), true))
    }
}

struct Win {
    id: u64,
    camera: String,
    caption: String,
    deadline: Instant,
    hard: Instant,
    shared: Arc<video::Shared>,
    rect: Option<Rect>,
    /// Picture size the decoder was started with.
    size: (u32, u32),
    last_draw: Option<Instant>,
}

struct App {
    cfg: Config,
    path: PathBuf,
    modified: Option<SystemTime>,
    out: Box<dyn Output>,
    ff: Arc<Ffmpeg>,
    tx: Sender<Ev>,
    cameras: Vec<CameraInfo>,
    wins: Vec<Win>,
    next_id: u64,
    paused: bool,
    connected: bool,
    status: String,
    listener: Cancel,
    pending_test: Option<String>,
    started: Instant,
}

impl App {
    fn new(cfg: Config, path: PathBuf, out: Box<dyn Output>, ff: Arc<Ffmpeg>, tx: Sender<Ev>) -> App {
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        App {
            cfg,
            path,
            modified,
            out,
            ff,
            tx,
            cameras: Vec::new(),
            wins: Vec::new(),
            next_id: 1,
            paused: false,
            connected: false,
            status: t().status_connecting.to_string(),
            listener: Cancel::new(),
            pending_test: None,
            started: Instant::now(),
        }
    }

    fn start_listener(&mut self) {
        self.listener.cancel();
        self.listener = Cancel::new();
        self.connected = false;
        self.status = t().status_connecting.to_string();
        let tx = self.tx.clone();
        frigate::start(self.cfg.clone(), self.listener.clone(), move |n| {
            let _ = tx.send(Ev::Notice(n));
        });
    }

    fn run(&mut self, rx: mpsc::Receiver<Ev>) {
        let mut last_tick = Instant::now() - Duration::from_secs(10);
        let mut last_reload = Instant::now();
        self.show_status();
        while !QUIT.load(Ordering::Relaxed) {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ev::Notice(n)) => self.on_notice(n),
                Ok(Ev::Key(k)) => {
                    if !self.on_key(k) {
                        break;
                    }
                }
                Ok(Ev::Wake) => {}
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.draw();
            if last_tick.elapsed() >= Duration::from_millis(500) {
                last_tick = Instant::now();
                self.tick();
            }
            if last_reload.elapsed() >= Duration::from_secs(2) {
                last_reload = Instant::now();
                self.reload_if_changed();
            }
        }
        for i in (0..self.wins.len()).rev() {
            self.close(i);
        }
        self.listener.cancel();
    }

    fn on_notice(&mut self, n: Notice) {
        match n {
            Notice::Connected(url) => {
                self.connected = true;
                self.status = url;
                self.show_status();
            }
            Notice::Disconnected(why) => {
                self.connected = false;
                self.status = why;
                self.show_status();
            }
            Notice::Cameras(list) => {
                self.cameras = list;
                if let Some(cam) = self.pending_test.take() {
                    self.test(&cam);
                }
            }
            Notice::Show(d, t0) => self.on_detection(d, t0, false),
            Notice::Activity(cam) => {
                let now = Instant::now();
                let dur = Duration::from_secs(self.cfg.duration as u64);
                for w in self.wins.iter_mut().filter(|w| w.camera == cam) {
                    w.deadline = w.deadline.max(now + dur).min(w.hard);
                }
            }
        }
    }

    /// A camera at once (`--test`, number keys): like a detection, also while paused.
    fn test(&mut self, name: &str) {
        let camera = self
            .cameras
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name) || c.title.eq_ignore_ascii_case(name))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| name.to_string());
        let d = Detection { camera, label: "test".into(), sub_label: None, score: 0.0, id: String::new() };
        self.on_detection(d, Instant::now(), true);
    }

    fn on_detection(&mut self, d: Detection, t0: Instant, test: bool) {
        if self.paused && !test {
            crate::log!("{}: skipped (paused)", d.camera);
            return;
        }
        let info = self.cameras.iter().find(|c| c.name == d.camera).cloned();
        let title = info.as_ref().map(|c| c.title.clone()).unwrap_or_else(|| d.camera.clone());
        let caption = crate::view::caption(&title, &d);
        let now = Instant::now();
        let dur = Duration::from_secs(self.cfg.duration as u64);
        let max = Duration::from_secs(self.cfg.max_duration.max(self.cfg.duration) as u64);
        if let Some(w) = self.wins.iter_mut().find(|w| w.camera == d.camera) {
            w.caption = caption;
            w.hard = now + max;
            w.deadline = w.deadline.max(now + dur).min(w.hard);
            w.shared.dirty.store(true, Ordering::Relaxed);
            return;
        }
        let states: Vec<(bool, bool)> = self.wins.iter().map(|_| (false, false)).collect();
        let replace_at = match choose_slot(&states, self.cfg.max_popups, self.cfg.replace) {
            Slot::New => None,
            Slot::Skip => {
                crate::log!("{}: skipped (all windows in use)", d.camera);
                return;
            }
            Slot::Replace(i) => Some(i),
        };
        let aspect = info.as_ref().map(|c| c.aspect).filter(|a| *a > 0.1).unwrap_or(16.0 / 9.0);
        let size = self.picture_size(aspect);
        let stream = choose_stream(&self.cfg, info.as_ref(), &d.camera, size.0 as i32);
        let params = video::Params {
            camera: d.camera.clone(),
            frigate_url: self.cfg.url.clone(),
            rtsp_url: stream.map(|s| self.cfg.rtsp_url(&s)),
            live: self.cfg.live,
            size,
            audio: self.cfg.camera_audio,
            t0,
        };
        let tx = self.tx.clone();
        let shared = video::start(
            params,
            self.ff.clone(),
            Arc::new(move || {
                let _ = tx.send(Ev::Wake);
            }),
        );
        crate::log!("{}: window after {} ms", d.camera, t0.elapsed().as_millis());
        let win =
            Win { id: self.next_id, camera: d.camera, caption, deadline: now + dur, hard: now + max, shared, rect: None, size, last_draw: None };
        self.next_id += 1;
        match replace_at {
            Some(i) => {
                crate::log!("{}: replaces window {}", win.camera, self.wins[i].camera);
                self.close_quiet(i);
                self.wins.insert(i, win);
            }
            None => self.wins.push(win),
        }
        self.layout();
    }

    /// Picture size for a camera with this aspect ratio, within the screen.
    fn picture_size(&self, aspect: f32) -> (u32, u32) {
        let (sw, sh) = self.out.size();
        let margin = self.cfg.margin.max(0) as u32;
        let w = (self.cfg.width.max(64) as u32).min(sw.saturating_sub(2 * margin).max(64));
        let mut h = (w as f32 / aspect).round() as u32;
        let max_h = sh.saturating_sub(2 * margin + self.out.caption_height()).max(32);
        let mut w = w;
        if h > max_h {
            h = max_h;
            w = (h as f32 * aspect).round() as u32;
        }
        (w & !1, h & !1)
    }

    /// Places the windows from the chosen corner, oldest first.
    fn layout(&mut self) {
        let caption_h = self.out.caption_height();
        let sizes: Vec<(u32, u32)> = self.wins.iter().map(|w| (w.size.0, w.size.1 + caption_h)).collect();
        let rects = stack(self.out.size(), self.cfg.position, self.cfg.margin.max(0) as u32, &sizes);
        for (i, rect) in rects.into_iter().enumerate() {
            if self.wins[i].rect != rect {
                if rect.is_none() {
                    self.out.clear(self.wins[i].id);
                }
                self.wins[i].rect = rect;
                self.wins[i].shared.dirty.store(true, Ordering::Relaxed);
            }
        }
    }

    fn draw(&mut self) {
        let caption_h = self.out.caption_height();
        let min_gap = self.out.min_interval();
        for w in &mut self.wins {
            let Some(rect) = w.rect else { continue };
            if w.last_draw.is_some_and(|t| t.elapsed() < min_gap) || !w.shared.dirty.swap(false, Ordering::Relaxed) {
                continue;
            }
            let Some(frame) = w.shared.frame.lock().unwrap().clone() else {
                continue;
            };
            let caption = if self.cfg.overlay || caption_h > 0 { w.caption.as_str() } else { "" };
            self.out.draw(w.id, rect, &frame, caption, w.shared.live.load(Ordering::Relaxed));
            w.last_draw = Some(Instant::now());
        }
        // A throttled picture waits for the next frame or the next loop pass (≤ 100 ms).
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let expired: Vec<usize> = (0..self.wins.len()).filter(|&i| now >= self.wins[i].deadline).collect();
        for i in expired.into_iter().rev() {
            self.close(i);
        }
        if self.out.refresh() {
            for w in &mut self.wins {
                w.rect = None;
            }
            self.layout();
        }
        if self.pending_test.is_some() && self.started.elapsed() > Duration::from_secs(3) {
            let cam = self.pending_test.take().unwrap_or_default();
            self.test(&cam);
        }
        self.show_status();
    }

    fn close_quiet(&mut self, i: usize) {
        let w = self.wins.remove(i);
        w.shared.cancel.cancel();
        self.out.clear(w.id);
        crate::log!("{}: closed", w.camera);
    }

    fn close(&mut self, i: usize) {
        self.close_quiet(i);
        self.layout();
    }

    /// `false` = quit.
    fn on_key(&mut self, k: u8) -> bool {
        match k {
            b'q' | b'Q' | 3 => return false,
            b'x' | b'X' | 27 => {
                for i in (0..self.wins.len()).rev() {
                    self.close_quiet(i);
                }
                self.layout();
            }
            b'p' | b'P' => {
                self.paused = !self.paused;
                crate::log!("{}", if self.paused { "Paused" } else { "Resumed" });
                self.show_status();
            }
            b'1'..=b'9' => {
                if let Some(c) = self.cameras.get((k - b'1') as usize) {
                    let name = c.name.clone();
                    self.test(&name);
                }
            }
            _ => {}
        }
        true
    }

    fn show_status(&mut self) {
        let state = if self.paused {
            t().status_paused_forever.to_string()
        } else if self.connected {
            i18n::fmt(t().status_connected, &[&self.status])
        } else {
            i18n::fmt(t().status_disconnected, &[&self.status])
        };
        let text = format!(" FrigatePiP {} · {state} · {}", env!("CARGO_PKG_VERSION"), t().console_keys);
        self.out.status(&text);
    }

    /// Applies the settings file when it was saved (checked every 2 s).
    fn reload_if_changed(&mut self) {
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if modified == self.modified {
            return;
        }
        self.modified = modified;
        let Ok((cfg, _)) = config::load(&self.path) else { return };
        if cfg == self.cfg {
            return;
        }
        crate::log!("Settings reloaded");
        i18n::set(i18n::resolve(&cfg.language));
        self.cfg = cfg;
        // The listener also applies the filters (cameras, objects, zones…).
        self.start_listener();
        self.layout();
        self.show_status();
    }
}

/// Window rectangles, stacked from `corner` with `margin` between them; `None` for
/// windows that no longer fit on the screen.
fn stack(screen: (u32, u32), corner: Corner, margin: u32, sizes: &[(u32, u32)]) -> Vec<Option<Rect>> {
    let (sw, sh, m) = (screen.0 as i32, screen.1 as i32, margin as i32);
    let right = matches!(corner, Corner::TopRight | Corner::BottomRight);
    let bottom = matches!(corner, Corner::BottomLeft | Corner::BottomRight);
    let mut offset = m;
    let mut full = false;
    sizes
        .iter()
        .map(|&(w, h)| {
            let (w, h) = (w as i32, h as i32);
            if full || offset + h + m > sh {
                full = true;
                return None;
            }
            let x = if right { sw - m - w } else { m };
            let y = if bottom { sh - offset - h } else { offset };
            offset += h + m;
            Some(Rect { x, y, w: w as u32, h: h as u32 })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_stack_from_the_corner() {
        let r = stack((1920, 1080), Corner::BottomRight, 16, &[(480, 270), (480, 270), (480, 270), (480, 270)]);
        assert_eq!(r[0], Some(Rect { x: 1424, y: 794, w: 480, h: 270 }));
        assert_eq!(r[1], Some(Rect { x: 1424, y: 508, w: 480, h: 270 }));
        assert_eq!(r[2], Some(Rect { x: 1424, y: 222, w: 480, h: 270 }));
        assert_eq!(r[3], None);
        let top_left = stack((800, 600), Corner::TopLeft, 10, &[(100, 50)]);
        assert_eq!(top_left[0], Some(Rect { x: 10, y: 10, w: 100, h: 50 }));
    }

    #[test]
    fn command_line_values() {
        let args: Vec<String> = ["--output", "terminal", "--graphics=sixel"].iter().map(|s| s.to_string()).collect();
        assert_eq!(arg(&args, "--output").as_deref(), Some("terminal"));
        assert_eq!(arg(&args, "--graphics").as_deref(), Some("sixel"));
        assert_eq!(arg(&args, "--test"), None);
    }
}
