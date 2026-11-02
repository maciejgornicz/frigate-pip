//! Settings of the desktop windows as a page in the browser: a small HTTP server on
//! 127.0.0.1 (a random port, and a random key in the address that every request must
//! carry) with the options and descriptions of the Windows settings window. Saving writes
//! config.ini (comments are kept); the running program applies it within two seconds.

use crate::config::{self, Config, Corner, IniDoc, LiveMode, Source, StreamPref, TriggerMode};
use crate::frigate::CameraInfo;
use crate::i18n::{self, t, Lang};
use crate::util::Cancel;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the page shows besides the settings file.
#[derive(Clone, Default)]
pub struct Info {
    pub cameras: Vec<CameraInfo>,
    /// Monitors in the order of their numbers in the `monitor` setting: (x, width, height, primary).
    pub monitors: Vec<(i32, u32, u32, bool)>,
    /// The program starts with the desktop session.
    pub autostart: bool,
}

pub struct Web {
    pub url: String,
}

/// Changes of the autostart checkbox go back to the program.
pub type OnAutostart = Arc<dyn Fn(bool) + Send + Sync>;

/// Starts the server; the page is at `Web::url`.
pub fn start(path: PathBuf, info: Arc<Mutex<Info>>, on_autostart: OnAutostart) -> Result<Web, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("settings page: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = token()?;
    let url = format!("http://127.0.0.1:{port}/{token}/");
    std::thread::Builder::new()
        .name("settings-page".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let (path, info, token, on_autostart) = (path.clone(), info.clone(), token.clone(), on_autostart.clone());
                let _ = std::thread::Builder::new().name("settings-request".into()).spawn(move || {
                    if let Err(e) = handle(stream, port, &token, &path, &info, &on_autostart) {
                        crate::log!("Settings page: {e}");
                    }
                });
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Web { url })
}

