//! Settings of the desktop windows as a page in the browser: a small HTTP server on
//! 127.0.0.1 (a random port, and a random key in the address that every request must
//! carry) with the options and descriptions of the Windows settings window. Saving writes
//! config.ini (comments are kept); the running program applies it within two seconds.

use crate::config::{self, Config, Corner, IniDoc, LiveMode, Source, StreamPref, TriggerMode};
use crate::frigate::CameraInfo;
use crate::i18n::{self, t, Lang};
use crate::util::Cancel;
use crate::view::{label_text, listed};
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

/// `Referrer-Policy: same-origin`: other sites get no referrer (the address carries the
/// key). Not `no-referrer`: Chromium then sends `Origin: null` with the form, refused.
fn respond(s: &mut TcpStream, status: &str, kind: &str, extra: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: same-origin\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; \
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
            let html = page(&current(path), &info.lock().unwrap(), query.contains("saved=1").then_some(Ok(())), &[]);
            respond(&mut s, "200 OK", "text/html; charset=utf-8", "", html.as_bytes())
        }
        ("POST", "save") => {
            let origins = hosts.iter().map(|h| format!("http://{h}")).collect::<Vec<_>>();
            if req.header("origin").is_some_and(|o| !origins.iter().any(|x| x == o)) {
                return respond(&mut s, "403 Forbidden", "text/plain", "", b"forbidden");
            }
            let form = parse_form(&String::from_utf8_lossy(&req.body));
            let known = info.lock().unwrap().cameras.clone();
            // An action URL typed that is not valid stops the whole save, as on Windows.
            let saved = config::load(&path.to_path_buf()).and_then(|(before, mut doc)| {
                let bad = bad_urls(&form, &before, &known);
                let autostart = apply(&form, &mut doc, &known);
                if bad.is_empty() {
                    std::fs::write(path, doc.to_text())?;
                }
                Ok((autostart, bad, doc))
            });
            match saved {
                Ok((autostart, bad, doc)) if !bad.is_empty() => {
                    // The page again with everything as typed (not saved) and the reason by
                    // each such row. Never the URL itself in the log.
                    crate::log!("Settings not saved: an action URL is not valid");
                    let mut shown = info.lock().unwrap().clone();
                    shown.autostart = autostart.unwrap_or(shown.autostart);
                    let first = &bad[0];
                    let row = format!("{} – {}", first.title, i18n::fmt(t().action_row, &[&first.n]));
                    let why = i18n::fmt(t().action_row_error, &[&row, &t().action_bad_url]);
                    let html = page(&Config::from_ini(&doc), &shown, Some(Err(why)), &bad);
                    respond(&mut s, "422 Unprocessable Content", "text/html; charset=utf-8", "", html.as_bytes())
                }
                Ok((autostart, _, _)) => {
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
                    let html = page(&current(path), &info.lock().unwrap(), Some(Err(why)), &[]);
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
const FIELDS: [&str; 29] = [
    "frigate.url",
    "frigate.user",
    "frigate.password",
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
    "video.camera_volume",
];

/// Puts the form values into the settings file; returns the autostart choice when the
/// form had it. Lists of checkboxes: everything checked = everything (also cameras and
/// objects added to Frigate later), as on Windows.
fn apply(form: &[(String, String)], doc: &mut IniDoc, known: &[CameraInfo]) -> Option<bool> {
    // Credentials exactly as typed (spaces included); the rest without outer spaces.
    let last = |name: &str| {
        form.iter().rev().find(|(k, _)| k == name).map(|(_, v)| {
            if matches!(name, "mqtt.user" | "mqtt.password" | "frigate.password") {
                v.clone()
            } else {
                v.trim().to_string()
            }
        })
    };
    for f in FIELDS.iter().chain(["video.prebuffer", "video.camera_audio", "video.wind_filter"].iter()) {
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
    for (name, all) in [
        ("trigger.cameras", Some(cams.iter().map(|c| c.0.clone()).collect::<Vec<_>>())),
        ("trigger.labels", Some(labels.clone())),
        ("trigger.zones", None),
    ] {
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
    let mut cfg = Config::from_ini(doc);
    // A camera's objects. A camera that follows the list for all cameras (follow = 1, the
    // page keeps it up to date) gets its own list only when its checks differ from that
    // one; a list of its own (follow = 0) stays as checked, also when equal to it.
    for (cam, _) in &cams {
        let name = format!("camera.{cam}.labels");
        if form.iter().any(|(k, _)| *k == name) {
            let picked: Vec<String> = form.iter().filter(|(k, v)| *k == name && !v.is_empty()).map(|(_, v)| v.clone()).collect();
            let own = if last(&format!("camera.{cam}.follow")).as_deref() == Some("0") {
                picked
            } else {
                crate::view::own_labels(&picked, &cfg.labels, &labels)
            };
            cfg.set_own_labels(cam, own);
        }
        // Its action buttons, by the rule the Windows tab uses (a URL that is not valid keeps
        // the one saved; `bad_urls` tells the user). A row that was not sent stays as it is.
        for n in 1..=config::ACTIONS {
            let Some(url) = last(&action_field(cam, n, "url")) else { continue };
            let saved = cfg.camera(cam).and_then(|o| o.action(n)).cloned().unwrap_or_default();
            let label = last(&action_field(cam, n, "label")).unwrap_or_else(|| saved.label.unwrap_or_default());
            let confirm = last(&action_field(cam, n, "confirm")).map_or(saved.confirm, |v| v == "true");
            cfg.set_action_typed(cam, n, &url, &label, confirm);
        }
    }
    config::write_all(&cfg, doc);
    last("autostart").map(|v| v == "true")
}

/// The form field of a camera's action `n`: `camera.<cam>.action<n>_<what>`.
fn action_field(cam: &str, n: usize, what: &str) -> String {
    format!("camera.{cam}.action{n}_{what}")
}

/// An action URL typed in the form that is not valid, so not saved. No `Debug`: the URL is a
/// secret.
struct BadUrl {
    cam: String,
    /// The camera's name on the page.
    title: String,
    /// The action's number, 1 to `ACTIONS`.
    n: usize,
    /// As typed: shown again in its field, nowhere else.
    url: String,
}

/// The form's action URLs that are not valid (`action::valid_url`), camera by camera as the
/// page lists them. One already so in the saved settings (`before`) is the user's own business,
/// as on Windows: it does not stop saving the rest.
fn bad_urls(form: &[(String, String)], before: &Config, known: &[CameraInfo]) -> Vec<BadUrl> {
    let (cams, _, _) = crate::view::choices(known, before);
    let mut bad = Vec::new();
    for (cam, title) in &cams {
        for n in 1..=config::ACTIONS {
            let name = action_field(cam, n, "url");
            let Some((_, url)) = form.iter().rev().find(|(k, _)| *k == name) else { continue };
            if !crate::action::valid_url(url) && before.action_url(cam, n).map(str::trim) != Some(url.trim()) {
                bad.push(BadUrl { cam: cam.clone(), title: title.clone(), n, url: url.clone() });
            }
        }
    }
    bad
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

/// A list of checkboxes (the empty hidden value marks the list as sent). The name may hold
/// a camera name, so it is escaped too.
fn checks(name: &str, items: &[(String, String, bool)], empty: &str) -> String {
    let (id, name) = (esc(&id(name)), esc(name));
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
    format!("<input type=\"hidden\" name=\"{name}\" value=\"\"><div class=\"checks\" id=\"{id}\">{boxes}</div>")
}

fn opts(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect()
}

/// The Actions section: a camera select and, for every camera, a group of `ACTIONS` rows
/// (URL, label, confirmation); the page's script shows the selected camera's group. A URL
/// that is not valid gets the reason under its row: one in `bad` (typed, not saved) is shown
/// as typed, one from the settings file as it is there.
fn actions(c: &Config, cams: &[(String, String)], bad: &[BadUrl]) -> String {
    let s = t();
    if cams.is_empty() {
        return row("action-for", s.action_camera, format!("<p class=\"tip\">{}</p>", esc(s.menu_no_cameras)), s.tip_action_camera);
    }
    let typed = |cam: &str, n: usize| bad.iter().find(|b| b.cam.eq_ignore_ascii_case(cam) && b.n == n).map(|b| b.url.as_str());
    // Shown first: a camera with a URL typed wrong, else one with buttons, else the first.
    let shown = bad
        .first()
        .and_then(|b| cams.iter().position(|(n, _)| n.eq_ignore_ascii_case(&b.cam)))
        .or_else(|| cams.iter().position(|(n, _)| !c.actions_for(n).is_empty()))
        .unwrap_or(0);
    let (mut options, mut groups) = (String::new(), String::new());
    for (i, (cam, title)) in cams.iter().enumerate() {
        options += &format!("<option value=\"{}\"{}>{}</option>", esc(cam), if i == shown { " selected" } else { "" }, esc(title));
        // Column headings, then a row per action. Ids by position: a camera's name can hold
        // anything, its form names are escaped.
        let mut rows = format!(
            "<span></span><span class=\"head\" title=\"{}\">{}</span><span class=\"head\" title=\"{}\">{}</span><span></span>",
            esc(s.tip_action_url),
            esc(s.action_url),
            esc(s.tip_action_label),
            esc(s.action_label)
        );
        for n in 1..=config::ACTIONS {
            let a = c.camera(cam).and_then(|o| o.action(n)).cloned().unwrap_or_default();
            let url = typed(cam, n).or(a.url.as_deref()).unwrap_or_default();
            let caption = i18n::fmt(s.action_row, &[&n]);
            let id = format!("action-{i}-{n}");
            let error = !crate::action::valid_url(url);
            let described = if error { format!(" aria-invalid=\"true\" aria-describedby=\"{id}-error\"") } else { String::new() };
            rows += &format!(
                "<label for=\"{id}-url\" title=\"{tip_url}\">{caption}</label>\
                 <input type=\"text\" inputmode=\"url\" spellcheck=\"false\" autocomplete=\"off\" id=\"{id}-url\" name=\"{url_name}\" value=\"{url}\" \
                 title=\"{tip_url}\" placeholder=\"{url_head}\" aria-label=\"{caption} {url_head}\"{described}>\
                 <input type=\"text\" autocomplete=\"off\" id=\"{id}-label\" name=\"{label_name}\" value=\"{label}\" title=\"{tip_label}\" \
                 placeholder=\"{label_head}\" aria-label=\"{caption} {label_head}\">\
                 <span><input type=\"hidden\" name=\"{confirm_name}\" value=\"false\"><label class=\"check\" title=\"{tip_confirm}\">\
                 <input type=\"checkbox\" id=\"{id}-confirm\" name=\"{confirm_name}\" value=\"true\" aria-label=\"{caption} {confirm}\"{checked}> {confirm}</label></span>",
                caption = esc(&caption),
                tip_url = esc(s.tip_action_url),
                url_name = esc(&action_field(cam, n, "url")),
                url = esc(url),
                url_head = esc(s.action_url),
                label_name = esc(&action_field(cam, n, "label")),
                label = esc(a.label.as_deref().unwrap_or_default()),
                tip_label = esc(s.tip_action_label),
                label_head = esc(s.action_label),
                confirm_name = esc(&action_field(cam, n, "confirm")),
                tip_confirm = esc(s.tip_action_confirm),
                confirm = esc(s.action_confirm_short),
                checked = if a.confirm { " checked" } else { "" },
            );
            if error {
                let why = i18n::fmt(s.action_row_error, &[&caption, &s.action_bad_url]);
                rows += &format!("<p class=\"error\" id=\"{id}-error\">{}</p>", esc(&why));
            }
        }
        groups += &format!("<div class=\"actions\" data-action-for=\"{}\">{rows}</div>", esc(cam));
    }
    let select = format!("<select id=\"action-for\" title=\"{}\">{options}</select>", esc(s.tip_action_camera));
    format!("{}{groups}<p class=\"note\">{}</p>\n", row("action-for", s.action_camera, select, s.tip_action_camera), esc(s.action_note))
}

/// The settings page; `saved` = the result of the last save, shown above the form; `bad` = the
/// action URLs typed that stopped it (see `actions`).
fn page(c: &Config, info: &Info, saved: Option<Result<(), String>>, bad: &[BadUrl]) -> String {
    let s = t();
    let lang = i18n::current();
    let (cams, labels, zones) = crate::view::choices(&info.cameras, c);
    let cam_items: Vec<(String, String, bool)> = cams.iter().map(|(n, title)| (n.clone(), title.clone(), listed(&c.cameras, n))).collect();
    // Objects: one group of checkboxes for all cameras and one for every camera; the
    // select above them shows one group (a camera with its own list is marked with *).
    let label_items =
        |list: &[String]| -> Vec<(String, String, bool)> { labels.iter().map(|l| (l.clone(), label_text(l), listed(list, l))).collect() };
    let mut objects = checks("trigger.labels", &label_items(&c.labels), "—");
    if !cams.is_empty() {
        let mut options = format!("<option value=\"\">{}</option>", esc(s.objects_all_cameras));
        let mut groups = format!("<div data-for=\"\">{objects}</div>");
        for (n, title) in &cams {
            let own = c.own_labels(n).is_some();
            let mark = if own { " *" } else { "" };
            options += &format!("<option value=\"{}\" data-title=\"{}\">{}{mark}</option>", esc(n), esc(title), esc(title));
            // follow = 1: no list of its own, the checks follow those for all cameras.
            groups += &format!(
                "<div data-for=\"{}\"><input type=\"hidden\" class=\"follow\" name=\"{}\" value=\"{}\">{}<button type=\"button\" class=\"same\" title=\"{}\">{}</button></div>",
                esc(n),
                esc(&format!("camera.{n}.follow")),
                if own { 0 } else { 1 },
                checks(&format!("camera.{n}.labels"), &label_items(c.labels_for(n)), "—"),
                esc(s.tip_objects_same_as_all),
                esc(s.objects_same_as_all)
            );
        }
        objects = format!("<select id=\"labels-for\" title=\"{}\">{options}</select>{groups}", esc(s.tip_objects_for));
    }
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
        "frigate.user",
        s.frigate_login,
        format!("{} {}", text("frigate.user", &c.user, "text"), text("frigate.password", &c.password, "password")),
        s.tip_frigate_login,
    );
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
    detection += &row("trigger.labels", s.objects, objects, s.tip_objects);
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
    video += &row("video.camera_volume", s.camera_volume, number("video.camera_volume", c.camera_volume, 0, 100, "%"), s.tip_camera_volume);
    video += &row("video.wind_filter", "", checkbox("video.wind_filter", c.wind_filter, s.wind_filter), s.tip_wind_filter);

    let actions = actions(c, &cams, bad);

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
header {{ display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: 12px; }}
h1 {{ font-size: 22px; margin: 0 0 16px; }}
h2 {{ font-size: 17px; margin: 0 0 12px; }}
a {{ color: var(--accent); }}
.links {{ display: flex; gap: 16px; white-space: nowrap; }}
section {{ background: var(--card); border: 1px solid var(--line); border-radius: 10px; padding: 18px 20px; margin-bottom: 16px; }}
.row {{ display: grid; grid-template-columns: 220px 1fr; gap: 6px 16px; padding: 8px 0; border-top: 1px solid var(--line); }}
.row:first-of-type {{ border-top: 0; }}
.row > label {{ font-weight: 600; padding-top: 6px; }}
.tip {{ margin: 4px 0 0; color: var(--muted); font-size: 13px; }}
.note {{ color: var(--muted); font-size: 13px; margin: 8px 0; }}
input[type=text], input[type=url], input[type=password], select {{ width: 100%; max-width: 420px; }}
input, select, button {{ font: inherit; color: inherit; background: var(--bg); border: 1px solid var(--line); border-radius: 6px; padding: 6px 8px; }}
input.num {{ width: 96px; }}
#mqtt-host {{ max-width: 240px; }} #mqtt-user, #mqtt-password, #frigate-user, #frigate-password {{ max-width: 200px; }}
label.check {{ display: inline-flex; align-items: center; gap: 6px; margin: 2px 16px 2px 0; font-weight: normal; }}
.checks {{ display: flex; flex-wrap: wrap; }}
#labels-for {{ max-width: 260px; margin-bottom: 6px; }}
button.same {{ margin-top: 6px; }}
#action-for {{ max-width: 260px; }}
.actions {{ display: grid; grid-template-columns: auto minmax(0, 1fr) minmax(0, 170px) auto; gap: 6px 10px; align-items: center; padding: 8px 0; }}
.actions > label {{ font-weight: 600; }}
.actions .head {{ font-weight: 600; color: var(--muted); font-size: 13px; }}
.actions input[type=text] {{ max-width: none; }}
.actions .error {{ grid-column: 2 / -1; margin: -2px 0 4px; font-size: 13px; }}
button {{ cursor: pointer; }}
button.primary {{ background: var(--accent); border-color: var(--accent); color: #fff; padding: 8px 20px; font-weight: 600; }}
pre {{ white-space: pre-wrap; background: var(--bg); border: 1px solid var(--line); border-radius: 6px; padding: 8px 10px; margin: 8px 0 0; font-size: 13px; }}
.bar {{ position: fixed; left: 0; right: 0; bottom: 0; background: var(--card); border-top: 1px solid var(--line); padding: 12px 16px; text-align: center; }}
.saved {{ color: var(--ok); font-weight: 600; margin: 0 0 12px; }}
.error {{ color: var(--bad); font-weight: 600; margin: 0 0 12px; }}
@media (max-width: 640px) {{ .row, .actions {{ grid-template-columns: 1fr; }} .row > label {{ padding-top: 0; }} .actions .head {{ display: none; }} .actions .error {{ grid-column: auto; }} }}
</style>
</head>
<body>
<main>
<header><h1>{title}</h1><span class="links"><a href="{help}" target="_blank" rel="noreferrer">{help_text}</a><a href="{sponsor}" target="_blank" rel="noreferrer">{support_text}</a></span></header>
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
<section><h2>{tab_actions}</h2>
{actions}</section>
<div class="bar"><button type="submit" class="primary">{apply}</button></div>
</form>
</main>
<script>
const form = document.getElementById("form");
const source = document.getElementById("frigate-source");
const mqtt = document.getElementById("mqtt");
const show = () => {{ mqtt.style.display = source.value === "mqtt" ? "" : "none"; }};
source.addEventListener("change", show); show();
// rules:begin
// The object lists' rules on plain data (the tests run them too). A camera follows the list
// for all cameras until the user checks something else for it: then it has a list of its
// own (*), kept as it is until checks equal to those for all cameras or "Same as all
// cameras" give it up. Nothing checked for all cameras = every object.
const rules = {{
  everyone: (checked, all) => (checked.length ? checked : all),
  own: (checked, every) => checked.length > 0 && (checked.length !== every.length || checked.some((x) => !every.includes(x))),
  // A camera {{ checked, all, follow }} after a change by "camera" (the user's checks for
  // it), "same" (its "Same as all cameras" button) or "all" (the list for all cameras).
  after: (cam, every, by) => {{
    if (by === "camera") return Object.assign({{}}, cam, {{ follow: !rules.own(cam.checked, every) }});
    if (by === "same" || cam.follow) return Object.assign({{}}, cam, {{ checked: every.filter((x) => cam.all.includes(x)), follow: true }});
    return cam;
  }},
}};
// rules:end
const scope = document.getElementById("labels-for");
if (scope) {{
  // One group of objects at a time; a camera's hidden "follow" field holds its state.
  const groups = [...document.querySelectorAll("[data-for]")];
  const group = (v) => groups.find((g) => g.dataset.for === v);
  const boxes = (g) => [...g.querySelectorAll("input[type=checkbox]")];
  const flag = (g) => g.querySelector("input.follow");
  const checked = (g) => boxes(g).filter((b) => b.checked).map((b) => b.value);
  const every = () => rules.everyone(checked(group("")), boxes(group("")).map((b) => b.value));
  const change = (g, by) => {{
    const c = rules.after({{ checked: checked(g), all: boxes(g).map((b) => b.value), follow: flag(g).value === "1" }}, every(), by);
    boxes(g).forEach((b) => {{ b.checked = c.checked.includes(b.value); }});
    flag(g).value = c.follow ? "1" : "0";
  }};
  const mark = () => {{ for (const o of scope.options) if (o.value) o.textContent = o.dataset.title + (flag(group(o.value)).value === "0" ? " *" : ""); }};
  const pick = () => groups.forEach((g) => {{ g.style.display = g.dataset.for === scope.value ? "" : "none"; }});
  for (const g of groups) {{
    if (!g.dataset.for) {{
      g.addEventListener("change", () => groups.forEach((x) => {{ if (x.dataset.for) change(x, "all"); }}));
      continue;
    }}
    g.addEventListener("change", () => change(g, "camera"));
    g.querySelector("button.same").addEventListener("click", () => {{ change(g, "same"); mark(); }});
  }}
  scope.addEventListener("change", pick);
  form.addEventListener("change", mark);
  pick();
}}
// actions:begin
// One camera's action buttons at a time (the page selects the one to show first).
const actionFor = document.getElementById("action-for");
if (actionFor) {{
  const groups = [...document.querySelectorAll("[data-action-for]")];
  const pick = () => groups.forEach((g) => {{ g.style.display = g.dataset.actionFor === actionFor.value ? "" : "none"; }});
  actionFor.addEventListener("change", pick);
  pick();
}}
// actions:end
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
        sponsor = crate::util::SPONSOR_URL,
        support_text = esc(s.menu_support),
        tab_connection = esc(s.tab_connection),
        tab_detection = esc(s.tab_detection),
        tab_window = esc(s.tab_window),
        tab_video = esc(s.tab_video),
        tab_actions = esc(s.tab_actions),
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
    fn saving_camera_object_lists() {
        let mut doc = IniDoc::parse(&format!("{}\n[camera.yard]\nlabels = cat\n", config::DEFAULT_INI));
        let known = [cam("front", &["person", "car"]), cam("garden", &["person", "dog"]), cam("yard", &["cat"])];
        // For all cameras: person. Front: the same (so no list of its own); garden: its own.
        let form = parse_form(
            "trigger.labels=&trigger.labels=person\
             &camera.front.labels=&camera.front.labels=person\
             &camera.garden.labels=&camera.garden.labels=person&camera.garden.labels=dog",
        );
        apply(&form, &mut doc, &known);
        let c = Config::from_ini(&doc);
        assert_eq!(c.labels, vec!["person".to_string()]);
        assert_eq!(c.own_labels("front"), None);
        assert_eq!(c.own_labels("garden"), Some(&["person".to_string(), "dog".to_string()][..]));
        // A camera whose list was not sent keeps it.
        assert_eq!(c.own_labels("yard"), Some(&["cat".to_string()][..]));
        assert_eq!(doc.get("camera.front", "labels"), None);
        // Back to the list for all cameras: the key stays empty.
        let form = parse_form("trigger.labels=&trigger.labels=person&camera.garden.labels=&camera.garden.labels=person&camera.garden.follow=1");
        apply(&form, &mut doc, &known);
        assert_eq!(doc.get("camera.garden", "labels").as_deref(), Some(""));
        assert_eq!(Config::from_ini(&doc).labels_for("garden"), ["person"]);
    }

    #[test]
    fn saving_keeps_lists_that_were_not_edited() {
        // Yard's list equals the one for all cameras but is its own (follow=0, as the page
        // sends it for a list nobody changed): saving other settings keeps it.
        let mut doc = IniDoc::parse(&format!("{}\n[camera.yard]\nlabels = person\n", config::DEFAULT_INI));
        let known = [cam("front", &["person", "car"]), cam("yard", &["person", "cat"])];
        let form = parse_form(
            "trigger.labels=&trigger.labels=person\
             &camera.yard.labels=&camera.yard.labels=person&camera.yard.follow=0\
             &camera.front.labels=&camera.front.labels=person&camera.front.labels=car&camera.front.follow=1",
        );
        apply(&form, &mut doc, &known);
        let c = Config::from_ini(&doc);
        assert_eq!(c.own_labels("yard"), Some(&["person".to_string()][..]));
        // A following camera whose checks were changed gets its own list.
        assert_eq!(c.own_labels("front"), Some(&["person".to_string(), "car".to_string()][..]));
    }

    #[test]
    fn page_shows_camera_object_lists() {
        i18n::set(Lang::En);
        let c = Config::from_ini(&IniDoc::parse("[trigger]\nlabels = person\n[camera.garden]\nlabels = dog\n"));
        let info = Info { cameras: vec![cam("front", &["person", "car"]), cam("garden", &["person", "dog"])], ..Default::default() };
        let html = page(&c, &info, None, &[]);
        assert!(html.contains("<option value=\"\">All cameras</option>"), "{html}");
        assert!(html.contains("<option value=\"front\" data-title=\"front\">front</option>"));
        assert!(html.contains("<option value=\"garden\" data-title=\"garden\">garden *</option>"));
        // Each camera's group: its own list, or the one for all cameras.
        assert!(html.contains("name=\"camera.garden.labels\" value=\"dog\" checked"));
        assert!(html.contains("name=\"camera.garden.labels\" value=\"person\">"));
        assert!(html.contains("name=\"camera.front.labels\" value=\"person\" checked"));
        assert!(html.contains("name=\"camera.front.labels\" value=\"car\">"));
        assert!(html.contains("Same as all cameras"));
        // Cameras without a list of their own follow changes made for all cameras (in the page).
        assert!(html.contains("<input type=\"hidden\" class=\"follow\" name=\"camera.front.follow\" value=\"1\">"));
        assert!(html.contains("<input type=\"hidden\" class=\"follow\" name=\"camera.garden.follow\" value=\"0\">"));
    }

    /// The page's object-list rules (plain functions in its script), run by node; skipped
    /// with a note when node is not installed.
    #[test]
    fn page_object_rules() {
        i18n::set(Lang::En);
        let c = Config::from_ini(&IniDoc::parse(""));
        let html = page(&c, &Info { cameras: vec![cam("front", &["person"])], ..Default::default() }, None, &[]);
        let rules = html.split("// rules:begin").nth(1).and_then(|r| r.split("// rules:end").next()).expect("rules in the page");
        let checks = r#"
const assert = require("assert");
const eq = (a, b) => assert.deepStrictEqual(a, b);
// Nothing checked for all cameras = every object.
eq(rules.everyone([], ["p", "c"]), ["p", "c"]);
eq(rules.everyone(["p"], ["p", "c"]), ["p"]);
// A list of its own: something checked, not the same as for all cameras.
eq([rules.own([], ["p"]), rules.own(["c", "p"], ["p", "c"]), rules.own(["p"], ["p", "c"]), rules.own(["p", "c", "d"], ["p", "c"])], [false, false, true, true]);
const cam = (checked, follow) => ({ checked, all: ["p", "c", "d"], follow });
// A change for all cameras: a following camera takes it (only its own entries), one with a list keeps it.
eq(rules.after(cam(["p"], true), ["p", "c", "x"], "all"), cam(["p", "c"], true));
eq(rules.after(cam(["p", "c"], false), ["p"], "all"), cam(["p", "c"], false));
// The user's checks for a camera: the same as for all cameras (or none) = following.
eq(rules.after(cam(["c", "p"], false), ["p", "c"], "camera").follow, true);
eq(rules.after(cam([], false), ["p", "c"], "camera").follow, true);
eq(rules.after(cam(["p", "d"], true), ["p", "c"], "camera").follow, false);
// "Same as all cameras".
eq(rules.after(cam(["d"], false), ["p", "c"], "same"), cam(["p", "c"], true));
"#;
        match std::process::Command::new("node").arg("-e").arg(format!("{rules}\n{checks}")).output() {
            Ok(out) => assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr)),
            Err(e) => eprintln!("node is not available ({e}): the page's object rules were not run"),
        }
    }

    #[test]
    fn camera_names_are_escaped_in_object_lists() {
        i18n::set(Lang::En);
        let c = Config::from_ini(&IniDoc::parse("[trigger]\ncameras = front, x\"<y\n"));
        let html = page(&c, &Info { cameras: vec![cam("front", &["person"])], ..Default::default() }, None, &[]);
        assert!(html.contains("name=\"camera.x&quot;&lt;y.labels\""), "{html}");
        assert!(!html.contains("x\"<y"));
    }

    #[test]
    fn saving_camera_actions() {
        let mut doc = IniDoc::parse(config::DEFAULT_INI);
        let known = [cam("front", &["person"]), cam("gate", &["person"])];
        // Rows 1 and 3 with a URL, 2 with a label only, 4 empty; the checkbox sends "false"
        // and, when checked, "true" after it.
        let form = parse_form(
            "camera.gate.action1_url=+http%3A%2F%2Fh%2Fapi%2Fwebhook%2Fa+&camera.gate.action1_label=+Gate+\
             &camera.gate.action1_confirm=false&camera.gate.action1_confirm=true\
             &camera.gate.action2_url=&camera.gate.action2_label=Light&camera.gate.action2_confirm=false&camera.gate.action2_confirm=true\
             &camera.gate.action3_url=https%3A%2F%2Fh%2Flight%3Fa%3D1%26b%3D2&camera.gate.action3_label=&camera.gate.action3_confirm=false\
             &camera.gate.action4_url=&camera.gate.action4_label=&camera.gate.action4_confirm=false",
        );
        apply(&form, &mut doc, &known);
        let c = Config::from_ini(&doc);
        let shown = c.actions_for("gate");
        assert_eq!(shown.iter().map(|a| a.n).collect::<Vec<_>>(), [1, 3], "no URL = no button");
        assert_eq!((shown[0].label, shown[0].confirm), (Some("Gate"), true));
        assert_eq!((shown[1].label, shown[1].confirm), (None, false));
        assert_eq!(c.action_url("gate", 1), Some("http://h/api/webhook/a"));
        assert_eq!(c.action_url("gate", 3), Some("https://h/light?a=1&b=2"));
        assert_eq!(c.action_url("gate", 2), None);
        assert!(c.actions_for("front").is_empty());
        assert_eq!(doc.get("camera.front", "action1_url"), None, "no section for a camera without buttons");
        // Rows that were not sent stay as they are.
        apply(&parse_form("camera.gate.action1_url=http%3A%2F%2Fh%2Fb"), &mut doc, &known);
        let c = Config::from_ini(&doc);
        assert_eq!(c.actions_for("gate").iter().map(|a| (a.n, a.label, a.confirm)).collect::<Vec<_>>(), [(1, Some("Gate"), true), (3, None, false)]);
        assert_eq!(c.action_url("gate", 1), Some("http://h/b"));
    }

    #[test]
    fn an_action_url_that_is_not_valid_is_never_saved() {
        let ini = format!("{}\n[camera.gate]\naction1_url = http://h/old\naction3_url = nonsense\n", config::DEFAULT_INI);
        let before = Config::from_ini(&IniDoc::parse(&ini));
        let form = parse_form(
            "camera.gate.action1_url=not+a+url&camera.gate.action1_label=Gate&camera.gate.action2_url=ftp%3A%2F%2Fh%2Fx\
             &camera.gate.action3_url=nonsense&camera.gate.action4_url=",
        );
        // Typed ones stop the save; one already in the file (row 3) does not, as on Windows.
        let bad = bad_urls(&form, &before, &[]);
        assert_eq!(
            bad.iter().map(|b| (b.cam.as_str(), b.n, b.url.as_str())).collect::<Vec<_>>(),
            [("gate", 1, "not a url"), ("gate", 2, "ftp://h/x")]
        );
        // Applied anyway (the test button does), the saved URLs stay.
        let mut doc = IniDoc::parse(&ini);
        apply(&form, &mut doc, &[]);
        let c = Config::from_ini(&doc);
        assert_eq!(c.action_url("gate", 1), Some("http://h/old"));
        assert_eq!(c.action_url("gate", 2), None);
        assert_eq!(c.action_url("gate", 3), Some("nonsense"));
        assert_eq!(c.camera("gate").unwrap().action(1).unwrap().label.as_deref(), Some("Gate"));
    }

    #[test]
    fn the_camera_filter_counts_cameras_with_buttons_or_objects() {
        // A camera Frigate does not know, with only an action button or only an object list,
        // is in the filter; all checked = all cameras (empty).
        for extra in ["[camera.gate]\naction1_url = http://h/a\n", "[camera.gate]\nlabels = cat\n"] {
            let known = [cam("front", &["person"])];
            let mut doc = IniDoc::parse(&format!("{}\n{extra}", config::DEFAULT_INI));
            apply(&parse_form("trigger.cameras=&trigger.cameras=front&trigger.cameras=gate"), &mut doc, &known);
            assert!(Config::from_ini(&doc).cameras.is_empty(), "{extra}");
            apply(&parse_form("trigger.cameras=&trigger.cameras=front"), &mut doc, &known);
            assert_eq!(Config::from_ini(&doc).cameras, ["front"], "{extra}");
        }
    }

    #[test]
    fn page_shows_camera_actions() {
        i18n::set(Lang::En);
        let c = Config::from_ini(&IniDoc::parse(
            "[camera.gate]\naction2_url = http://h/api/webhook/x\naction2_label = Gate\naction2_confirm = false\naction3_url = nonsense\n",
        ));
        let html = page(&c, &Info { cameras: vec![cam("front", &["person"]), cam("gate", &["person"])], ..Default::default() }, None, &[]);
        let video = html.find("<h2>Video &amp; audio</h2>").expect("video section");
        assert!(html.find("<section><h2>Actions</h2>").is_some_and(|a| a > video), "the Actions section after the Video one");
        // A camera with buttons is shown first.
        assert!(html.contains("<option value=\"front\">front</option><option value=\"gate\" selected>gate</option>"), "{html}");
        assert!(html.contains("data-action-for=\"front\"") && html.contains("data-action-for=\"gate\""));
        assert!(html.contains(">Action 4</label>") && html.contains("aria-label=\"Action 2 URL\""));
        assert!(html.contains("name=\"camera.gate.action2_url\" value=\"http://h/api/webhook/x\""));
        assert!(html.contains("name=\"camera.gate.action2_label\" value=\"Gate\""));
        // Confirmation: on unless switched off; unchecked still sends "false".
        assert!(html.contains("<input type=\"hidden\" name=\"camera.gate.action2_confirm\" value=\"false\">"));
        assert!(html.contains("name=\"camera.gate.action2_confirm\" value=\"true\" aria-label=\"Action 2 Confirmation\"> Confirmation"));
        assert!(html.contains("name=\"camera.gate.action1_confirm\" value=\"true\" aria-label=\"Action 1 Confirmation\" checked>"));
        // A URL in the file that is not valid: the reason under its row (and only there).
        assert!(html.contains("<p class=\"error\" id=\"action-1-3-error\">Action 3: Not a valid http:// or https:// address.</p>"));
        assert_eq!(html.matches("class=\"error\"").count(), 1);
    }

    #[test]
    fn page_shows_action_urls_typed_wrong() {
        i18n::set(Lang::En);
        let c = Config::from_ini(&IniDoc::parse("[camera.gate]\naction1_url = http://h/old\n[camera.yard]\naction1_url = http://h/y\n"));
        let bad = [BadUrl { cam: "yard".into(), title: "yard".into(), n: 2, url: "x\"<y".into() }];
        let html = page(&c, &Info::default(), Some(Err("Action 2: …".into())), &bad);
        // That camera is shown, the field keeps what was typed, the reason is under it.
        assert!(html.contains("<option value=\"yard\" selected>"), "{html}");
        assert!(html.contains("name=\"camera.yard.action2_url\" value=\"x&quot;&lt;y\" title="));
        assert!(html.contains("aria-invalid=\"true\" aria-describedby=\"action-1-2-error\""));
        assert!(html.contains("id=\"action-1-2-error\">Action 2: Not a valid"));
        assert!(!html.contains("x\"<y"));
    }

    #[test]
    fn page_lists_cameras_only_known_by_their_buttons() {
        i18n::set(Lang::En);
        let c = Config::from_ini(&IniDoc::parse("[camera.gate]\naction1_url = http://h/a\n"));
        let html = page(&c, &Info { cameras: vec![cam("front", &["person"])], ..Default::default() }, None, &[]);
        assert!(html.contains("data-action-for=\"gate\"") && html.contains("<option value=\"gate\" selected>gate</option>"), "{html}");
        assert!(html.contains("name=\"trigger.cameras\" value=\"gate\" checked"));
        // No cameras at all: no rows, a word why.
        let html = page(&Config::from_ini(&IniDoc::parse("")), &Info::default(), None, &[]);
        assert!(!html.contains("<div class=\"actions\"") && html.contains("(no cameras)"));
    }

    #[test]
    fn camera_names_are_escaped_in_action_groups() {
        i18n::set(Lang::En);
        let c = Config::from_ini(&IniDoc::parse("[camera.x\"<y]\naction1_url = http://h/a\n"));
        let html = page(&c, &Info::default(), None, &[]);
        assert!(html.contains("data-action-for=\"x&quot;&lt;y\""), "{html}");
        assert!(html.contains("name=\"camera.x&quot;&lt;y.action1_url\""));
        assert!(html.contains("<option value=\"x&quot;&lt;y\" selected>x&quot;&lt;y</option>"));
        assert!(!html.contains("x\"<y"));
    }

    /// The script that shows one camera's action rows, run by node on stand-ins for the page.
    #[test]
    fn page_action_camera_select() {
        i18n::set(Lang::En);
        let html = page(&Config::from_ini(&IniDoc::parse("")), &Info { cameras: vec![cam("front", &[])], ..Default::default() }, None, &[]);
        let script = html.split("// actions:begin").nth(1).and_then(|r| r.split("// actions:end").next()).expect("script in the page");
        let setup = r#"
const assert = require("assert");
const groups = [{ dataset: { actionFor: "front" }, style: {} }, { dataset: { actionFor: "gate" }, style: {} }];
// The object lists' groups are not touched.
const other = { dataset: { for: "front" }, style: {} };
let changed;
const select = { value: "gate", addEventListener: (e, f) => { assert.strictEqual(e, "change"); changed = f; } };
const document = {
  getElementById: (id) => (id === "action-for" ? select : null),
  querySelectorAll: (q) => (q === "[data-action-for]" ? groups : q === "[data-for]" ? [other] : []),
};
"#;
        let checks = r#"
const shown = () => groups.map((g) => g.style.display);
assert.deepStrictEqual(shown(), ["none", ""]);
select.value = "front";
changed();
assert.deepStrictEqual(shown(), ["", "none"]);
assert.strictEqual(other.style.display, undefined);
"#;
        match std::process::Command::new("node").arg("-e").arg(format!("{setup}\n{script}\n{checks}")).output() {
            Ok(out) => assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr)),
            Err(e) => eprintln!("node is not available ({e}): the page's action script was not run"),
        }
    }

    #[test]
    fn saving_stops_at_an_action_url_typed_wrong() {
        i18n::set(Lang::En);
        let dir = std::env::temp_dir().join(format!("fpip-web-actions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.ini");
        std::fs::write(&path, format!("{}\n[camera.gate]\naction1_url = http://h/old\naction3_url = nonsense\n", config::DEFAULT_INI)).unwrap();
        let web = start(path.clone(), Arc::new(Mutex::new(Info::default())), Arc::new(|_| {})).unwrap();
        let (port, key) = web.url.strip_prefix("http://127.0.0.1:").unwrap().split_once('/').unwrap();
        let (port, host) = (port.parse().unwrap(), format!("127.0.0.1:{port}"));
        let save = |form: &str| {
            let head = format!("POST /{key}save HTTP/1.1\r\nHost: {host}\r\nOrigin: http://{host}\r\nContent-Length: {}", form.len());
            send(port, &format!("{head}\r\n\r\n{form}"))
        };
        let before = std::fs::read_to_string(&path).unwrap();
        let answer = save("display.width=640&camera.gate.action1_url=not+a+url&camera.gate.action3_url=nonsense");
        assert!(answer.starts_with("HTTP/1.1 422 "), "{answer}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before, "nothing is saved");
        // The page again: what was typed, and why it was not saved.
        assert!(answer.contains("<p class=\"error\">Not saved: gate – Action 1: Not a valid http:// or https:// address.</p>"), "{answer}");
        assert!(answer.contains("name=\"camera.gate.action1_url\" value=\"not a url\""));
        assert!(answer.contains("id=\"display-width\" name=\"display.width\" value=\"640\""));
        // A URL that is not valid but already in the file does not stop the rest.
        let answer = save("display.width=640&camera.gate.action1_url=http%3A%2F%2Fh%2Fnew&camera.gate.action3_url=nonsense");
        assert!(answer.starts_with("HTTP/1.1 303 "), "{answer}");
        let c = config::load(&path).unwrap().0;
        assert_eq!((c.width, c.action_url("gate", 1), c.action_url("gate", 3)), (640, Some("http://h/new"), Some("nonsense")));
        let _ = std::fs::remove_dir_all(&dir);
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

    /// A raw request to the running page; the whole answer (the server then closes).
    fn send(port: u16, request: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(request.as_bytes()).unwrap();
        let mut answer = String::new();
        s.read_to_string(&mut answer).unwrap();
        answer
    }

    #[test]
    fn saving_works_in_chromium() {
        // Chromium sends `Origin: null` with the form of a page served with
        // `Referrer-Policy: no-referrer`, and that origin is refused. Other sites must
        // still get no referrer: the page address carries the key.
        let dir = std::env::temp_dir().join(format!("fpip-web-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.ini");
        let web = start(path.clone(), Arc::new(Mutex::new(Info::default())), Arc::new(|_| {})).unwrap();
        let (port, key) = web.url.strip_prefix("http://127.0.0.1:").unwrap().split_once('/').unwrap();
        let (port, host) = (port.parse().unwrap(), format!("127.0.0.1:{port}"));
        let page = send(port, &format!("GET /{key} HTTP/1.1\r\nHost: {host}\r\n\r\n"));
        assert!(page.starts_with("HTTP/1.1 200 "));
        assert!(page.contains("\r\nReferrer-Policy: same-origin\r\n"));
        let form = "frigate.url=http%3A%2F%2F10.0.0.5%3A5000";
        let save = |origin: &str| {
            let head = format!("POST /{key}save HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Length: {}", form.len());
            send(port, &format!("{head}\r\n\r\n{form}"))
        };
        // Other origins (also `null`) are still refused; the page's own one saves.
        assert!(save("null").starts_with("HTTP/1.1 403 "));
        assert!(save("http://example.com").starts_with("HTTP/1.1 403 "));
        assert!(save(&format!("http://{host}")).starts_with("HTTP/1.1 303 "));
        assert_eq!(config::load(&path).unwrap().0.url, "http://10.0.0.5:5000");
        let _ = std::fs::remove_dir_all(&dir);
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
        let html = page(&c, &info, Some(Ok(())), &[]);
        assert!(html.contains("&lt;script&gt;") && !html.contains("\"<script>"));
        assert!(html.contains("name=\"trigger.cameras\" value=\"front_door\" checked"));
        // Monitors are numbered from the left.
        assert!(html.contains("<option value=\"1\">Monitor 1 – 1280×1024</option>"));
        assert!(html.contains("Monitor 2 – 1920×1080 (primary)"));
        assert!(html.contains("id=\"autostart\" name=\"autostart\" value=\"true\" checked"));
        let failed = page(&c, &info, Some(Err("/x/config.ini: <denied>".into())), &[]);
        assert!(failed.contains("class=\"error\">Not saved: /x/config.ini: &lt;denied&gt;</p>"));
    }

    /// The header carries the way out to the user guide and to the sponsors page, in the
    /// language of the page. `FRIGATEPIP_PAGE_OUT=page.html` also writes it out, to look at.
    #[test]
    fn page_header_links_out() {
        // English, like its neighbours: the language is one setting for the whole program,
        // and these tests run side by side, so a test that changes it breaks another.
        i18n::set(Lang::En);
        let c = Config::from_ini(&IniDoc::parse(config::DEFAULT_INI));
        let html = page(&c, &Info { cameras: vec![cam("front", &["person"])], ..Default::default() }, None, &[]);
        assert!(html.contains("user-guide.md"), "no link to the user guide");
        let link = format!("<a href=\"{}\" target=\"_blank\" rel=\"noreferrer\">Support FrigatePiP</a>", crate::util::SPONSOR_URL);
        assert!(html.contains(&link), "no link to the sponsors page");
        // Both sit in the header, in one row of their own.
        let header = html.split("<header>").nth(1).and_then(|h| h.split("</header>").next()).unwrap_or_default();
        assert_eq!(header.matches("<a href=").count(), 2, "header links: {header}");
        assert!(header.contains("class=\"links\""));
        assert!(html.contains(".links { display: flex;"));
        if let Ok(path) = std::env::var("FRIGATEPIP_PAGE_OUT") {
            std::fs::write(&path, &html).expect("cannot write the page");
            println!("written {path}");
        }
    }
}
