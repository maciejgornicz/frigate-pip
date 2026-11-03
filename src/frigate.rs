//! Listens for Frigate events (WebSocket or MQTT) and reads the camera list from `/api/config`.

use crate::client::Frigate;
use crate::config::{Config, Source};
use crate::filter::{Detection, EventFilter, Verdict};
use crate::json::{self, Json};
use crate::mqtt::{Mqtt, Packet};
use crate::tls::Trust;
use crate::util::{random_u64, Cancel};
use crate::ws::Msg;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct CameraInfo {
    pub name: String,
    pub title: String,
    pub main: Option<String>,
    pub sub: Option<String>,
    /// Width / height of the detect stream.
    pub aspect: f32,
    /// Objects tracked by Frigate on this camera.
    pub labels: Vec<String>,
    pub zones: Vec<String>,
}

pub enum Notice {
    Connected(String),
    Disconnected(String),
    Cameras(Vec<CameraInfo>),
    Show(Detection, Instant),
    Activity(String),
}

fn is_sub(key: &str, stream: &str) -> bool {
    let (k, s) = (key.to_ascii_lowercase(), stream.to_ascii_lowercase());
    k.contains("sub") || s.contains("sub") || k.contains("low") || k.contains("niska")
}

pub fn camera_infos(j: &Json, cfg: &Config) -> Vec<CameraInfo> {
    let go2rtc: Vec<&str> = j.get("go2rtc").get("streams").obj().iter().map(|(k, _)| k.as_str()).collect();
    let mut out = Vec::new();
    for (name, cam) in j.get("cameras").obj() {
        if cam.get("enabled").bool() == Some(false) {
            continue;
        }
        let ov = cfg.overrides.get(name).cloned().unwrap_or_default();
        let title =
            ov.name.clone().or_else(|| cam.get("friendly_name").str().filter(|s| !s.is_empty()).map(str::to_string)).unwrap_or_else(|| name.clone());
        let detect = cam.get("detect");
        let (w, h) = (detect.get("width").f64().unwrap_or(0.0), detect.get("height").f64().unwrap_or(0.0));
        let aspect = if w > 0.0 && h > 0.0 { (w / h) as f32 } else { 16.0 / 9.0 };
        let mut main: Option<String> = None;
        let mut sub: Option<String> = None;
        let live = cam.get("live");
        for (k, v) in live.get("streams").obj() {
            let Some(s) = v.str() else { continue };
            let slot = if is_sub(k, s) { &mut sub } else { &mut main };
            if slot.is_none() {
                *slot = Some(s.to_string());
            }
        }
        if let Some(s) = live.get("stream_name").str() {
            main.get_or_insert_with(|| s.to_string());
        }
        if main.is_none() && go2rtc.contains(&name.as_str()) {
            main = Some(name.clone());
        }
        let sub_guess = format!("{name}_sub");
        if sub.is_none() && go2rtc.contains(&sub_guess.as_str()) {
            sub = Some(sub_guess);
        }
        if ov.main.is_some() {
            main = ov.main;
        }
        if ov.sub.is_some() {
            sub = ov.sub;
        }
        let labels = cam.get("objects").get("track").strs().map(str::to_string).collect();
        let zones = cam.get("zones").obj().iter().map(|(k, _)| k.clone()).collect();
        out.push(CameraInfo { name: name.clone(), title, main, sub, aspect, labels, zones });
    }
    out
}

/// The cameras from `/api/config` (the error is the reason, or "JSON" for an unreadable answer).
pub fn cameras(cfg: &Config, cancel: &Cancel) -> Result<Vec<CameraInfo>, String> {
    let b = Frigate::new(cfg).get_bytes("/api/config", Duration::from_secs(4), Some(cancel), 16 << 20).map_err(|e| e.to_string())?;
    let j = Json::parse(&String::from_utf8_lossy(&b)).ok_or("JSON")?;
    Ok(camera_infos(&j, cfg))
}

