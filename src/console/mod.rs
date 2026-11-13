//! Linux version: preview windows on the desktop (X11 or XWayland), drawn on the
//! framebuffer (a console without X or Wayland, e.g. a Raspberry Pi with a monitor),
//! inside a terminal (Kitty graphics, Sixel or coloured half blocks) or as tmux popups.
//! Video is decoded by the `ffmpeg` program.

mod canvas;
mod dbus;
mod desktop;
mod fb;
mod ffmpeg;
mod term;
mod tmux;
mod tray;
mod video;
mod web;
mod x11;

use crate::config::{self, Config, Corner, IniDoc, LiveMode};
use crate::filter::Detection;
use crate::frigate::{self, CameraInfo, Notice};
use crate::history::Recorder;
use crate::hub::Hub;
use crate::i18n::{self, t};
use crate::util::Cancel;
use crate::view::{choose_slot, choose_stream, Slot, Zoom};
use canvas::Image;
use ffmpeg::Ffmpeg;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
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
    /// Draws window `id`: the picture, the caption (empty = none) and the live dot. The
    /// picture is handed over (outputs that keep it need no copy).
    fn draw(&mut self, id: u64, rect: Rect, img: Image, caption: &str, live: bool);
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
    /// At most this many windows at once (tmux: one popup per client).
    fn max_windows(&self) -> Option<usize> {
        None
    }
    /// Outputs that show a window in another process (tmux popups) open it here and
    /// return `Some(opened)`; `None` = this process decodes and draws the window.
    fn open_external(&mut self, _id: u64, _view: &View) -> Option<bool> {
        None
    }
    /// Windows shown by another process: `false` once closed (e.g. by the user).
    fn alive(&mut self, _id: u64) -> bool {
        true
    }
    /// The settings changed.
    fn configure(&mut self, _cfg: &Config) {}
    /// Mouse input on desktop windows: which window and what the user did.
    fn pointer(&mut self, _e: &x11::Event) -> Vec<(u64, Action)> {
        Vec::new()
    }
    /// Camera audio of window `id`: 0 = none, 1 = on, 2 = muted (desktop windows show a button).
    fn set_sound(&mut self, _id: u64, _state: u8) {}
    /// Replay of window `id`: 0 = nothing to replay, 1 = possible, 2 = replaying (desktop windows show a button).
    fn set_replay(&mut self, _id: u64, _state: u8) {}
    /// The picture of window `id` is zoomed in (desktop windows: Ctrl + drag then moves it).
    fn set_zoomed(&mut self, _id: u64, _zoomed: bool) {}
    /// Volume of window `id` in percent (desktop windows show a slider).
    fn set_volume(&mut self, _id: u64, _volume: u8) {}
    /// The mouse is moving the picture of window `id` right now (Ctrl + drag).
    fn panning(&self, _id: u64) -> bool {
        false
    }
    /// The windows have their own buttons (close, sound).
    fn buttons(&self) -> bool {
        false
    }
    /// Desktop scaling: the window width and the margin from the settings are multiplied by it.
    fn scale(&self) -> f32 {
        1.0
    }
    /// The monitors (desktop windows): (x, width, height, primary).
    fn monitors(&self) -> Vec<(i32, u32, u32, bool)> {
        Vec::new()
    }
    /// A full-screen application is active (desktop windows; `skip_fullscreen`).
    fn fullscreen(&mut self) -> bool {
        false
    }
}

/// What the user did with a desktop window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Close,
    /// Sound button: mute / unmute.
    Sound,
    /// A click: pin / unpin.
    Pin,
    /// A double click: open the camera in Frigate.
    Open,
    Hover(bool),
    /// Dragged to this place (in the usable area).
    Moved(i32, i32),
    /// The mouse wheel: bigger (+1) or smaller (-1).
    Wheel(i32),
    /// Ctrl + mouse wheel: zoom in (+1) or out (-1) around this point of the window (0–1).
    Zoom(i32, f32, f32),
    /// Ctrl + drag on a zoomed picture: the mouse moved by this part of the window (0–1).
    Pan(f32, f32),
    /// The volume slider was let go at this volume (percent).
    Volume(u8),
    /// Replay button: start or stop a replay of the event start.
    Replay,
}

/// A window shown by another process (see `Output::open_external`).
pub struct View {
    pub camera: String,
    /// go2rtc stream.
    pub stream: Option<String>,
    pub aspect: f32,
    pub caption: String,
    /// Start with the camera audio muted.
    pub muted: bool,
    /// `stream` is the camera's main stream (see `ffmpeg::HwAccel`).
    pub main: bool,
}

/// How the console version shows its windows.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Window,
    Framebuffer,
    Terminal,
    Tmux,
}

