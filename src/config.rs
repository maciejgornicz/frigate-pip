//! Settings in an INI file (comments are preserved when saving from the menu).

use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------- INI document

#[derive(Clone, Debug, Default)]
pub struct IniDoc {
    pub lines: Vec<String>,
}

enum Line<'a> {
    Section(&'a str),
    Pair(&'a str, &'a str),
    Other,
}

fn classify(line: &str) -> Line<'_> {
    let t = line.trim();
    if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
        return Line::Other;
    }
    if t.starts_with('[') && t.ends_with(']') {
        return Line::Section(t[1..t.len() - 1].trim());
    }
    match t.split_once('=') {
        Some((k, v)) => Line::Pair(k.trim(), v),
        None => Line::Other,
    }
}

fn clean_value(raw: &str) -> String {
    let v = raw.trim();
    if let Some(rest) = v.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
    }
    let mut cut = v.len();
    for pat in [" ;", " #", "\t;", "\t#"] {
        if let Some(p) = v.find(pat) {
            cut = cut.min(p);
        }
    }
    v[..cut].trim().to_string()
}

impl IniDoc {
    pub fn parse(text: &str) -> Self {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        IniDoc { lines: text.lines().map(|l| l.to_string()).collect() }
    }

    pub fn to_text(&self) -> String {
        let mut s = self.lines.join("\r\n");
        s.push_str("\r\n");
        s
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        let mut cur = "";
        for l in &self.lines {
            match classify(l) {
                Line::Section(s) => cur = s,
                Line::Pair(k, v) if cur.eq_ignore_ascii_case(section) && k.eq_ignore_ascii_case(key) => {
                    return Some(clean_value(v));
                }
                _ => {}
            }
        }
        None
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let new_line = format!("{key} = {value}");
        let mut cur = String::new();
        let mut section_found = false;
        let mut insert_at = None;
        for (i, l) in self.lines.iter().enumerate() {
            match classify(l) {
                Line::Section(s) => {
                    cur = s.to_string();
                    if cur.eq_ignore_ascii_case(section) {
                        section_found = true;
                        insert_at = Some(i + 1);
                    }
                }
                Line::Pair(k, _) if cur.eq_ignore_ascii_case(section) => {
                    if k.eq_ignore_ascii_case(key) {
                        self.lines[i] = new_line;
                        return;
                    }
                    insert_at = Some(i + 1);
                }
                _ => {}
            }
        }
        match (section_found, insert_at) {
            (true, Some(i)) => self.lines.insert(i, new_line),
            _ => {
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.push(format!("[{section}]"));
                self.lines.push(new_line);
            }
        }
    }

    pub fn sections(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|l| match classify(l) {
                Line::Section(s) => Some(s.to_string()),
                _ => None,
            })
            .collect()
    }
}