/// Connection test for the settings window. Every step is reported with its time.
pub fn connection_test(cfg: &Config, cancel: &Cancel, report: &mut dyn FnMut(String)) {
    use crate::i18n::{fmt, t as tr};
    let s = tr();
    let ms = |t: Instant| t.elapsed().as_millis();
    let timeout = Duration::from_secs(4);
    let api = Frigate::new(cfg);
    let t = Instant::now();
    match api.get_bytes("/api/version", timeout, Some(cancel), 4096) {
        Ok(v) => report(fmt(s.test_frigate_ok, &[&String::from_utf8_lossy(&v).trim(), &ms(t)])),
        Err(e) => {
            report(fmt(s.test_frigate_fail, &[&cfg.url, &e]));
            let hint = match e.kind() {
                std::io::ErrorKind::PermissionDenied => s.test_login_hint,
                _ if e.to_string().contains("HTTP 401") => s.test_login_hint,
                std::io::ErrorKind::InvalidData if e.to_string().contains("certificate") => s.test_cert_hint,
                _ => s.test_frigate_hint,
            };
            report(hint.into());
            return;
        }
    }
    if let Some(u) = crate::net::Url::parse(&cfg.url).filter(|u| u.scheme == "https") {
        match crate::tls::trust(&u.host, u.port) {
            Some(Trust::Public) => report(s.test_tls_public.into()),
            Some(Trust::FirstUse(fp)) => report(fmt(s.test_tls_first_use, &[&fp])),
            Some(Trust::Remembered(fp)) => report(fmt(s.test_tls_remembered, &[&fp])),
            None => {}
        }
    }
    if let Some(user) = api.user() {
        report(fmt(s.test_login_ok, &[&user]));
    }
    let t = Instant::now();
    let cams = cameras(cfg, cancel).unwrap_or_else(|e| {
        report(fmt(s.test_config_fail, &[&e]));
        Vec::new()
    });
    if !cams.is_empty() {
        let names: Vec<&str> = cams.iter().map(|c| c.title.as_str()).collect();
        report(fmt(s.test_cameras, &[&cams.len(), &names.join(", "), &ms(t)]));
    }
    let t = Instant::now();
    match cfg.source {
        Source::WebSocket => match api.websocket(timeout, timeout, cancel) {
            Ok(_) => report(fmt(s.test_websocket_ok, &[&ms(t)])),
            Err(e) => report(fmt(s.test_websocket_fail, &[&api.ws_url(), &e])),
        },
        Source::Mqtt => {
            let id = format!("frigate-pip-test-{:08x}", random_u64() as u32);
            let host = format!("{}:{}", cfg.mqtt_host, cfg.mqtt_port);
            let r = Mqtt::connect(&cfg.mqtt_host, cfg.mqtt_port, &id, &cfg.mqtt_user, &cfg.mqtt_password, 30, timeout, cancel)
                .and_then(|mut m| m.subscribe(&[format!("{}/events", cfg.mqtt_prefix.trim_end_matches('/'))]));
            match r {
                Ok(()) => report(fmt(s.test_mqtt_ok, &[&host, &ms(t)])),
                Err(e) => report(fmt(s.test_mqtt_fail, &[&host, &e])),
            }
        }
    }
    if let Some(c) = cams.first() {
        let stream = c.sub.clone().or_else(|| c.main.clone()).unwrap_or_else(|| c.name.clone());
        let url = cfg.rtsp_url(&stream);
        let t = Instant::now();
        match crate::rtsp::probe(&url, cancel) {
            Ok((v, a)) => {
                let audio = a.map(|a| fmt(s.test_audio, &[&a.rate])).unwrap_or_else(|| s.test_no_audio.into());
                report(fmt(s.test_rtsp_ok, &[&stream, &v.codec.name(), &audio, &ms(t)]));
            }
            Err(e) => report(fmt(s.test_rtsp_fail, &[&url, &e])),
        }
    }
    report(s.test_done.into());
}

/// Starts the listener thread; it ends after `cancel.cancel()`.
pub fn start(cfg: Config, cancel: Cancel, sink: impl Fn(Notice) + Send + 'static) {
    let _ = std::thread::Builder::new().name("frigate".into()).spawn(move || {
        let delays = [1000u64, 2000, 5000, 10000, 15000];
        let mut attempt = 0;
        let mut last_err = String::new();
        while !cancel.is_set() {
            let started = Instant::now();
            let err = session(&cfg, &cancel, &sink);
            if cancel.is_set() {
                break;
            }
            if started.elapsed() > Duration::from_secs(60) {
                attempt = 0;
                last_err.clear();
            }
            if err != last_err {
                crate::log!("Frigate: {err}");
                last_err = err.clone();
            }
            sink(Notice::Disconnected(err));
            cancel.sleep(delays[attempt.min(delays.len() - 1)]);
            attempt += 1;
        }
    });
}