/// Settings of the console version (`[console]` in config.ini, overridable on the command line).
struct ConsoleCfg {
    output: String,
    graphics: String,
    framebuffer: String,
    framebuffer_size: Option<String>,
    ffmpeg: String,
    hwaccel: String,
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
            hwaccel: get("hwaccel", "auto"),
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

const HELP: &str = "FrigatePiP for Linux: instant picture-in-picture camera windows for Frigate NVR.

Usage: frigate-pip [options]

Where the windows appear (--output):
  window       On the desktop (X11, or Wayland with XWayland): borderless windows above
               the others in a corner of the screen; runs in the background. Mouse: x
               closes, the speaker mutes, the slider under it sets the volume, drag
               moves, the wheel resizes, Ctrl + wheel zooms the picture around the
               mouse, Ctrl + drag moves the zoomed picture,
               a click pins, a double click opens the camera in Frigate, the right
               button closes.
               A tray icon has the menu known from Windows; the settings open in the
               browser.
  framebuffer  On the screen of a Linux console without a desktop (e.g. a Raspberry Pi
               with a monitor). Runs in the background, e.g. as a systemd service.
  terminal     Inside this terminal window (Kitty graphics, Sixel or half blocks).
               Runs in the foreground: the terminal shows FrigatePiP while it runs.
  tmux         Runs in the background; each window opens as a tmux popup over what
               you are doing in tmux, e.g.: tmux new -d -s frigate-pip 'frigate-pip --output tmux'
  auto         window on a desktop (DISPLAY is set), framebuffer on a console
               (TERM=linux) or without a terminal, terminal otherwise (default).

Options:
  --output auto|window|framebuffer|terminal|tmux   see above
  --graphics auto|kitty|sixel|blocks        pictures in a terminal (default: auto)
  --framebuffer /dev/fb0                    framebuffer device
  --hwaccel auto|on|off                     video decoding on the graphics card (default:
                                            auto = for the main stream)
  --test CAMERA                             show a camera right after the start
  --test-connection                         check the connection to Frigate and exit
  --list                                    list the cameras with their objects and zones
                                            (the names for cameras, labels, zones in config.ini)
  --settings                                open the settings of the desktop windows
  --autostart on|off                        start the desktop windows with the desktop session
  --quit                                    stop the running desktop windows
  --version                                 print the version
  --help                                    this help

Keys (when started in a terminal or on a console): q quit, x close windows,
p pause / resume, m sound on / off, 1-9 show camera 1-9.
In a tmux popup: q, x or Esc close it, m sound on / off.
Settings: ~/.config/frigate-pip/config.ini - user guide:
https://github.com/maciejgornicz/frigate-pip/blob/main/docs/en/user-guide.md#12-linux-version";

enum Ev {
    Notice(Notice),
    Key(u8),
    /// A window has a new picture.
    Wake,
    /// Mouse on a desktop window, or the desktop went away.
    Pointer(x11::Event),
    /// Tray menu item (or `tray::ACTIVATE`, `tray::NOTIFICATION`).
    Tray(i32),
}

static QUIT: AtomicBool = AtomicBool::new(false);
/// Another start (or `--settings`) asked the running copy to open its settings (SIGUSR1).
static OPEN_SETTINGS: AtomicBool = AtomicBool::new(false);
/// Another start with `--test CAMERA` asked the running copy to show it (SIGUSR2; the
/// camera is in `request_path()`).
static SHOW_CAMERA: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: libc::c_int) {
    match sig {
        libc::SIGUSR1 => OPEN_SETTINGS.store(true, Ordering::Relaxed),
        libc::SIGUSR2 => SHOW_CAMERA.store(true, Ordering::Relaxed),
        _ => QUIT.store(true, Ordering::Relaxed),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Pause {
    Off,
    Until(Instant),
    Forever,
}

// Tray menu items.
const M_STATUS: i32 = 1;
const M_SETTINGS: i32 = 3;
const M_CAMERAS: i32 = 4;
const M_PAUSE: i32 = 5;
const M_CLOSE_ALL: i32 = 6;
const M_OPEN_FRIGATE: i32 = 8;
const M_HELP: i32 = 9;
const M_LOG: i32 = 10;
const M_CONFIG: i32 = 11;
const M_EXIT: i32 = 13;
const M_PAUSE_15: i32 = 51;
const M_PAUSE_60: i32 = 52;
const M_PAUSE_FOREVER: i32 = 53;
const M_RESUME: i32 = 55;
const M_NO_CAMERAS: i32 = 99;
const M_CAMERA: i32 = 100;

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
    // Without a terminal (e.g. a systemd service) the log also goes to stderr (the journal);
    // not for the commands that only print something (their output may be piped).
    let report_only = args.iter().any(|a| a == "--list" || a == "--test-connection");
    crate::util::init_log(!report_only && !is_tty(libc::STDOUT_FILENO));
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
    if args.iter().any(|a| a == "--list") {
        match frigate::cameras(&cfg, &Cancel::new()) {
            Ok(cams) => print!("{}", camera_list(&cfg.url, &cams)),
            Err(e) => {
                eprintln!("{}", i18n::fmt(t().test_config_fail, &[&e]));
                std::process::exit(1);
            }
        }
        return;
    }
    if args.iter().any(|a| a == "--quit") {
        match single_instance() {
            Err(pid) if pid > 0 => unsafe {
                libc::kill(pid, libc::SIGTERM);
            },
            _ => println!("{}", t().console_not_running),
        }
        return;
    }
    let open_settings = args.iter().any(|a| a == "--settings");
    if open_settings {
        // The running desktop windows open their settings; otherwise start with them.
        if let Err(pid) = single_instance() {
            if pid > 0 {
                unsafe {
                    libc::kill(pid, libc::SIGUSR1);
                }
                return;
            }
        }
    }
    if let Some(v) = arg(&args, "--autostart") {
        match set_autostart(v != "off") {
            Ok(msg) => println!("{msg}"),
            Err(e) => {
                eprintln!("FrigatePiP: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let console = ConsoleCfg::read(&doc, &args);
    let ff = match Ffmpeg::find(&console.ffmpeg, ffmpeg::HwAccel::parse(&console.hwaccel)) {
        Ok(f) => Arc::new(f),
        Err(e) => {
            eprintln!("FrigatePiP: {e}\nffmpeg is needed for video - install it, e.g.: sudo apt install ffmpeg");
            std::process::exit(1);
        }
    };
    if start_in_background(&args, &console) {
        return;
    }
    install_signals();
    // Viewer inside a tmux popup (started by the background process).
    if let Some(camera) = arg(&args, "--view") {
        run_view(&camera, &args, &cfg, &console, ff);
        return;
    }
    let (tx, rx) = mpsc::channel();
    let (out, mode) = match open_output(&console, &cfg, &tx, args.iter().any(|a| a == "--detached")) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("FrigatePiP: {e}");
            std::process::exit(1);
        }
    };
    // One copy of the desktop windows per user (autostart and a manual start); a second
    // start (e.g. from the applications menu) opens the settings of the running one.
    let _instance = match mode {
        Mode::Window => match single_instance() {
            Ok(lock) => lock,
            Err(pid) => {
                hand_over(pid, &args);
                println!("{}", t().console_already_running);
                return;
            }
        },
        _ => None,
    };
    if is_tty(libc::STDOUT_FILENO) {
        match mode {
            Mode::Tmux => println!("FrigatePiP {} - {}", env!("CARGO_PKG_VERSION"), t().console_tmux_running),
            Mode::Window => println!("FrigatePiP {} - {}", env!("CARGO_PKG_VERSION"), t().console_window_running),
            _ => {}
        }
    }
    let keys = is_tty(libc::STDIN_FILENO).then(|| {
        let tx = tx.clone();
        // The terminal output switches the terminal to raw mode itself.
        term::Keys::start(mode != Mode::Terminal, move |k| {
            let _ = tx.send(Ev::Key(k));
        })
    });
    let mut app = App::new(cfg, path, out, ff, tx);
    app.pending_test = arg(&args, "--test");
    // tmux popups are separate processes: they cannot use this one's prebuffer.
    app.prebuffer_possible = mode != Mode::Tmux;
    app.start_listener();
    if mode == Mode::Window {
        app.start_desktop(first_start, open_settings, args.iter().any(|a| a == "--session"));
    }
    app.run(rx);
    drop(app);
    drop(keys);
    crate::log!("Stopped");
}

/// `--list`: every camera with the objects and zones Frigate knows, for the filters in config.ini.
fn camera_list(url: &str, cams: &[CameraInfo]) -> String {
    let s = t();
    let mut out = i18n::fmt(s.console_list_title, &[&url, &cams.len()]);
    out.push('\n');
    let width = s.console_list_objects.chars().count().max(s.console_list_zones.chars().count()) + 1;
    for c in cams {
        out.push('\n');
        out.push_str(&c.name);
        if c.title != c.name {
            out.push_str(&format!(" ({})", c.title));
        }
        out.push('\n');
        for (label, names) in [(s.console_list_objects, &c.labels), (s.console_list_zones, &c.zones)] {
            let names = if names.is_empty() { "—".to_string() } else { names.join(", ") };
            out.push_str(&format!("  {:<width$} {names}\n", format!("{label}:")));
        }
    }
    out.push('\n');
    out.push_str(s.console_list_hint);
    out.push('\n');
    out
}

/// Lock file of the desktop windows (one copy per user); it holds the process id. In the
/// user's private runtime folder, or the settings folder — never a shared /tmp.
fn lock_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(crate::util::app_dir);
    dir.join(format!("frigate-pip-{}.lock", unsafe { libc::getuid() }))
}

/// The camera another start (`--test`) wants the running copy to show.
fn request_path() -> PathBuf {
    lock_path().with_extension("camera")
}

/// Takes the lock for this copy (`None` when no lock file can be used);
/// `Err(pid)` when another copy holds it.
fn single_instance() -> Result<Option<std::fs::File>, i32> {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let open =
        std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).custom_flags(libc::O_NOFOLLOW).open(lock_path());
    let Ok(mut f) = open else {
        return Ok(None);
    };
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let mut pid = String::new();
        let _ = f.read_to_string(&mut pid);
        return Err(pid.trim().parse().unwrap_or(0));
    }
    let _ = f.set_len(0).and_then(|_| f.write_all(std::process::id().to_string().as_bytes()));
    Ok(Some(f))
}