// ---------------------------------------------------------------- settings

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    WebSocket,
    Mqtt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriggerMode {
    Events,
    Alerts,
    Reviews,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight];

    pub fn key(self) -> &'static str {
        match self {
            Corner::TopLeft => "top-left",
            Corner::TopRight => "top-right",
            Corner::BottomLeft => "bottom-left",
            Corner::BottomRight => "bottom-right",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LiveMode {
    Rtsp,
    Mjpeg,
    Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StreamPref {
    Auto,
    Sub,
    Main,
}

impl StreamPref {
    pub const ALL: [StreamPref; 3] = [StreamPref::Auto, StreamPref::Sub, StreamPref::Main];

    pub fn key(self) -> &'static str {
        match self {
            StreamPref::Auto => "auto",
            StreamPref::Sub => "sub",
            StreamPref::Main => "main",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CamOverride {
    pub name: Option<String>,
    pub main: Option<String>,
    pub sub: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub url: String,
    pub source: Source,
    pub rtsp: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_user: String,
    pub mqtt_password: String,
    pub mqtt_prefix: String,

    pub mode: TriggerMode,
    pub cameras: Vec<String>,
    pub labels: Vec<String>,
    pub zones: Vec<String>,
    pub ignore_stationary: bool,
    pub min_score: f32,

    pub monitor: String,
    pub position: Corner,
    pub margin: i32,
    pub width: i32,
    pub duration: u32,
    pub max_duration: u32,
    pub topmost: bool,
    pub sound: bool,
    pub overlay: bool,
    pub max_popups: usize,
    /// When all windows are in use: a new one replaces the oldest (`true`) or is skipped.
    pub replace: bool,
    pub skip_fullscreen: bool,

    pub live: LiveMode,
    pub stream: StreamPref,
    pub auto_main_width: i32,
    pub prebuffer: bool,
    pub gpu_warm: bool,
    pub camera_audio: bool,
    /// Interface language: `auto` (Windows) or a language code.
    pub language: String,
    /// Once a day ask GitHub whether a new version exists.
    pub check_updates: bool,

    pub overrides: HashMap<String, CamOverride>,
}

fn list(v: Option<String>) -> Vec<String> {
    v.map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default()
}

fn boolean(v: Option<String>, default: bool) -> bool {
    match v.as_deref().map(|s| s.to_ascii_lowercase()) {
        Some(s) if ["true", "1", "yes", "tak", "on"].contains(&s.as_str()) => true,
        Some(s) if ["false", "0", "no", "nie", "off"].contains(&s.as_str()) => false,
        _ => default,
    }
}

fn num<T: std::str::FromStr>(v: Option<String>, default: T) -> T {
    v.and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn opt(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty())
}

impl Config {
    pub fn from_ini(d: &IniDoc) -> Config {
        let g = |s: &str, k: &str| d.get(s, k);
        let mut overrides = HashMap::new();
        for sec in d.sections() {
            if let Some(cam) = sec.strip_prefix("camera.") {
                overrides.insert(cam.to_string(), CamOverride { name: opt(g(&sec, "name")), main: opt(g(&sec, "main")), sub: opt(g(&sec, "sub")) });
            }
        }
        Config {
            url: g("frigate", "url").unwrap_or_default().trim_end_matches('/').to_string(),
            source: match g("frigate", "source").as_deref() {
                Some("mqtt") => Source::Mqtt,
                _ => Source::WebSocket,
            },
            rtsp: opt(g("frigate", "rtsp")).unwrap_or_else(|| "rtsp://{host}:8554/{stream}".into()),
            mqtt_host: g("mqtt", "host").unwrap_or_default(),
            mqtt_port: num(g("mqtt", "port"), 1883),
            mqtt_user: g("mqtt", "user").unwrap_or_default(),
            mqtt_password: g("mqtt", "password").unwrap_or_default(),
            mqtt_prefix: opt(g("mqtt", "prefix")).unwrap_or_else(|| "frigate".into()),

            mode: match g("trigger", "mode").as_deref() {
                Some("alerts") => TriggerMode::Alerts,
                Some("reviews") => TriggerMode::Reviews,
                _ => TriggerMode::Events,
            },
            cameras: list(g("trigger", "cameras")),
            labels: list(g("trigger", "labels")),
            zones: list(g("trigger", "zones")),
            ignore_stationary: boolean(g("trigger", "ignore_stationary"), true),
            min_score: num(g("trigger", "min_score"), 0.0),

            monitor: opt(g("display", "monitor")).unwrap_or_else(|| "primary".into()),
            position: match g("display", "position").as_deref() {
                Some("top-left") => Corner::TopLeft,
                Some("top-right") => Corner::TopRight,
                Some("bottom-left") => Corner::BottomLeft,
                _ => Corner::BottomRight,
            },
            margin: num(g("display", "margin"), 16).clamp(0, 500),
            width: num(g("display", "width"), 480).clamp(160, 3840),
            duration: num(g("display", "duration"), 15).clamp(1, 3600),
            max_duration: num(g("display", "max_duration"), 120).clamp(1, 36000),
            topmost: boolean(g("display", "topmost"), true),
            sound: boolean(g("display", "sound"), false),
            overlay: boolean(g("display", "overlay"), true),
            max_popups: num(g("display", "max_popups"), 4usize).clamp(1, 16),
            replace: boolean(g("display", "replace"), true),
            skip_fullscreen: boolean(g("display", "skip_fullscreen"), false),

            live: match g("video", "live").as_deref() {
                Some("mjpeg") => LiveMode::Mjpeg,
                Some("snapshot") => LiveMode::Snapshot,
                _ => LiveMode::Rtsp,
            },
            stream: match g("video", "stream").as_deref() {
                Some("sub") => StreamPref::Sub,
                Some("main") => StreamPref::Main,
                _ => StreamPref::Auto,
            },
            auto_main_width: num(g("video", "auto_main_width"), 800),
            prebuffer: boolean(g("video", "prebuffer"), false),
            gpu_warm: boolean(g("video", "gpu_warm"), false),
            camera_audio: boolean(g("video", "camera_audio"), false),
            language: opt(g("app", "language")).unwrap_or_else(|| "auto".into()),
            check_updates: boolean(g("app", "check_updates"), true),

            overrides,
        }
    }

    /// Host from the Frigate URL (for the RTSP template).
    pub fn host(&self) -> String {
        crate::net::Url::parse(&self.url).map(|u| u.host).unwrap_or_default()
    }

    pub fn rtsp_url(&self, stream: &str) -> String {
        self.rtsp.replace("{host}", &self.host()).replace("{stream}", stream)
    }
}

impl Source {
    pub fn key(self) -> &'static str {
        match self {
            Source::WebSocket => "websocket",
            Source::Mqtt => "mqtt",
        }
    }
}

impl TriggerMode {
    pub const ALL: [TriggerMode; 3] = [TriggerMode::Events, TriggerMode::Alerts, TriggerMode::Reviews];

    pub fn key(self) -> &'static str {
        match self {
            TriggerMode::Events => "events",
            TriggerMode::Alerts => "alerts",
            TriggerMode::Reviews => "reviews",
        }
    }
}

impl LiveMode {
    pub const ALL: [LiveMode; 3] = [LiveMode::Rtsp, LiveMode::Mjpeg, LiveMode::Snapshot];

    pub fn key(self) -> &'static str {
        match self {
            LiveMode::Rtsp => "rtsp",
            LiveMode::Mjpeg => "mjpeg",
            LiveMode::Snapshot => "snapshot",
        }
    }
}

fn flag(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

/// Writes all settings to the INI document (existing lines and comments are kept).
pub fn write_all(c: &Config, d: &mut IniDoc) {
    let quoted = |v: &str| if v.contains([';', '#']) || v != v.trim() { format!("\"{v}\"") } else { v.to_string() };
    let pairs: [(&str, &str, String); 34] = [
        ("frigate", "url", c.url.clone()),
        ("frigate", "source", c.source.key().into()),
        ("frigate", "rtsp", c.rtsp.clone()),
        ("mqtt", "host", c.mqtt_host.clone()),
        ("mqtt", "port", c.mqtt_port.to_string()),
        ("mqtt", "user", quoted(&c.mqtt_user)),
        ("mqtt", "password", quoted(&c.mqtt_password)),
        ("mqtt", "prefix", c.mqtt_prefix.clone()),
        ("trigger", "mode", c.mode.key().into()),
        ("trigger", "cameras", c.cameras.join(", ")),
        ("trigger", "labels", c.labels.join(", ")),
        ("trigger", "zones", c.zones.join(", ")),
        ("trigger", "ignore_stationary", flag(c.ignore_stationary).into()),
        ("trigger", "min_score", format!("{}", (c.min_score * 100.0).round() / 100.0)),
        ("display", "monitor", c.monitor.clone()),
        ("display", "position", c.position.key().into()),
        ("display", "margin", c.margin.to_string()),
        ("display", "width", c.width.to_string()),
        ("display", "duration", c.duration.to_string()),
        ("display", "max_duration", c.max_duration.to_string()),
        ("display", "topmost", flag(c.topmost).into()),
        ("display", "sound", flag(c.sound).into()),
        ("display", "overlay", flag(c.overlay).into()),
        ("display", "max_popups", c.max_popups.to_string()),
        ("display", "replace", flag(c.replace).into()),
        ("display", "skip_fullscreen", flag(c.skip_fullscreen).into()),
        ("video", "live", c.live.key().into()),
        ("video", "stream", c.stream.key().into()),
        ("video", "auto_main_width", c.auto_main_width.to_string()),
        ("video", "prebuffer", flag(c.prebuffer).into()),
        ("video", "gpu_warm", flag(c.gpu_warm).into()),
        ("video", "camera_audio", flag(c.camera_audio).into()),
        ("app", "language", c.language.clone()),
        ("app", "check_updates", flag(c.check_updates).into()),
    ];
    for (section, key, value) in pairs {
        d.set(section, key, &value);
    }
}

pub fn config_path() -> PathBuf {
    crate::util::app_dir().join("config.ini")
}

/// Loads the settings file. A default one is created only if the file is missing — any other
/// read error (e.g. the file briefly locked by an editor) never overwrites it.
pub fn load(path: &PathBuf) -> std::io::Result<(Config, IniDoc)> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(path));
            let _ = std::fs::write(path, DEFAULT_INI.replace('\n', "\r\n"));
            DEFAULT_INI.to_string()
        }
        Err(e) => return Err(e),
    };
    let doc = IniDoc::parse(&text);
    Ok((Config::from_ini(&doc), doc))
}