struct Dispatch<'a, F: Fn(Notice)> {
    filter: EventFilter,
    sink: &'a F,
    last_activity: HashMap<String, Instant>,
}

impl<F: Fn(Notice)> Dispatch<'_, F> {
    fn handle(&mut self, topic: &str, payload: &Json, t0: Instant) {
        match self.filter.on_message(topic, payload) {
            Verdict::Show(d) => {
                if d.score > 0.0 {
                    crate::log!("Detected: {} on {} ({:.0}%)", d.label, d.camera, d.score * 100.0);
                } else {
                    crate::log!("Detected: {} on {} (review item)", d.label, d.camera);
                }
                self.last_activity.insert(d.camera.clone(), t0);
                (self.sink)(Notice::Show(d, t0));
            }
            Verdict::Activity(cam) => {
                let due = self.last_activity.get(&cam).is_none_or(|t| t.elapsed() > Duration::from_millis(1000));
                if due {
                    self.last_activity.insert(cam.clone(), t0);
                    (self.sink)(Notice::Activity(cam));
                }
            }
            Verdict::Ignore => {}
        }
    }
}

fn session(cfg: &Config, cancel: &Cancel, sink: &impl Fn(Notice)) -> String {
    let api = Frigate::new(cfg);
    // Frigate's clock from the answer's `Date`: review items carry times by that clock.
    let (body, server_time) = match api.get("/api/config", Duration::from_secs(5), Some(cancel)) {
        Ok(r) if r.status == 200 => {
            let date = crate::net::header(&r.headers, "Date").and_then(crate::net::http_date);
            (r.read_all(16 << 20), date)
        }
        Ok(r) => (Err(std::io::Error::other(format!("HTTP {} for {}", r.status, api.url("/api/config")))), None),
        Err(e) => (Err(e), None),
    };
    match body {
        Ok(b) => match Json::parse(&String::from_utf8_lossy(&b)) {
            Some(j) => sink(Notice::Cameras(camera_infos(&j, cfg))),
            None => crate::log!("Cannot parse /api/config"),
        },
        Err(e) => return format!("Frigate unreachable ({}): {e}", cfg.url),
    }
    let mut filter = EventFilter::new(cfg);
    if let Some(t) = server_time {
        filter.set_since(t);
    }
    let mut d = Dispatch { filter, sink, last_activity: HashMap::new() };
    match cfg.source {
        Source::WebSocket => ws_session(&api, cfg, cancel, &mut d),
        Source::Mqtt => mqtt_session(cfg, cancel, &mut d),
    }
}

fn ws_session<F: Fn(Notice)>(api: &Frigate, cfg: &Config, cancel: &Cancel, d: &mut Dispatch<F>) -> String {
    let url = api.ws_url();
    let mut ws = match api.websocket(Duration::from_secs(5), Duration::from_secs(30), cancel) {
        Ok(ws) => ws,
        Err(e) => return format!("WebSocket {url}: {e}"),
    };
    crate::log!("Connected to {url}");
    (d.sink)(Notice::Connected(cfg.url.clone()));
    let mut waiting_pong = false;
    loop {
        if cancel.is_set() {
            return "cancelled".into();
        }
        match ws.read() {
            Ok(Msg::Text(t)) => {
                waiting_pong = false;
                let t0 = Instant::now();
                let Some(topic) = json::peek_topic(&t) else { continue };
                if !d.filter.wants(topic) {
                    continue;
                }
                let Some(outer) = Json::parse(&t) else { continue };
                let payload = outer.get("payload");
                let inner = payload.str().and_then(Json::parse);
                d.handle(topic, inner.as_ref().unwrap_or(payload), t0);
            }
            Ok(Msg::Idle) => {
                if waiting_pong {
                    return "no response from Frigate".into();
                }
                if let Err(e) = ws.send_ping() {
                    return e.to_string();
                }
                waiting_pong = true;
            }
            Ok(Msg::Pong) | Ok(Msg::Binary) => waiting_pong = false,
            Ok(Msg::Close) => return "Frigate closed the connection".into(),
            Err(e) => return format!("WebSocket: {e}"),
        }
    }
}