/// Hands this start over to the running copy (`pid`): `--test CAMERA` shows the camera
/// there, anything else opens its settings.
fn hand_over(pid: i32, args: &[String]) {
    if pid <= 0 {
        return;
    }
    let sig = match arg(args, "--test") {
        Some(camera) if std::fs::write(request_path(), &camera).is_ok() => libc::SIGUSR2,
        _ => libc::SIGUSR1,
    };
    unsafe {
        libc::kill(pid, sig);
    }
}

/// `--output auto` (or an unknown value).
fn output_auto(output: &str) -> bool {
    !matches!(output, "window" | "desktop" | "tmux" | "framebuffer" | "fb" | "terminal")
}

/// Desktop windows started from a terminal continue in the background, like a start from
/// the menu: the terminal is free again and may be closed. `true` = this process is done
/// (the copy in the background runs, or one was already running).
fn start_in_background(args: &[String], c: &ConsoleCfg) -> bool {
    use std::os::unix::process::CommandExt;
    let output = c.output.as_str();
    let window = matches!(output, "window" | "desktop") || (output_auto(output) && std::env::var_os("DISPLAY").is_some());
    if !window || !is_tty(libc::STDIN_FILENO) || args.iter().any(|a| a == "--detached" || a == "--view") {
        return false;
    }
    // Without a usable desktop the normal start reports it (or uses the console).
    if x11::X11::connect(|_| {}).is_err() {
        return false;
    }
    match single_instance() {
        Err(pid) => {
            hand_over(pid, args);
            println!("{}", t().console_already_running);
            return true;
        }
        Ok(lock) => drop(lock),
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args).arg("--detached").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Its own session: closing the terminal does not end it.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    match cmd.spawn() {
        Ok(_) => {
            println!("FrigatePiP {} - {}", env!("CARGO_PKG_VERSION"), t().console_window_running);
            true
        }
        Err(e) => {
            crate::log!("Cannot continue in the background: {e}");
            false
        }
    }
}

/// The desktop entry in ~/.config/autostart that starts the desktop windows with the session.
fn autostart_file() -> Option<PathBuf> {
    crate::util::app_dir().parent().map(|p| p.join("autostart").join("frigate-pip.desktop"))
}

/// `--autostart on|off` (also the settings page).
fn set_autostart(on: bool) -> Result<String, String> {
    let file = autostart_file().ok_or("no configuration folder")?;
    let dir = file.parent().map(PathBuf::from).unwrap_or_default();
    if !on {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", file.display())),
            _ => Ok(t().console_autostart_off.to_string()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&file, autostart_entry(&exe.to_string_lossy())))
        .map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(i18n::fmt(t().console_autostart_on, &[&file.display()]))
}

fn autostart_entry(exe: &str) -> String {
    let exec = if exe.contains(' ') { format!("\"{exe}\"") } else { exe.to_string() };
    format!(
        "[Desktop Entry]\nType=Application\nName=FrigatePiP\nComment=Camera windows for Frigate NVR detections\nExec={exec} --output window --session\nIcon=frigate-pip\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    )
}

fn install_signals() {
    unsafe {
        for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGUSR1, libc::SIGUSR2] {
            libc::signal(sig, on_signal as *const () as libc::sighandler_t);
        }
    }
}

/// Desktop windows on a desktop, tmux popups when asked for, otherwise the framebuffer on
/// a console without graphics and the terminal elsewhere. `detached` = the copy started in
/// the background for desktop windows (no terminal to fall back to).
fn open_output(c: &ConsoleCfg, cfg: &Config, tx: &Sender<Ev>, detached: bool) -> Result<(Box<dyn Output>, Mode), String> {
    let output = c.output.as_str();
    let auto = output_auto(output) && !detached;
    if matches!(output, "window" | "desktop") || detached || (auto && std::env::var_os("DISPLAY").is_some()) {
        let tx = tx.clone();
        let font = canvas::Font::load(c.font.as_deref());
        match desktop::Desktop::open(cfg, font, move |e| {
            let _ = tx.send(Ev::Pointer(e));
        }) {
            Ok(d) => {
                crate::log!("Output: desktop windows");
                return Ok((Box::new(d), Mode::Window));
            }
            Err(e) if auto => crate::log!("No desktop windows ({e}) - using the console"),
            Err(e) => return Err(e),
        }
    }
    let on_console = std::env::var("TERM").is_ok_and(|t| t == "linux") || !is_tty(libc::STDOUT_FILENO);
    let mode = match output {
        "tmux" => Mode::Tmux,
        "framebuffer" | "fb" => Mode::Framebuffer,
        "terminal" => Mode::Terminal,
        _ if on_console && std::path::Path::new(&c.framebuffer).exists() => Mode::Framebuffer,
        _ => Mode::Terminal,
    };
    let out: Box<dyn Output> = match mode {
        Mode::Framebuffer => {
            let font = canvas::Font::load(c.font.as_deref());
            Box::new(fb::Framebuffer::open(&c.framebuffer, c.framebuffer_size.as_deref(), font)?)
        }
        Mode::Tmux => Box::new(tmux::Tmux::new(cfg.width.max(64) as u32, cfg.position)?),
        _ => Box::new(term::Terminal::open(&c.graphics, true)?),
    };
    crate::log!(
        "Output: {}",
        match mode {
            Mode::Framebuffer => "framebuffer",
            Mode::Tmux => "tmux popups",
            _ => "terminal",
        }
    );
    Ok((out, mode))
}