pub fn save(path: &PathBuf, doc: &IniDoc) {
    if let Err(e) = std::fs::write(path, doc.to_text()) {
        crate::log!("Cannot save settings: {e}");
    }
}

pub const DEFAULT_INI: &str = "\
; FrigatePiP settings
; On Windows everything here can also be changed in the settings window
; (left-click the tray icon). Changes saved in this file are applied automatically.

[app]
; auto = Windows display language (English if not supported), or: en, pl, de, es, pt, fr
language = auto
; Once a day, ask GitHub whether a new version exists (true / false)
check_updates = true

[frigate]
; Address of the Frigate server. Port 5000 does not require logging in.
url = http://frigate.local:5000
; Event source: websocket (recommended, nothing else needed) or mqtt
source = websocket
; RTSP address of go2rtc. {host} = host from url, {stream} = go2rtc stream name
rtsp = rtsp://{host}:8554/{stream}

[mqtt]
; Used only when source = mqtt
host =
port = 1883
user =
password =
prefix = frigate

[trigger]
; events  = every object tracked by Frigate (fastest)
; alerts  = only Review alerts
; reviews = Review alerts and detections
mode = events
; Empty = all. Comma-separated lists, e.g.: labels = person, car
cameras =
labels =
; Required zones (empty = anywhere in the frame)
zones =
; Objects that are not moving (e.g. a parked car) do not open windows
ignore_stationary = true
; Minimum confidence 0.0-1.0 (0 = Frigate's own thresholds)
min_score = 0

[display]
; primary = primary monitor, 1, 2, 3... = monitor number from the left
monitor = primary
; top-left, top-right, bottom-left, bottom-right
position = bottom-right
; Distance from the screen edge (px)
margin = 16
; Window width in pixels (the height follows the picture's aspect ratio)
width = 480
; How many seconds a window stays after the object's last movement
duration = 15
; Maximum time (s) a window is shown, even if the object keeps moving
max_duration = 120
; Keep windows above all other windows
topmost = true
; Short system sound when a window appears (not the camera audio)
sound = false
; Camera, object and time caption on the picture
overlay = true
; How many windows (cameras) at once
max_popups = 4
; When all windows are in use: true = the new event replaces the oldest window
; (same place on the screen), false = the new event is skipped
replace = true
; Do not show windows while a full-screen game or presentation is running
skip_fullscreen = false

[video]
; rtsp     = smooth video from go2rtc (hardware decoding)
; mjpeg    = MJPEG from Frigate (about 5 fps, detect resolution)
; snapshot = refreshed snapshots only
live = rtsp
; auto = sub stream for a small window, main for a wide one (> auto_main_width px)
; sub / main = always this stream
stream = auto
auto_main_width = 800
; Prebuffer: keeps a connection to every camera's sub stream so live video
; starts at once (~0.15 s) instead of at the next key frame (up to ~1 s).
; Cost: about 0.1-0.5 Mbit/s of local network traffic per camera, all the time.
prebuffer = false
; Keep the graphics device ready: video starts ~130 ms sooner,
; at the cost of ~1% of one CPU core while idle (GPU driver thread).
gpu_warm = false
; Camera audio in the preview window (AAC / G.711 from the RTSP stream).
; Played on its own thread - it never delays the window or the video.
camera_audio = false

[console]
; Linux console version only (not used on Windows):
; output = auto, framebuffer, terminal or tmux
; graphics = auto, kitty, sixel or blocks (pictures in a terminal)
; framebuffer = /dev/fb0
; ffmpeg = ffmpeg
; font = TrueType font for captions on the framebuffer (default: DejaVu Sans Bold)

; Per-camera overrides (optional):
; [camera.front_door]
; name = Front door
; main = front_door
; sub = front_door_sub
";

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "; komentarz\n[frigate]\nurl = http://10.0.0.5:5000/ ; adres\nsource = mqtt\n\n[display]\n; szerokość\nwidth = 640\ntopmost = nie\n\n[camera.brama]\nname = \"Brama ; wjazd\"\nsub = brama_sub\n";

    #[test]
    fn reads_values_and_strips_comments() {
        let d = IniDoc::parse(SAMPLE);
        assert_eq!(d.get("frigate", "url").as_deref(), Some("http://10.0.0.5:5000/"));
        assert_eq!(d.get("FRIGATE", "Source").as_deref(), Some("mqtt"));
        assert_eq!(d.get("camera.brama", "name").as_deref(), Some("Brama ; wjazd"));
        assert_eq!(d.get("display", "missing"), None);
        let c = Config::from_ini(&d);
        assert_eq!(c.url, "http://10.0.0.5:5000");
        assert_eq!(c.source, Source::Mqtt);
        assert_eq!(c.width, 640);
        assert!(!c.topmost);
        assert_eq!(c.position, Corner::BottomRight);
        assert_eq!(c.overrides["brama"].sub.as_deref(), Some("brama_sub"));
        assert_eq!(c.overrides["brama"].main, None);
        assert_eq!(c.rtsp_url("brama_sub"), "rtsp://10.0.0.5:8554/brama_sub");
    }

    #[test]
    fn set_replaces_in_place_and_keeps_comments() {
        let mut d = IniDoc::parse(SAMPLE);
        d.set("display", "width", "800");
        assert_eq!(d.get("display", "width").as_deref(), Some("800"));
        assert!(d.lines.iter().any(|l| l == "; szerokość"));
        let idx = d.lines.iter().position(|l| l == "width = 800").unwrap();
        assert_eq!(d.lines[idx - 1], "; szerokość");
    }

    #[test]
    fn set_appends_to_existing_and_new_sections() {
        let mut d = IniDoc::parse(SAMPLE);
        d.set("display", "sound", "true");
        let sound = d.lines.iter().position(|l| l == "sound = true").unwrap();
        let topmost = d.lines.iter().position(|l| l == "topmost = nie").unwrap();
        assert_eq!(sound, topmost + 1);
        d.set("video", "stream", "main");
        assert_eq!(d.get("video", "stream").as_deref(), Some("main"));
        assert!(d.lines.contains(&"[video]".to_string()));
    }

    #[test]
    fn lists_and_defaults() {
        let d = IniDoc::parse("[trigger]\nlabels = person,  car ,\n");
        let c = Config::from_ini(&d);
        assert_eq!(c.labels, vec!["person", "car"]);
        assert!(c.cameras.is_empty());
        assert_eq!(c.duration, 15);
        assert_eq!(c.live, LiveMode::Rtsp);
        assert_eq!(c.stream, StreamPref::Auto);
        assert!(c.ignore_stationary);
    }

    #[test]
    fn write_all_roundtrips_every_setting_and_keeps_comments() {
        let mut doc = IniDoc::parse(DEFAULT_INI);
        let mut c = Config::from_ini(&doc);
        c.url = "http://10.0.0.9:5000".into();
        c.source = Source::Mqtt;
        c.rtsp = "rtsp://cam:554/{stream}".into();
        c.mqtt_host = "broker.local".into();
        c.mqtt_port = 1884;
        c.mqtt_user = "u".into();
        c.mqtt_password = "p ;x".into();
        c.mqtt_prefix = "fr".into();
        c.mode = TriggerMode::Alerts;
        c.cameras = vec!["a".into(), "b".into()];
        c.labels = vec!["person".into()];
        c.zones = vec!["Droga".into()];
        c.ignore_stationary = false;
        c.min_score = 0.65;
        c.monitor = "2".into();
        c.position = Corner::TopLeft;
        c.margin = 30;
        c.width = 640;
        c.duration = 20;
        c.max_duration = 90;
        c.topmost = false;
        c.sound = true;
        c.overlay = false;
        c.max_popups = 2;
        c.replace = false;
        c.skip_fullscreen = true;
        c.live = LiveMode::Mjpeg;
        c.stream = StreamPref::Main;
        c.auto_main_width = 900;
        c.prebuffer = true;
        c.gpu_warm = true;
        c.camera_audio = true;
        c.language = "de".into();
        c.check_updates = false;
        write_all(&c, &mut doc);
        let back = Config::from_ini(&IniDoc::parse(&doc.to_text()));
        assert_eq!(back, c);
        assert!(doc.lines.iter().any(|l| l.starts_with("; Address of the Frigate server")));
    }

    #[test]
    #[cfg(windows)]
    fn load_never_overwrites_unreadable_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!("fpip-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.ini");
        let _ = std::fs::remove_file(&path);
        // No file: a default one is created.
        let (c, _) = load(&path).unwrap();
        assert_eq!(c.url, "http://frigate.local:5000");
        std::fs::write(&path, "[frigate]\nurl = http://10.1.1.1:5000\n").unwrap();
        // File locked (e.g. while an editor is saving it): error, no overwrite.
        let lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(&path).unwrap();
        assert!(load(&path).is_err());
        drop(lock);
        assert_eq!(load(&path).unwrap().0.url, "http://10.1.1.1:5000");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_file_parses() {
        let c = Config::from_ini(&IniDoc::parse(DEFAULT_INI));
        assert_eq!(c.url, "http://frigate.local:5000");
        assert_eq!(c.mode, TriggerMode::Events);
        assert_eq!(c.width, 480);
        assert!(!c.prebuffer);
        assert!(c.overrides.is_empty());
    }
}