fn mqtt_session<F: Fn(Notice)>(cfg: &Config, cancel: &Cancel, d: &mut Dispatch<F>) -> String {
    let id = format!("frigate-pip-{:08x}", random_u64() as u32);
    let mut m = match Mqtt::connect(&cfg.mqtt_host, cfg.mqtt_port, &id, &cfg.mqtt_user, &cfg.mqtt_password, 30, Duration::from_secs(5), cancel) {
        Ok(m) => m,
        Err(e) => return format!("MQTT {}:{}: {e}", cfg.mqtt_host, cfg.mqtt_port),
    };
    let prefix = cfg.mqtt_prefix.trim_end_matches('/');
    if let Err(e) = m.subscribe(&[format!("{prefix}/events"), format!("{prefix}/reviews")]) {
        return e.to_string();
    }
    crate::log!("Connected to MQTT {}", cfg.mqtt_host);
    (d.sink)(Notice::Connected(format!("mqtt://{}", cfg.mqtt_host)));
    let mut waiting_pong = false;
    loop {
        if cancel.is_set() {
            return "cancelled".into();
        }
        match m.read() {
            Ok(Packet::Publish { topic, payload }) => {
                waiting_pong = false;
                let t0 = Instant::now();
                let topic = topic.strip_prefix(prefix).unwrap_or(&topic).trim_start_matches('/');
                if !d.filter.wants(topic) {
                    continue;
                }
                if let Some(j) = Json::parse(&String::from_utf8_lossy(&payload)) {
                    d.handle(topic, &j, t0);
                }
            }
            Ok(Packet::Other) => waiting_pong = false,
            Ok(Packet::Idle) => {
                if waiting_pong {
                    return "no response from the MQTT broker".into();
                }
                if let Err(e) = m.ping() {
                    return e.to_string();
                }
                waiting_pong = true;
            }
            Err(e) => return format!("MQTT: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IniDoc;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// Fake Frigate server: `/api/version`, `/api/config` and `/ws` with a single `events` message.
    fn fake_frigate() -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in l.incoming().take(4) {
                let mut s = stream.unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                r.read_line(&mut first).unwrap();
                loop {
                    let mut h = String::new();
                    r.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                }
                if first.starts_with("GET /api/version") {
                    s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\n0.18.0").unwrap();
                } else if first.starts_with("GET /api/config") {
                    let body = r#"{"cameras": {"brama": {"detect": {"width": 640, "height": 360}, "live": {"streams": {"Main": "brama", "Sub": "brama_sub"}}}}}"#;
                    write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
                } else {
                    s.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n").unwrap();
                    let inner = r#"{"type": "new", "before": {}, "after": {"id": "e1", "camera": "brama", "label": "person", "top_score": 0.9, "false_positive": false, "stationary": false, "entered_zones": []}}"#;
                    let msg = r#"{"topic": "stats", "payload": "{}"}"#.to_string();
                    let ev = format!(r#"{{"topic": "events", "payload": {}}}"#, json_string(inner));
                    for m in [msg, ev] {
                        let b = m.as_bytes();
                        let mut f = vec![0x81, 126];
                        f.extend_from_slice(&(b.len() as u16).to_be_bytes());
                        f.extend_from_slice(b);
                        s.write_all(&f).unwrap();
                    }
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        });
        format!("http://{addr}")
    }

    fn json_string(s: &str) -> String {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }

    #[test]
    fn end_to_end_with_fake_frigate() {
        let url = fake_frigate();
        let cfg = Config::from_ini(&IniDoc::parse(&format!("[frigate]\nurl = {url}\n")));
        let (tx, rx) = mpsc::channel();
        let cancel = Cancel::new();
        start(cfg, cancel.clone(), move |n| {
            let _ = tx.send(n);
        });
        let mut got = Vec::new();
        while let Ok(n) = rx.recv_timeout(Duration::from_secs(3)) {
            let stop = matches!(n, Notice::Show(..));
            got.push(n);
            if stop {
                break;
            }
        }
        cancel.cancel();
        assert!(matches!(&got[0], Notice::Cameras(c) if c[0].sub.as_deref() == Some("brama_sub")));
        assert!(matches!(&got[1], Notice::Connected(_)));
        match &got[2] {
            Notice::Show(d, _) => assert_eq!((d.camera.as_str(), d.label.as_str()), ("brama", "person")),
            _ => panic!("oczekiwano Show"),
        }
    }

    #[test]
    fn connection_test_reports_each_step() {
        let url = fake_frigate();
        // RTSP on port 1 – nothing is listening, so this step must report an error.
        let cfg = Config::from_ini(&IniDoc::parse(&format!("[frigate]\nurl = {url}\nrtsp = rtsp://127.0.0.1:1/{{stream}}\n")));
        crate::i18n::set(crate::i18n::Lang::En);
        let mut lines = Vec::new();
        connection_test(&cfg, &Cancel::new(), &mut |l| lines.push(l));
        let all = lines.join("\n");
        assert!(lines[0].starts_with("✔ Frigate 0.18.0"), "{all}");
        assert!(lines[1].starts_with("✔ Cameras (1): brama"), "{all}");
        assert!(lines[2].starts_with("✔ Events via WebSocket"), "{all}");
        assert!(lines[3].starts_with("✖ RTSP video rtsp://127.0.0.1:1/brama_sub"), "{all}");
        assert_eq!(lines.last().map(String::as_str), Some("Done."));
        // Bad address: only the error and a hint.
        let bad = Config::from_ini(&IniDoc::parse("[frigate]\nurl = http://127.0.0.1:1\n"));
        let mut lines = Vec::new();
        connection_test(&bad, &Cancel::new(), &mut |l| lines.push(l));
        assert!(lines[0].starts_with("✖ Frigate at http://127.0.0.1:1"), "{lines:?}");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn camera_streams_from_frigate_config() {
        let j = Json::parse(
            r#"{"go2rtc": {"streams": {"a": [], "a_sub": [], "b": [], "c_sub": []}},
                "cameras": {
                  "a": {"enabled": true, "detect": {"width": 640, "height": 360},
                        "objects": {"track": ["person", "car"]}, "zones": {"Droga": {}},
                        "live": {"streams": {"Main Stream": "a", "Sub Stream": "a_sub"}}},
                  "b": {"detect": {"width": 1280, "height": 960}, "live": {"stream_name": "b"}},
                  "c": {"detect": {}, "live": {}},
                  "off": {"enabled": false}
                }}"#,
        )
        .unwrap();
        let cfg = Config::from_ini(&IniDoc::parse("[camera.c]\nname = Ogród\nmain = c_hd\n"));
        let cams = camera_infos(&j, &cfg);
        assert_eq!(cams.len(), 3);
        assert_eq!(
            cams[0],
            CameraInfo {
                name: "a".into(),
                title: "a".into(),
                main: Some("a".into()),
                sub: Some("a_sub".into()),
                aspect: 640.0 / 360.0,
                labels: vec!["person".into(), "car".into()],
                zones: vec!["Droga".into()],
            }
        );
        assert!(cams[1].labels.is_empty() && cams[1].zones.is_empty());
        assert_eq!(cams[1].main.as_deref(), Some("b"));
        assert_eq!(cams[1].sub, None);
        assert!((cams[1].aspect - 4.0 / 3.0).abs() < 1e-6);
        assert_eq!(cams[2].title, "Ogród");
        assert_eq!(cams[2].main.as_deref(), Some("c_hd"));
        assert_eq!(cams[2].sub.as_deref(), Some("c_sub"));
    }

    /// The whole connection test against a real Frigate, e.g. port 8971 with a login:
    /// `FRIGATE_URL=https://192.168.1.10:8971 FRIGATE_USER=viewer FRIGATE_PASSWORD=… cargo test --release live_connection -- --ignored --nocapture`
    /// (`FRIGATE_RTSP_TEMPLATE` overrides `rtsp://{host}:8554/{stream}`).
    #[test]
    #[ignore = "requires Frigate on the network"]
    fn live_connection() {
        let Ok(url) = std::env::var("FRIGATE_URL") else { return println!("skipped: set FRIGATE_URL (and FRIGATE_USER, FRIGATE_PASSWORD)") };
        let mut cfg = Config::from_ini(&IniDoc::parse(""));
        cfg.url = url;
        cfg.user = std::env::var("FRIGATE_USER").unwrap_or_default();
        cfg.password = std::env::var("FRIGATE_PASSWORD").unwrap_or_default();
        if let Ok(t) = std::env::var("FRIGATE_RTSP_TEMPLATE") {
            cfg.rtsp = t;
        }
        let mut lines = Vec::new();
        connection_test(&cfg, &Cancel::new(), &mut |l| {
            println!("{l}");
            lines.push(l);
        });
        assert!(!lines.iter().any(|l| l.starts_with('✖')), "{lines:#?}");
    }
}