/// Viewer mode: one camera filling the terminal (a tmux popup) until it is closed
/// (q, x, Esc) or the popup is closed by the background process.
fn run_view(camera: &str, args: &[String], cfg: &Config, console: &ConsoleCfg, ff: Arc<Ffmpeg>) {
    // The lock tells the background process that this popup is still open.
    let _lock = arg(args, "--view-lock").and_then(|p| tmux::hold_lock(&p));
    let stream = arg(args, "--view-stream");
    let aspect = arg(args, "--view-aspect").and_then(|a| a.parse::<f32>().ok()).filter(|a| *a > 0.1).unwrap_or(16.0 / 9.0);
    let caption = arg(args, "--view-caption").unwrap_or_else(|| camera.to_string());
    let mut out = match term::Terminal::open(&console.graphics, false) {
        Ok(t) => t,
        Err(e) => {
            crate::log!("{camera}: viewer: {e}");
            return;
        }
    };
    let (tx, rx) = mpsc::channel();
    let _keys = {
        let tx = tx.clone();
        term::Keys::start(false, move |k| {
            let _ = tx.send(Ev::Key(k));
        })
    };
    let (sw, sh) = out.size();
    let size = ((sw & !1).max(2), (sh.saturating_sub(out.caption_height()) & !1).max(2));
    crate::log!("{camera}: viewer {sw}x{sh} (aspect {aspect:.2})");
    let params = video::Params {
        camera: camera.to_string(),
        frigate: crate::client::Frigate::new(cfg),
        rtsp_url: stream.map(|s| cfg.rtsp_url(&s)),
        hub: None,
        main: args.iter().any(|a| a == "--view-main"),
        live: cfg.live,
        size,
        zoom: Zoom::FULL,
        audio: cfg.camera_audio,
        volume: cfg.camera_volume,
        wind_filter: cfg.wind_filter,
        t0: Instant::now(),
        event: Instant::now(),
        own: Arc::default(),
    };
    let wake = tx.clone();
    let shared = video::start(
        params,
        ff,
        Arc::new(move || {
            let _ = wake.send(Ev::Wake);
        }),
    );
    shared.muted.store(args.iter().any(|a| a == "--view-muted"), Ordering::Relaxed);
    let rect = Rect { x: 0, y: 0, w: sw, h: sh };
    let mut last: Option<Instant> = None;
    while !QUIT.load(Ordering::Relaxed) {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ev::Key(b'q' | b'Q' | b'x' | b'X' | 27 | 3)) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Ev::Key(b'm' | b'M')) if cfg.camera_audio => {
                let muted = !shared.muted.fetch_xor(true, Ordering::Relaxed);
                crate::log!("{camera}: sound {}", if muted { "muted" } else { "on" });
                shared.dirty.store(true, Ordering::Relaxed);
            }
            Ok(Ev::Key(b'r' | b'R')) => shared.replay_pressed.store(true, Ordering::Relaxed),
            _ => {}
        }
        if last.is_some_and(|t| t.elapsed() < out.min_interval()) || !shared.dirty.swap(false, Ordering::Relaxed) {
            continue;
        }
        let frame = shared.frame.lock().unwrap().clone();
        if let Some((frame, _)) = frame {
            let text = with_replay(&shared, with_sound(&caption, cfg.camera_audio && shared.muted.load(Ordering::Relaxed)));
            out.draw(1, rect, frame, &text, shared.live.load(Ordering::Relaxed));
            last = Some(Instant::now());
        }
    }
    shared.cancel.cancel();
}

/// Opens an address in the desktop's browser (xdg-open).
fn open_url(url: &str) {
    match std::process::Command::new("xdg-open").arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(mut child) => {
            let _ = std::thread::Builder::new().name("xdg-open".into()).spawn(move || child.wait());
        }
        Err(e) => crate::log!("Cannot open {url}: xdg-open: {e}"),
    }
}

/// The caption during a replay: how far behind live the picture is (also without a caption).
fn with_replay(shared: &video::Shared, caption: String) -> String {
    if shared.replay.load(Ordering::Relaxed) == 2 {
        let behind = Duration::from_millis(shared.behind.load(Ordering::Relaxed) as u64);
        crate::replay::caption(t().replay, behind, &caption)
    } else {
        caption
    }
}