/// The key in the page address: 128 bits from the system's random source (not the
/// program's quick generator, whose numbers also travel over the network).
fn token() -> Result<String, String> {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map_err(|e| format!("settings page: /dev/urandom: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn read_request(s: &mut TcpStream) -> std::io::Result<Request> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = s.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::other("connection closed"));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        if buf.len() > 16 << 10 {
            return Err(std::io::Error::other("request too large"));
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (method, target) = (first.next().unwrap_or("").to_string(), first.next().unwrap_or("").to_string());
    let headers: Vec<(String, String)> = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    let mut body = buf[head_end + 4..].to_vec();
    let want = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
    if want > 256 << 10 {
        return Err(std::io::Error::other("request too large"));
    }
    while body.len() < want {
        let n = s.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(want);
    Ok(Request { method, target, headers, body })
}

fn respond(s: &mut TcpStream, status: &str, kind: &str, extra: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; \
         connect-src 'self'; form-action 'self'; frame-ancestors 'none'\r\n{extra}Connection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes())?;
    s.write_all(body)
}

fn handle(mut s: TcpStream, port: u16, token: &str, path: &Path, info: &Mutex<Info>, on_autostart: &OnAutostart) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    let req = read_request(&mut s)?;
    // Only this computer, only through our address (no DNS rebinding from a web page).
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !req.header("host").is_some_and(|h| hosts.iter().any(|x| x == h)) {
        return respond(&mut s, "403 Forbidden", "text/plain", "", b"forbidden");
    }
    let Some(rest) = req.target.strip_prefix(&format!("/{token}/")) else {
        return respond(&mut s, "404 Not Found", "text/plain", "", b"not found");
    };
    let (route, query) = rest.split_once('?').unwrap_or((rest, ""));
    match (req.method.as_str(), route) {
        ("GET", "") => {
            let html = page(&current(path), &info.lock().unwrap(), query.contains("saved=1").then_some(Ok(())));
            respond(&mut s, "200 OK", "text/html; charset=utf-8", "", html.as_bytes())
        }
        ("POST", "save") => {
            let origins = hosts.iter().map(|h| format!("http://{h}")).collect::<Vec<_>>();
            if req.header("origin").is_some_and(|o| !origins.iter().any(|x| x == o)) {
                return respond(&mut s, "403 Forbidden", "text/plain", "", b"forbidden");
            }
            let form = parse_form(&String::from_utf8_lossy(&req.body));
            let known = info.lock().unwrap().cameras.clone();
            let saved = config::load(&path.to_path_buf()).and_then(|(_, mut doc)| {
                let autostart = apply(&form, &mut doc, &known);
                std::fs::write(path, doc.to_text())?;
                Ok(autostart)
            });
            match saved {
                Ok(autostart) => {
                    crate::log!("Settings saved from the settings page");
                    if let Some(on) = autostart {
                        on_autostart(on);
                    }
                    respond(&mut s, "303 See Other", "text/plain", "Location: ./?saved=1\r\n", b"")
                }
                Err(e) => {
                    // The page again, with the reason (what was entered is not lost: Back).
                    let why = format!("{}: {e}", path.display());
                    crate::log!("Cannot save settings: {why}");
                    let html = page(&current(path), &info.lock().unwrap(), Some(Err(why)));
                    respond(&mut s, "500 Internal Server Error", "text/html; charset=utf-8", "", html.as_bytes())
                }
            }
        }
        ("GET", "test") => {
            // Like on Windows: the values from the form, nothing is saved.
            let form = parse_form(query);
            let (_, mut doc) = config::load(&path.to_path_buf())?;
            let known = info.lock().unwrap().cameras.clone();
            apply(&form, &mut doc, &known);
            let cfg = Config::from_ini(&doc);
            let mut out = Vec::new();
            crate::frigate::connection_test(&cfg, &Cancel::new(), &mut |line| out.push(line));
            respond(&mut s, "200 OK", "text/plain; charset=utf-8", "", out.join("\n").as_bytes())
        }
        _ => respond(&mut s, "404 Not Found", "text/plain", "", b"not found"),
    }
}

/// The settings as saved (the defaults when the file cannot be read).
fn current(path: &Path) -> Config {
    config::load(&path.to_path_buf()).map(|(c, _)| c).unwrap_or_else(|_| Config::from_ini(&IniDoc::parse(config::DEFAULT_INI)))
}

/// `application/x-www-form-urlencoded` (also a query string).
fn parse_form(s: &str) -> Vec<(String, String)> {
    let decode = |v: &str| crate::net::percent_decode(&v.replace('+', " "));
    s.split('&').filter(|p| !p.is_empty()).map(|p| p.split_once('=').unwrap_or((p, ""))).map(|(k, v)| (decode(k), decode(v))).collect()
}

/// Settings fields of the page: `section.key` as in config.ini.
const FIELDS: [&str; 26] = [
    "frigate.url",
    "frigate.source",
    "frigate.rtsp",
    "mqtt.host",
    "mqtt.port",
    "mqtt.user",
    "mqtt.password",
    "mqtt.prefix",
    "app.language",
    "trigger.mode",
    "trigger.ignore_stationary",
    "display.monitor",
    "display.position",
    "display.width",
    "display.margin",
    "display.duration",
    "display.max_duration",
    "display.max_popups",
    "display.replace",
    "display.overlay",
    "display.topmost",
    "display.sound",
    "display.skip_fullscreen",
    "video.live",
    "video.stream",
    "video.auto_main_width",
];

/// Puts the form values into the settings file; returns the autostart choice when the
/// form had it. Lists of checkboxes: everything checked = everything (also cameras and
/// objects added to Frigate later), as on Windows.
fn apply(form: &[(String, String)], doc: &mut IniDoc, known: &[CameraInfo]) -> Option<bool> {
    // Credentials exactly as typed (spaces included); the rest without outer spaces.
    let last = |name: &str| {
        form.iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| if matches!(name, "mqtt.user" | "mqtt.password") { v.clone() } else { v.trim().to_string() })
    };
    for f in FIELDS.iter().chain(["video.prebuffer", "video.camera_audio"].iter()) {
        if let (Some(v), Some((section, key))) = (last(f), f.split_once('.')) {
            // Quoted when needed: "abc #def" must not lose its end as a comment.
            doc.set(section, key, &config::ini_value(&v));
        }
    }
    if let Some(v) = last("trigger.min_score") {
        let pct: f32 = v.replace(',', ".").parse().unwrap_or(0.0);
        doc.set("trigger", "min_score", &format!("{}", (pct.clamp(0.0, 100.0) / 100.0 * 100.0).round() / 100.0));
    }
    let before = Config::from_ini(doc);
    let (cams, labels, _) = crate::view::choices(known, &before);
    for (name, all) in
        [("trigger.cameras", Some(cams.iter().map(|c| c.0.clone()).collect::<Vec<_>>())), ("trigger.labels", Some(labels)), ("trigger.zones", None)]
    {
        if !form.iter().any(|(k, _)| k == name) {
            continue;
        }
        let picked: Vec<String> = form.iter().filter(|(k, v)| k == name && !v.is_empty()).map(|(_, v)| v.clone()).collect();
        let everything = all.is_some_and(|all| !all.is_empty() && all.iter().all(|a| picked.iter().any(|p| p.eq_ignore_ascii_case(a))));
        let value = if everything { String::new() } else { picked.join(", ") };
        let (section, key) = name.split_once('.').unwrap_or_default();
        doc.set(section, key, &value);
    }
    // Normalized values (numbers, keys) as the program reads them.
    let cfg = Config::from_ini(doc);
    config::write_all(&cfg, doc);
    last("autostart").map(|v| v == "true")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn id(name: &str) -> String {
    name.replace('.', "-")
}

fn row(name: &str, label: &str, control: String, tip: &str) -> String {
    format!("<div class=\"row\"><label for=\"{}\">{}</label><div>{control}<p class=\"tip\">{}</p></div></div>\n", id(name), esc(label), esc(tip))
}

fn text(name: &str, value: &str, kind: &str) -> String {
    format!("<input type=\"{kind}\" id=\"{}\" name=\"{name}\" value=\"{}\" autocomplete=\"off\">", id(name), esc(value))
}

fn number(name: &str, value: impl std::fmt::Display, min: i32, max: i32, unit: &str) -> String {
    format!("<input type=\"number\" id=\"{}\" name=\"{name}\" value=\"{value}\" min=\"{min}\" max=\"{max}\" class=\"num\"> {}", id(name), esc(unit))
}

fn select(name: &str, options: &[(String, String)], selected: &str) -> String {
    let opts: String = options
        .iter()
        .map(|(v, l)| format!("<option value=\"{}\"{}>{}</option>", esc(v), if v == selected { " selected" } else { "" }, esc(l)))
        .collect();
    format!("<select id=\"{}\" name=\"{name}\">{opts}</select>", id(name))
}

/// A checkbox that also sends "false" when unchecked.
fn checkbox(name: &str, checked: bool, label: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{name}\" value=\"false\"><label class=\"check\"><input type=\"checkbox\" id=\"{}\" name=\"{name}\" value=\"true\"{}> {}</label>",
        id(name),
        if checked { " checked" } else { "" },
        esc(label)
    )
}