/// The caption, marked when the camera audio is muted.
fn with_sound(caption: &str, muted: bool) -> String {
    if muted {
        format!("{caption} · {}", t().console_sound_off)
    } else {
        caption.to_string()
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
    /// Size of the window's picture (the mouse wheel changes it).
    size: (u32, u32),
    /// Picture size the decoder was started with, and its go2rtc stream.
    dec_size: (u32, u32),
    stream: Option<String>,
    /// Resized or zoomed with the wheel: the decoder follows once the wheel stops.
    resized: Option<Instant>,
    /// The part of the picture shown (Ctrl + wheel), and the part the decoder delivers.
    zoom: Zoom,
    dec_zoom: Zoom,
    /// A decoder restarted after the wheel: it takes over at its first live picture, the old
    /// one plays until then (no frozen picture while the new one waits for a key frame).
    next: Option<(Arc<video::Shared>, Instant)>,
    /// The last picture of the whole view, kept when zooming in: zooming out fills the sides
    /// from it until the decoder delivers the wider part.
    whole: Option<Image>,
    last_draw: Option<Instant>,
    /// Desktop windows: the mouse is over it, pinned with a click, dragged by the user.
    hovered: bool,
    pinned: bool,
    user_moved: bool,
    /// Sound state last given to the output (see `Output::set_sound`).
    sound: u8,
    /// When the event came, and what the window recorded itself (replays, also after a restart).
    event: Instant,
    own: Arc<Mutex<Recorder>>,
}

impl Win {
    /// Stops the window's video (and a decoder waiting to take over).
    fn stop(&self) {
        self.shared.cancel.cancel();
        if let Some((next, _)) = &self.next {
            next.cancel.cancel();
        }
    }
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
    pause: Pause,
    /// Camera audio muted (key m), also for windows opened later.
    muted: bool,
    connected: bool,
    status: String,
    listener: Cancel,
    pending_test: Option<String>,
    started: Instant,
    /// Desktop windows: the tray icon, the settings page and what the page shows.
    tray: Option<tray::Tray>,
    web: Option<web::Web>,
    info: Arc<std::sync::Mutex<web::Info>>,
    /// `prebuffer`: a connection to every camera's sub stream, by camera name.
    hubs: HashMap<String, Arc<Hub>>,
    prebuffer_possible: bool,
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
            pause: Pause::Off,
            muted: false,
            connected: false,
            status: t().status_connecting.to_string(),
            listener: Cancel::new(),
            pending_test: None,
            started: Instant::now(),
            tray: None,
            web: None,
            info: Arc::default(),
            hubs: HashMap::new(),
            prebuffer_possible: false,
        }
    }

    /// Desktop windows: the settings page, the tray icon, and the settings at the first
    /// start (or when asked). `session` = started with the desktop session.
    fn start_desktop(&mut self, first_start: bool, open_settings: bool, session: bool) {
        {
            let mut info = self.info.lock().unwrap();
            info.monitors = self.out.monitors();
            info.autostart = autostart_file().is_some_and(|f| f.exists());
        }
        let info = self.info.clone();
        let on_autostart: web::OnAutostart = Arc::new(move |on| {
            match set_autostart(on) {
                Ok(msg) => crate::log!("{msg}"),
                Err(e) => crate::log!("Autostart: {e}"),
            }
            info.lock().unwrap().autostart = on;
        });
        match web::start(self.path.clone(), self.info.clone(), on_autostart) {
            Ok(w) => {
                crate::log!("Settings page ready");
                self.web = Some(w);
            }
            Err(e) => crate::log!("No settings page: {e}"),
        }
        let tx = self.tx.clone();
        match tray::Tray::start(move |id| {
            let _ = tx.send(Ev::Tray(id));
        }) {
            Ok(tr) => self.tray = Some(tr),
            Err(e) => crate::log!("No tray icon: {e}"),
        }
        self.update_tray();
        let s = t();
        let unset = first_start || self.cfg.url == Config::from_ini(&IniDoc::parse(config::DEFAULT_INI)).url;
        if unset {
            // Nothing to show yet: the Frigate address comes first.
            if let Some(tr) = &self.tray {
                tr.notify("FrigatePiP", s.linux_notify_setup, Some(s.menu_settings));
            }
            self.open_settings();
        } else if open_settings {
            self.open_settings();
        } else if !session {
            if let Some(tr) = self.tray.as_ref().filter(|tr| !tr.registered) {
                tr.notify("FrigatePiP", s.linux_notify_no_tray, Some(s.menu_settings));
            }
        }
    }

    fn open_settings(&self) {
        match &self.web {
            Some(w) => open_url(&w.url),
            None => crate::log!("The settings page is not available"),
        }
    }

    fn paused(&self) -> bool {
        match self.pause {
            Pause::Off => false,
            Pause::Forever => true,
            Pause::Until(until) => Instant::now() < until,
        }
    }

    fn set_pause(&mut self, pause: Pause) {
        self.pause = pause;
        crate::log!("{}", if self.paused() { "Paused" } else { "Resumed" });
        self.show_status();
    }

    /// The state shown in the status line and the tray menu.
    fn status_line(&self) -> String {
        let s = t();
        match self.pause {
            Pause::Forever => s.status_paused_forever.to_string(),
            Pause::Until(until) if Instant::now() < until => {
                let minutes = until.saturating_duration_since(Instant::now()).as_secs().div_ceil(60);
                i18n::fmt(s.status_paused_minutes, &[&minutes])
            }
            _ if self.connected => i18n::fmt(s.status_connected, &[&self.status]),
            _ => i18n::fmt(s.status_disconnected, &[&self.status]),
        }
    }

    /// The tray menu — the same as on Windows.
    fn tray_menu(&self) -> Vec<tray::Item> {
        use tray::Item;
        let s = t();
        let paused = self.paused();
        let mut cams: Vec<Item> = self.cameras.iter().enumerate().take(100).map(|(i, c)| Item::action(M_CAMERA + i as i32, &c.title)).collect();
        if cams.is_empty() {
            cams.push(Item::disabled(M_NO_CAMERAS, s.menu_no_cameras));
        }
        let mut pause = vec![
            Item::action(M_PAUSE_15, s.menu_pause_15),
            Item::action(M_PAUSE_60, s.menu_pause_60),
            Item::check(M_PAUSE_FOREVER, s.menu_pause_forever, self.pause == Pause::Forever),
        ];
        if paused {
            pause.push(Item::separator(54));
            pause.push(Item::action(M_RESUME, s.menu_resume));
        }
        vec![
            Item::disabled(M_STATUS, &self.status_line()),
            Item::separator(2),
            Item::action(M_SETTINGS, s.menu_settings),
            Item::submenu(M_CAMERAS, s.menu_show_camera, cams),
            Item::submenu(M_PAUSE, if paused { s.menu_paused } else { s.menu_pause }, pause),
            Item::action(M_CLOSE_ALL, s.menu_close_all),
            Item::separator(7),
            Item::action(M_OPEN_FRIGATE, s.menu_open_frigate),
            Item::action(M_HELP, s.menu_help),
            Item::action(M_LOG, s.menu_log),
            Item::action(M_CONFIG, s.menu_config_file),
            Item::separator(12),
            Item::action(M_EXIT, s.menu_exit),
        ]
    }

    fn update_tray(&self) {
        let Some(tr) = &self.tray else { return };
        // The colours of the Windows tray icon: paused, connected, not connected.
        let color = if self.paused() {
            (0xF5, 0x9E, 0x0B)
        } else if self.connected {
            (0x3B, 0x82, 0xF6)
        } else {
            (0x94, 0x94, 0x94)
        };
        tr.update(&self.status_line(), color, self.tray_menu());
    }

    fn on_tray(&mut self, id: i32) {
        let now = Instant::now();
        match id {
            tray::ACTIVATE | tray::NOTIFICATION | M_SETTINGS => self.open_settings(),
            M_PAUSE_15 => self.set_pause(Pause::Until(now + Duration::from_secs(15 * 60))),
            M_PAUSE_60 => self.set_pause(Pause::Until(now + Duration::from_secs(60 * 60))),
            M_PAUSE_FOREVER => self.set_pause(Pause::Forever),
            M_RESUME => self.set_pause(Pause::Off),
            M_CLOSE_ALL => {
                for i in (0..self.wins.len()).rev() {
                    self.close_quiet(i);
                }
                self.layout();
            }
            M_OPEN_FRIGATE => open_url(&self.cfg.url),
            M_HELP => open_url(&format!("https://github.com/maciejgornicz/frigate-pip/blob/main/docs/{}/user-guide.md", i18n::current().code())),
            M_LOG => open_url(&crate::util::log_path().to_string_lossy()),
            M_CONFIG => open_url(&self.path.to_string_lossy()),
            M_EXIT => QUIT.store(true, Ordering::Relaxed),
            id if id >= M_CAMERA => {
                if let Some(c) = self.cameras.get((id - M_CAMERA) as usize) {
                    let name = c.name.clone();
                    self.test(&name);
                }
            }
            _ => {}
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
                Ok(Ev::Pointer(x11::Event::Closed)) => {
                    crate::log!("The desktop session ended");
                    break;
                }
                Ok(Ev::Pointer(e)) => {
                    for (id, action) in self.out.pointer(&e) {
                        self.on_action(id, action);
                    }
                }
                Ok(Ev::Tray(id)) => self.on_tray(id),
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
            if OPEN_SETTINGS.swap(false, Ordering::Relaxed) {
                self.open_settings();
            }
            if SHOW_CAMERA.swap(false, Ordering::Relaxed) {
                if let Ok(camera) = std::fs::read_to_string(request_path()) {
                    let _ = std::fs::remove_file(request_path());
                    self.test(camera.trim());
                }
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
                self.info.lock().unwrap().cameras = list.clone();
                self.cameras = list;
                self.update_hubs();
                self.update_tray();
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

    /// The mouse on a desktop window — like on Windows.
    fn on_action(&mut self, id: u64, action: Action) {
        let Some(i) = self.wins.iter().position(|w| w.id == id) else { return };
        let w = &mut self.wins[i];
        match action {
            Action::Close => {
                crate::log!("{}: closed by the user", w.camera);
                self.close(i);
                return;
            }
            Action::Sound => {
                let muted = !w.shared.muted.fetch_xor(true, Ordering::Relaxed);
                crate::log!("{}: sound {}", w.camera, if muted { "muted" } else { "on" });
            }
            Action::Pin => w.pinned = !w.pinned,
            Action::Open => {
                // The first click of the double click pinned it: undo that.
                w.pinned = !w.pinned;
                open_url(&format!("{}/#{}", self.cfg.url, w.camera));
            }
            Action::Hover(on) => {
                w.hovered = on;
                if !on {
                    // Like on Windows: at least 3 more seconds after the mouse leaves.
                    w.deadline = w.deadline.max(Instant::now() + Duration::from_secs(3));
                }
            }
            Action::Moved(x, y) => {
                w.user_moved = true;
                if let Some(r) = w.rect.as_mut() {
                    (r.x, r.y) = (x, y);
                }
            }
            Action::Zoom(notches, u, v) => {
                if w.zoom.is_full() && w.whole.is_none() {
                    w.whole = w.shared.frame.lock().unwrap().as_ref().filter(|(_, z)| z.is_full()).map(|(f, _)| f.clone());
                }
                w.zoom = w.zoom.at(notches as f32, u, v);
                // At once from the current pictures; sharp once the wheel stops.
                w.resized = Some(Instant::now());
            }
            Action::Pan(du, dv) => {
                w.zoom = w.zoom.panned(du, dv);
                // Like the wheel: sharp, with the new part live, once the drag ends.
                w.resized = Some(Instant::now());
            }
            Action::Replay => {
                w.shared.replay_pressed.store(true, Ordering::Relaxed);
                // A stopped replay, too, leaves a few seconds of live video before the window closes.
                w.deadline = w.deadline.max(Instant::now() + Duration::from_secs(5));
            }
            Action::Volume(volume) => {
                w.shared.volume.store(volume, Ordering::Relaxed);
                if volume > 0 {
                    w.shared.muted.store(false, Ordering::Relaxed);
                }
                crate::log!("{}: volume {volume} %", w.camera);
            }
            Action::Wheel(step) => {
                // Like on Windows: 12 % per notch, from 160 px to the width of the screen.
                let scale = self.out.scale();
                let max = self.out.size().0.saturating_sub(2 * self.margin()).max(64);
                let w = &mut self.wins[i];
                let aspect = w.size.0 as f32 / w.size.1.max(1) as f32;
                let factor = if step > 0 { 1.12 } else { 1.0 / 1.12 };
                let width = ((w.size.0 as f32 * factor) as u32).clamp(((160.0 * scale) as u32).min(max), max);
                w.size = (width & !1, ((width as f32 / aspect).round() as u32).max(2) & !1);
                w.resized = Some(Instant::now());
                if w.user_moved {
                    let (width, height) = (w.size.0, w.size.1 + self.out.caption_height());
                    if let Some(r) = w.rect.as_mut() {
                        (r.w, r.h) = (width, height);
                    }
                }
                w.shared.dirty.store(true, Ordering::Relaxed);
                self.layout();
                return;
            }
        }
        w.shared.dirty.store(true, Ordering::Relaxed);
    }

    /// The decoder of window `i` again, at the window's current size and zoom (after the wheel).
    fn restart_video(&mut self, i: usize) {
        let main = self.is_main(&self.wins[i].camera, self.wins[i].stream.as_deref());
        let hub = self.hubs.get(&self.wins[i].camera).cloned();
        let w = &mut self.wins[i];
        let params = video::Params {
            camera: w.camera.clone(),
            frigate: crate::client::Frigate::new(&self.cfg),
            rtsp_url: w.stream.as_ref().map(|s| self.cfg.rtsp_url(s)),
            hub,
            main,
            live: self.cfg.live,
            size: w.size,
            zoom: w.zoom,
            audio: self.cfg.camera_audio,
            volume: w.shared.volume.load(Ordering::Relaxed),
            wind_filter: self.cfg.wind_filter,
            t0: Instant::now(),
            event: w.event,
            own: w.own.clone(),
        };
        let tx = self.tx.clone();
        let shared = video::start(
            params,
            self.ff.clone(),
            Arc::new(move || {
                let _ = tx.send(Ev::Wake);
            }),
        );
        // Silent until it takes over: the old decoder still plays (see `draw`).
        shared.muted.store(true, Ordering::Relaxed);
        if let Some((waiting, _)) = w.next.replace((shared, Instant::now())) {
            waiting.cancel.cancel();
        }
        w.dec_size = w.size;
        w.dec_zoom = w.zoom;
        let zoom = w.zoom.label().map(|z| format!(", zoom {z}")).unwrap_or_default();
        crate::log!("{}: video {}x{}{zoom}", w.camera, w.size.0, w.size.1);
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
        if self.paused() && !test {
            crate::log!("{}: skipped (paused)", d.camera);
            return;
        }
        if self.cfg.skip_fullscreen && !test && self.out.fullscreen() {
            crate::log!("{}: skipped (full screen)", d.camera);
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
        // Pinned windows are never replaced.
        let states: Vec<(bool, bool)> = self.wins.iter().map(|w| (false, w.pinned)).collect();
        let max_windows = self.out.max_windows().map_or(self.cfg.max_popups, |m| m.min(self.cfg.max_popups));
        let replace_at = match choose_slot(&states, max_windows, self.cfg.replace) {
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
        // The replaced window goes first (tmux shows only one popup at a time).
        if let Some(i) = replace_at {
            crate::log!("{}: replaces window {}", d.camera, self.wins[i].camera);
            self.close_quiet(i);
        }
        let id = self.next_id;
        self.next_id += 1;
        let main = self.is_main(&d.camera, stream.as_deref());
        let view = View { camera: d.camera.clone(), stream: stream.clone(), aspect, caption: caption.clone(), muted: self.muted, main };
        let own: Arc<Mutex<Recorder>> = Arc::default();
        let shared = match self.out.open_external(id, &view) {
            Some(true) => Arc::new(video::Shared::default()),
            Some(false) => {
                self.layout();
                return;
            }
            None => {
                let params = video::Params {
                    camera: d.camera.clone(),
                    frigate: crate::client::Frigate::new(&self.cfg),
                    rtsp_url: stream.as_ref().map(|s| self.cfg.rtsp_url(s)),
                    hub: self.hubs.get(&d.camera).cloned(),
                    main,
                    live: self.cfg.live,
                    size,
                    zoom: Zoom::FULL,
                    audio: self.cfg.camera_audio,
                    volume: self.cfg.camera_volume,
                    wind_filter: self.cfg.wind_filter,
                    t0,
                    event: t0,
                    own: own.clone(),
                };
                let tx = self.tx.clone();
                video::start(
                    params,
                    self.ff.clone(),
                    Arc::new(move || {
                        let _ = tx.send(Ev::Wake);
                    }),
                )
            }
        };
        shared.muted.store(self.muted, Ordering::Relaxed);
        crate::log!("{}: window after {} ms", d.camera, t0.elapsed().as_millis());
        let win = Win {
            id,
            camera: d.camera,
            caption,
            deadline: now + dur,
            hard: now + max,
            shared,
            rect: None,
            size,
            dec_size: size,
            stream,
            resized: None,
            zoom: Zoom::FULL,
            dec_zoom: Zoom::FULL,
            next: None,
            whole: None,
            last_draw: None,
            hovered: false,
            pinned: false,
            user_moved: false,
            sound: 0,
            event: t0,
            own,
        };
        match replace_at {
            Some(i) => self.wins.insert(i, win),
            None => self.wins.push(win),
        }
        self.layout();
        if self.cfg.sound {
            self.ff.play_notification_sound();
        }
    }

    /// Picture size for a camera with this aspect ratio, within the screen.
    fn picture_size(&self, aspect: f32) -> (u32, u32) {
        let (sw, sh) = self.out.size();
        let margin = self.margin();
        let width = (self.cfg.width.max(64) as f32 * self.out.scale()).round() as u32;
        let w = width.min(sw.saturating_sub(2 * margin).max(64));
        let mut h = (w as f32 / aspect).round() as u32;
        let max_h = sh.saturating_sub(2 * margin + self.out.caption_height()).max(32);
        let mut w = w;
        if h > max_h {
            h = max_h;
            w = (h as f32 * aspect).round() as u32;
        }
        (w & !1, h & !1)
    }

    /// Distance from the screen edge in pixels (scaled on the desktop).
    fn margin(&self) -> u32 {
        (self.cfg.margin.max(0) as f32 * self.out.scale()).round() as u32
    }

    /// Places the windows from the chosen corner, oldest first (windows the user dragged
    /// somewhere else stay there).
    fn layout(&mut self) {
        let caption_h = self.out.caption_height();
        let placed: Vec<usize> = (0..self.wins.len()).filter(|&i| !self.wins[i].user_moved).collect();
        let sizes: Vec<(u32, u32)> = placed.iter().map(|&i| (self.wins[i].size.0, self.wins[i].size.1 + caption_h)).collect();
        let rects = stack(self.out.size(), self.cfg.position, self.margin(), &sizes);
        for (k, rect) in rects.into_iter().enumerate() {
            let i = placed[k];
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
        let buttons = self.out.buttons();
        for w in &mut self.wins {
            // A restarted decoder takes over at its first live picture (a picture of any kind
            // when the old one has none live, or after 5 s).
            let ready = w.next.as_ref().is_some_and(|(next, since)| {
                next.frame.lock().unwrap().is_some()
                    && (next.live.load(Ordering::Relaxed) || !w.shared.live.load(Ordering::Relaxed) || since.elapsed() > Duration::from_secs(5))
            });
            if ready {
                if let Some((next, _)) = w.next.take() {
                    next.muted.store(w.shared.muted.load(Ordering::Relaxed), Ordering::Relaxed);
                    next.volume.store(w.shared.volume.load(Ordering::Relaxed), Ordering::Relaxed);
                    next.dirty.store(true, Ordering::Relaxed);
                    std::mem::replace(&mut w.shared, next).cancel.cancel();
                }
            }
            let Some(rect) = w.rect else { continue };
            let muted = w.shared.muted.load(Ordering::Relaxed);
            let sound = match (self.cfg.camera_audio && w.shared.audio.load(Ordering::Relaxed), muted) {
                (false, _) => 0,
                (true, false) => 1,
                (true, true) => 2,
            };
            if sound != w.sound && w.last_draw.is_some() {
                w.sound = sound;
                self.out.set_sound(w.id, sound);
            }
            if w.last_draw.is_some_and(|t| t.elapsed() < min_gap) || !w.shared.dirty.swap(false, Ordering::Relaxed) {
                continue;
            }
            let Some((frame, frame_zoom)) = w.shared.frame.lock().unwrap().clone() else {
                continue;
            };
            // A new zoom shows at once from the pictures the decoder delivers now.
            if frame_zoom.is_full() && w.zoom.is_full() {
                w.whole = None;
            }
            let frame = frame.zoomed(frame_zoom, w.zoom, w.whole.as_ref());
            let caption = with_replay(
                &w.shared,
                if self.cfg.overlay || caption_h > 0 {
                    let base = crate::view::state_caption(&w.caption, w.pinned, w.zoom);
                    // Desktop windows show a muted speaker instead.
                    with_sound(&base, muted && self.cfg.camera_audio && !buttons)
                } else {
                    String::new()
                },
            );
            self.out.draw(w.id, rect, frame, &caption, w.shared.live.load(Ordering::Relaxed));
            // Each time: a window that did not fit for a while comes back as a new one.
            self.out.set_zoomed(w.id, !w.zoom.is_full());
            self.out.set_volume(w.id, w.shared.volume.load(Ordering::Relaxed));
            self.out.set_sound(w.id, w.sound);
            self.out.set_replay(w.id, w.shared.replay.load(Ordering::Relaxed));
            w.last_draw = Some(Instant::now());
        }
        // A throttled picture waits for the next frame or the next loop pass (≤ 100 ms).
    }

    fn tick(&mut self) {
        if matches!(self.pause, Pause::Until(until) if Instant::now() >= until) {
            self.set_pause(Pause::Off);
        }
        if matches!(self.pause, Pause::Until(_)) {
            // The minutes left in the tray (sent only when the text changes).
            self.update_tray();
        }
        // Windows closed by the user in another process (tmux popups).
        let gone: Vec<usize> = (0..self.wins.len()).filter(|&i| !self.out.alive(self.wins[i].id)).collect();
        for i in gone.into_iter().rev() {
            self.wins.remove(i).stop();
        }
        let now = Instant::now();
        // A replay keeps its window open, and a few seconds of live video follow it.
        for w in &mut self.wins {
            if w.shared.replay.load(Ordering::Relaxed) == 2 {
                w.deadline = w.deadline.max(now + Duration::from_secs(5));
            }
        }
        // A window under the mouse or pinned stays (desktop windows).
        let expired: Vec<usize> =
            (0..self.wins.len()).filter(|&i| now >= self.wins[i].deadline && !self.wins[i].hovered && !self.wins[i].pinned).collect();
        for i in expired.into_iter().rev() {
            self.close(i);
        }
        let settled: Vec<usize> = (0..self.wins.len())
            .filter(|&i| {
                let w = &self.wins[i];
                w.resized.is_some_and(|t| t.elapsed() > Duration::from_millis(700))
                    && (w.dec_size != w.size || w.dec_zoom != w.zoom)
                    && !self.out.panning(w.id)
            })
            .collect();
        for i in settled {
            self.wins[i].resized = None;
            self.restart_video(i);
        }
        if self.out.refresh() {
            self.info.lock().unwrap().monitors = self.out.monitors();
            for w in &mut self.wins {
                w.rect = None;
                w.user_moved = false;
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
        w.stop();
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
                self.set_pause(if self.paused() { Pause::Off } else { Pause::Forever });
            }
            b'm' | b'M' if self.cfg.camera_audio => {
                // Open tmux popups have their own m; new ones start muted.
                self.muted = !self.muted;
                crate::log!("Sound {}", if self.muted { "muted" } else { "on" });
                for w in &self.wins {
                    w.shared.muted.store(self.muted, Ordering::Relaxed);
                    w.shared.dirty.store(true, Ordering::Relaxed);
                }
                self.show_status();
            }
            b'r' | b'R' => {
                // Stops the replays that run; with none running, every window replays its event start.
                let running = self.wins.iter().any(|w| w.shared.replay.load(Ordering::Relaxed) == 2);
                let now = Instant::now();
                for w in &mut self.wins {
                    if (w.shared.replay.load(Ordering::Relaxed) == 2) == running {
                        w.shared.replay_pressed.store(true, Ordering::Relaxed);
                        w.deadline = w.deadline.max(now + Duration::from_secs(5));
                    }
                }
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
        self.update_tray();
        let mut state = self.status_line();
        if self.muted && self.cfg.camera_audio {
            state = format!("{state} · {}", t().console_sound_off);
        }
        let text = format!(" FrigatePiP {} · {state} · {}", env!("CARGO_PKG_VERSION"), t().console_keys);
        self.out.status(&text);
    }

    /// `stream` is the camera's main stream (larger than the sub stream).
    fn is_main(&self, camera: &str, stream: Option<&str>) -> bool {
        let info = self.cameras.iter().find(|c| c.name == camera);
        stream.is_some() && info.is_some_and(|c| c.main.as_deref() == stream)
    }

    /// `prebuffer`: keeps a connection to the sub stream of every camera (like on Windows),
    /// so a window shows live video at once instead of at the next key frame.
    fn update_hubs(&mut self) {
        if !self.cfg.prebuffer || self.cfg.live != LiveMode::Rtsp || !self.prebuffer_possible {
            if !self.hubs.is_empty() {
                crate::log!("Prebuffer disabled");
            }
            self.hubs.clear();
            return;
        }
        let mut want: HashMap<String, String> = HashMap::new();
        for c in &self.cameras {
            if !self.cfg.cameras.is_empty() && !self.cfg.cameras.iter().any(|x| x.eq_ignore_ascii_case(&c.name)) {
                continue;
            }
            if let Some(s) = c.sub.clone().or_else(|| c.main.clone()) {
                want.insert(c.name.clone(), self.cfg.rtsp_url(&s));
            }
        }
        let audio = self.cfg.camera_audio;
        self.hubs.retain(|k, h| want.get(k).is_some_and(|u| u == h.url()) && h.audio() == audio);
        for (cam, url) in want {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.hubs.entry(cam) {
                crate::log!("Prebuffer {}: {url}", slot.key());
                let hub = Arc::new(Hub::start(url, slot.key(), audio));
                slot.insert(hub);
            }
        }
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
        self.out.configure(&self.cfg);
        self.update_hubs();
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
    fn camera_list_shows_objects_and_zones() {
        i18n::set(i18n::Lang::En);
        let cam = |name: &str, title: &str, labels: &[&str], zones: &[&str]| CameraInfo {
            name: name.into(),
            title: title.into(),
            main: None,
            sub: None,
            aspect: 16.0 / 9.0,
            labels: labels.iter().map(|s| s.to_string()).collect(),
            zones: zones.iter().map(|s| s.to_string()).collect(),
        };
        let text = camera_list(
            "http://frigate:5000",
            &[cam("front_door", "Front door", &["person", "car"], &["driveway"]), cam("garden", "garden", &[], &[])],
        );
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Cameras in Frigate (http://frigate:5000): 2");
        assert_eq!(&lines[2..5], ["front_door (Front door)", "  objects: person, car", "  zones:   driveway"]);
        assert_eq!(&lines[6..9], ["garden", "  objects: —", "  zones:   —"]);
        assert!(lines[10].contains("[trigger]"));
    }

    #[test]
    fn autostart_entry_starts_the_desktop_windows() {
        let e = autostart_entry("/usr/bin/frigate-pip");
        assert!(e.starts_with("[Desktop Entry]\n"));
        assert!(e.contains("\nExec=/usr/bin/frigate-pip --output window --session\n"));
        assert!(e.contains("\nIcon=frigate-pip\n") && e.contains("\nTerminal=false\n"));
        assert!(autostart_entry("/opt/my apps/frigate-pip").contains("\nExec=\"/opt/my apps/frigate-pip\" --output window --session\n"));
    }

    #[test]
    fn muted_caption() {
        i18n::set(i18n::Lang::En);
        assert_eq!(with_sound("Garden · person", true), "Garden · person · sound off");
        assert_eq!(with_sound("Garden · person", false), "Garden · person");
    }

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