/// A list of checkboxes (the empty hidden value marks the list as sent).
fn checks(name: &str, items: &[(String, String, bool)], empty: &str) -> String {
    if items.is_empty() {
        return format!("<input type=\"hidden\" name=\"{name}\" value=\"\"><p class=\"tip\">{}</p>", esc(empty));
    }
    let boxes: String = items
        .iter()
        .map(|(v, l, c)| {
            format!(
                "<label class=\"check\"><input type=\"checkbox\" name=\"{name}\" value=\"{}\"{}> {}</label>",
                esc(v),
                if *c { " checked" } else { "" },
                esc(l)
            )
        })
        .collect();
    format!("<input type=\"hidden\" name=\"{name}\" value=\"\"><div class=\"checks\" id=\"{}\">{boxes}</div>", id(name))
}

fn opts(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect()
}

/// The settings page; `saved` = the result of the last save, shown above the form.
fn page(c: &Config, info: &Info, saved: Option<Result<(), String>>) -> String {
    let s = t();
    let lang = i18n::current();
    let all = |list: &[String], name: &str| list.is_empty() || list.iter().any(|x| x.eq_ignore_ascii_case(name));
    let (cams, labels, zones) = crate::view::choices(&info.cameras, c);
    let cam_items: Vec<(String, String, bool)> = cams.iter().map(|(n, title)| (n.clone(), title.clone(), all(&c.cameras, n))).collect();
    let label_items: Vec<(String, String, bool)> = labels
        .iter()
        .map(|l| {
            let name = i18n::label(l);
            (l.clone(), if name == l { l.clone() } else { format!("{name} ({l})") }, all(&c.labels, l))
        })
        .collect();
    let zone_items: Vec<(String, String, bool)> =
        zones.iter().map(|z| (z.clone(), z.clone(), c.zones.iter().any(|x| x.eq_ignore_ascii_case(z)))).collect();
    let mut languages = vec![("auto".to_string(), s.linux_language_auto.to_string())];
    languages.extend(Lang::ALL.iter().map(|l| (l.code().to_string(), l.native_name().to_string())));
    let mut monitors = vec![("primary".to_string(), s.monitor_primary.to_string())];
    for (i, (_, w, h, primary)) in info.monitors.iter().enumerate() {
        let text = i18n::fmt(s.monitor_item, &[&(i + 1), w, h]) + if *primary { s.monitor_primary_suffix } else { "" };
        monitors.push(((i + 1).to_string(), text));
    }
    let corners: Vec<(String, String)> = Corner::ALL
        .iter()
        .map(|c| {
            let label = match c {
                Corner::TopLeft => s.corner_top_left,
                Corner::TopRight => s.corner_top_right,
                Corner::BottomLeft => s.corner_bottom_left,
                Corner::BottomRight => s.corner_bottom_right,
            };
            (c.key().to_string(), label.to_string())
        })
        .collect();
    let streams: Vec<(String, String)> = StreamPref::ALL
        .iter()
        .map(|p| {
            let label = match p {
                StreamPref::Auto => s.stream_auto,
                StreamPref::Sub => s.stream_sub,
                StreamPref::Main => s.stream_main,
            };
            (p.key().to_string(), label.to_string())
        })
        .collect();
    let source = match c.source {
        Source::WebSocket => "websocket",
        Source::Mqtt => "mqtt",
    };
    let mode = match c.mode {
        TriggerMode::Events => "events",
        TriggerMode::Alerts => "alerts",
        TriggerMode::Reviews => "reviews",
    };
    let live = match c.live {
        LiveMode::Rtsp => "rtsp",
        LiveMode::Mjpeg => "mjpeg",
        LiveMode::Snapshot => "snapshot",
    };
    let replace = format!(
        "<label class=\"check\"><input type=\"radio\" name=\"display.replace\" value=\"true\"{}> {}</label><label class=\"check\"><input type=\"radio\" name=\"display.replace\" value=\"false\"{}> {}</label>",
        if c.replace { " checked" } else { "" },
        esc(s.replace_oldest),
        if c.replace { "" } else { " checked" },
        esc(s.skip_new)
    );
    let mut connection = String::new();
    connection += &row("frigate.url", s.frigate_url, text("frigate.url", &c.url, "url"), s.tip_url);
    connection += &row(
        "frigate.source",
        s.source,
        select("frigate.source", &opts(&[("websocket", s.source_websocket), ("mqtt", s.source_mqtt)]), source),
        s.tip_source,
    );
    connection += "<div id=\"mqtt\">";
    connection += &row(
        "mqtt.host",
        s.mqtt_server,
        format!("{} {} {}", text("mqtt.host", &c.mqtt_host, "text"), esc(s.port), number("mqtt.port", c.mqtt_port, 1, 65535, "")),
        &format!("{} {}", s.tip_mqtt_server, s.tip_mqtt_port),
    );
    connection += &row(
        "mqtt.user",
        s.mqtt_login,
        format!("{} {}", text("mqtt.user", &c.mqtt_user, "text"), text("mqtt.password", &c.mqtt_password, "password")),
        s.tip_mqtt_login,
    );
    connection += &row("mqtt.prefix", s.mqtt_prefix, text("mqtt.prefix", &c.mqtt_prefix, "text"), s.tip_mqtt_prefix);
    connection += "</div>";
    connection += &row("frigate.rtsp", s.rtsp, text("frigate.rtsp", &c.rtsp, "text"), &format!("{} — {}", s.rtsp_hint, s.tip_rtsp));
    connection += &format!(
        "<div class=\"row\"><span></span><div><button type=\"button\" id=\"test\">{}</button><p class=\"tip\">{}</p><pre id=\"out\">{}</pre></div></div>\n",
        esc(s.test_button),
        esc(s.tip_test),
        esc(s.test_placeholder)
    );
    connection += &row("app.language", s.language, select("app.language", &languages, &c.language), s.linux_tip_language);
    connection += &row("autostart", "", checkbox("autostart", info.autostart, s.linux_autostart), s.linux_tip_autostart);

    let mut detection = String::new();
    detection += &row(
        "trigger.mode",
        s.show_for,
        select("trigger.mode", &opts(&[("events", s.mode_events), ("alerts", s.mode_alerts), ("reviews", s.mode_reviews)]), mode),
        s.tip_mode,
    );
    detection += &row("trigger.cameras", s.cameras, checks("trigger.cameras", &cam_items, s.menu_no_cameras), s.tip_cameras);
    detection += &row("trigger.labels", s.objects, checks("trigger.labels", &label_items, "—"), s.tip_objects);
    detection += &row("trigger.zones", s.zones, checks("trigger.zones", &zone_items, "—"), s.tip_zones);
    detection += &format!("<p class=\"note\">{}</p>\n", esc(s.detection_note));
    detection += &row(
        "trigger.ignore_stationary",
        "",
        checkbox("trigger.ignore_stationary", c.ignore_stationary, s.ignore_stationary),
        s.tip_ignore_stationary,
    );
    detection += &row("trigger.min_score", s.min_score, number("trigger.min_score", (c.min_score * 100.0).round(), 0, 100, "%"), s.tip_min_score);

    let mut window = String::new();
    window += &row("display.monitor", s.monitor, select("display.monitor", &monitors, &c.monitor), s.tip_monitor);
    window += &row("display.position", s.corner, select("display.position", &corners, c.position.key()), s.tip_corner);
    window += &row("display.width", s.width, number("display.width", c.width, 64, 4000, s.px), s.linux_tip_width);
    window += &row("display.margin", s.margin, number("display.margin", c.margin, 0, 500, s.px), s.tip_margin);
    window += &row(
        "display.duration",
        s.duration,
        format!(
            "{} {}",
            number("display.duration", c.duration, 1, 3600, s.duration_max),
            number("display.max_duration", c.max_duration, 1, 86400, s.seconds)
        ),
        &format!("{} {}", s.tip_duration, s.tip_max_duration),
    );
    window += &row("display.max_popups", s.max_windows, number("display.max_popups", c.max_popups, 1, 16, ""), s.tip_max_windows);
    window += &row("display.replace", s.when_full, replace, s.tip_when_full);
    window += &row("display.topmost", "", checkbox("display.topmost", c.topmost, s.topmost), s.tip_topmost);
    window += &row("display.overlay", "", checkbox("display.overlay", c.overlay, s.overlay), s.tip_overlay);
    window += &row("display.sound", "", checkbox("display.sound", c.sound, s.sound), s.linux_tip_sound);
    window += &row("display.skip_fullscreen", "", checkbox("display.skip_fullscreen", c.skip_fullscreen, s.skip_fullscreen), s.tip_skip_fullscreen);
    window += &format!("<p class=\"note\">{}</p>\n", esc(s.linux_window_hint));

    let mut video = String::new();
    video += &row(
        "video.live",
        s.live,
        select("video.live", &opts(&[("rtsp", s.linux_live_rtsp), ("mjpeg", s.live_mjpeg), ("snapshot", s.live_snapshot)]), live),
        s.linux_tip_live,
    );
    video += &row("video.stream", s.stream, select("video.stream", &streams, c.stream.key()), s.tip_stream);
    video += &row(
        "video.auto_main_width",
        s.auto_main_width,
        number("video.auto_main_width", c.auto_main_width, 64, 8000, s.auto_main_hint),
        s.tip_auto_main_width,
    );
    video += &row("video.prebuffer", "", checkbox("video.prebuffer", c.prebuffer, s.prebuffer), s.tip_prebuffer);
    video += &row("video.camera_audio", "", checkbox("video.camera_audio", c.camera_audio, s.camera_audio), s.tip_camera_audio);

    let saved = match saved {
        Some(Ok(())) => format!("<p class=\"saved\">{}</p>", esc(s.linux_settings_saved)),
        Some(Err(why)) => format!("<p class=\"error\">{}</p>", esc(&i18n::fmt(s.linux_settings_not_saved, &[&why]))),
        None => String::new(),
    };
    let help = format!("https://github.com/maciejgornicz/frigate-pip/blob/main/docs/{}/user-guide.md", lang.code());
    format!(
        r#"<!doctype html>
<html lang="{code}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
:root {{ color-scheme: light dark; --bg: #f6f7f9; --card: #ffffff; --text: #1f2933; --muted: #5f6b7a; --line: #dde2e8; --accent: #2563eb; --ok: #15803d; --bad: #b91c1c; }}
@media (prefers-color-scheme: dark) {{ :root {{ --bg: #111418; --card: #1a1f25; --text: #e6e9ed; --muted: #9aa5b1; --line: #2c333b; --accent: #60a5fa; --ok: #4ade80; --bad: #f87171; }} }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; background: var(--bg); color: var(--text); font: 15px/1.45 system-ui, -apple-system, "Segoe UI", Ubuntu, Cantarell, sans-serif; }}
main {{ max-width: 860px; margin: 0 auto; padding: 24px 16px 96px; }}
header {{ display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }}
h1 {{ font-size: 22px; margin: 0 0 16px; }}
h2 {{ font-size: 17px; margin: 0 0 12px; }}
a {{ color: var(--accent); }}
section {{ background: var(--card); border: 1px solid var(--line); border-radius: 10px; padding: 18px 20px; margin-bottom: 16px; }}
.row {{ display: grid; grid-template-columns: 220px 1fr; gap: 6px 16px; padding: 8px 0; border-top: 1px solid var(--line); }}
.row:first-of-type {{ border-top: 0; }}
.row > label {{ font-weight: 600; padding-top: 6px; }}
.tip {{ margin: 4px 0 0; color: var(--muted); font-size: 13px; }}
.note {{ color: var(--muted); font-size: 13px; margin: 8px 0; }}
input[type=text], input[type=url], input[type=password], select {{ width: 100%; max-width: 420px; }}
input, select, button {{ font: inherit; color: inherit; background: var(--bg); border: 1px solid var(--line); border-radius: 6px; padding: 6px 8px; }}
input.num {{ width: 96px; }}
#mqtt-host {{ max-width: 240px; }} #mqtt-user, #mqtt-password {{ max-width: 200px; }}
label.check {{ display: inline-flex; align-items: center; gap: 6px; margin: 2px 16px 2px 0; font-weight: normal; }}
.checks {{ display: flex; flex-wrap: wrap; }}
button {{ cursor: pointer; }}
button.primary {{ background: var(--accent); border-color: var(--accent); color: #fff; padding: 8px 20px; font-weight: 600; }}
pre {{ white-space: pre-wrap; background: var(--bg); border: 1px solid var(--line); border-radius: 6px; padding: 8px 10px; margin: 8px 0 0; font-size: 13px; }}
.bar {{ position: fixed; left: 0; right: 0; bottom: 0; background: var(--card); border-top: 1px solid var(--line); padding: 12px 16px; text-align: center; }}
.saved {{ color: var(--ok); font-weight: 600; margin: 0 0 12px; }}
.error {{ color: var(--bad); font-weight: 600; margin: 0 0 12px; }}
@media (max-width: 640px) {{ .row {{ grid-template-columns: 1fr; }} .row > label {{ padding-top: 0; }} }}
</style>
</head>
<body>
<main>
<header><h1>{title}</h1><a href="{help}" target="_blank" rel="noreferrer">{help_text}</a></header>
{saved}
<form method="post" action="save" id="form">
<section><h2>{tab_connection}</h2>
{connection}</section>
<section><h2>{tab_detection}</h2>
{detection}</section>
<section><h2>{tab_window}</h2>
{window}</section>
<section><h2>{tab_video}</h2>
{video}</section>
<div class="bar"><button type="submit" class="primary">{apply}</button></div>
</form>
</main>
<script>
const form = document.getElementById("form");
const source = document.getElementById("frigate-source");
const mqtt = document.getElementById("mqtt");
const show = () => {{ mqtt.style.display = source.value === "mqtt" ? "" : "none"; }};
source.addEventListener("change", show); show();
document.getElementById("test").addEventListener("click", async () => {{
  const out = document.getElementById("out");
  out.textContent = "…";
  try {{
    const r = await fetch("test?" + new URLSearchParams(new FormData(form)));
    out.textContent = await r.text();
  }} catch (e) {{ out.textContent = String(e); }}
}});
</script>
</body>
</html>
"#,
        code = lang.code(),
        title = esc(s.settings_title),
        help_text = esc(s.help),
        tab_connection = esc(s.tab_connection),
        tab_detection = esc(s.tab_detection),
        tab_window = esc(s.tab_window),
        tab_video = esc(s.tab_video),
        apply = esc(s.apply),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(name: &str, labels: &[&str]) -> CameraInfo {
        CameraInfo {
            name: name.into(),
            title: name.into(),
            main: None,
            sub: None,
            aspect: 1.78,
            labels: labels.iter().map(|s| s.to_string()).collect(),
            zones: vec!["drive".into()],
        }
    }

    #[test]
    fn form_decoding() {
        let f = parse_form("frigate.url=http%3A%2F%2F10.0.0.5%3A5000&mqtt.user=a+b&x=%C4%85&empty=");
        assert_eq!(f[0], ("frigate.url".into(), "http://10.0.0.5:5000".into()));
        assert_eq!(f[1].1, "a b");
        assert_eq!(f[2].1, "ą");
        assert_eq!(f[3], ("empty".into(), String::new()));
    }

    #[test]
    fn saving_keeps_comments_and_normalizes() {
        let mut doc = IniDoc::parse(config::DEFAULT_INI);
        let known = [cam("front", &["person", "car"]), cam("garden", &["person"])];
        let form = parse_form(
            "frigate.url=http%3A%2F%2F10.0.0.5%3A5000&display.overlay=false&display.overlay=true&video.camera_audio=false&video.prebuffer=false&video.prebuffer=true\
             &trigger.cameras=&trigger.cameras=front&trigger.labels=&trigger.labels=person&trigger.labels=car\
             &trigger.zones=&trigger.zones=drive&trigger.min_score=55&autostart=false&autostart=true&display.width=abc",
        );
        assert_eq!(apply(&form, &mut doc, &known), Some(true));
        let c = Config::from_ini(&doc);
        assert_eq!(c.url, "http://10.0.0.5:5000");
        assert!(c.overlay && !c.camera_audio && c.prebuffer);
        // One camera of two: a list; every known object: "all" (empty).
        assert_eq!(c.cameras, vec!["front".to_string()]);
        assert!(c.labels.is_empty());
        assert_eq!(c.zones, vec!["drive".to_string()]);
        assert!((c.min_score - 0.55).abs() < 1e-6);
        // A bad number keeps the default.
        assert_eq!(c.width, 480);
        assert!(doc.to_text().contains("; FrigatePiP settings"));
    }

    #[test]
    fn credentials_are_kept_exactly() {
        let mut doc = IniDoc::parse(config::DEFAULT_INI);
        let form = parse_form("mqtt.user=+me+&mqtt.password=+ab%3Bc+%23d+&frigate.url=+http%3A%2F%2Fx%3A5000+");
        apply(&form, &mut doc, &[]);
        let c = Config::from_ini(&doc);
        assert_eq!((c.mqtt_user.as_str(), c.mqtt_password.as_str(), c.url.as_str()), (" me ", " ab;c #d ", "http://x:5000"));
        // Saved again (e.g. another setting changed): still the same.
        let c2 = Config::from_ini(&IniDoc::parse(&doc.to_text()));
        assert_eq!(c2.mqtt_password, " ab;c #d ");
    }

    #[test]
    fn page_key_is_random() {
        let (a, b) = (token().unwrap(), token().unwrap());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn page_shows_the_settings_safely() {
        i18n::set(Lang::En);
        let mut doc = IniDoc::parse(config::DEFAULT_INI);
        doc.set("frigate", "url", "http://x/\"<script>");
        let c = Config::from_ini(&doc);
        let info = Info {
            cameras: vec![cam("front_door", &["person"])],
            // Numbered by the output (from the left).
            monitors: vec![(-1920, 1280, 1024, false), (0, 1920, 1080, true)],
            autostart: true,
        };
        let html = page(&c, &info, Some(Ok(())));
        assert!(html.contains("&lt;script&gt;") && !html.contains("\"<script>"));
        assert!(html.contains("name=\"trigger.cameras\" value=\"front_door\" checked"));
        // Monitors are numbered from the left.
        assert!(html.contains("<option value=\"1\">Monitor 1 – 1280×1024</option>"));
        assert!(html.contains("Monitor 2 – 1920×1080 (primary)"));
        assert!(html.contains("id=\"autostart\" name=\"autostart\" value=\"true\" checked"));
        let failed = page(&c, &info, Some(Err("/x/config.ini: <denied>".into())));
        assert!(failed.contains("class=\"error\">Not saved: /x/config.ini: &lt;denied&gt;</p>"));
    }
}
