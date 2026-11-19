//! The action button on a camera window: one HTTP POST to a URL set per camera (e.g. a Home
//! Assistant webhook). Shared by Windows and Linux: the state machine, the request and the
//! button's place. The URL is a secret and never reaches the log.

// Until the window code uses it (Linux already allows dead code for the whole crate).
#![cfg_attr(windows, allow(dead_code))]

use std::io;
use std::time::{Duration, Instant};

const ARMED_FOR: Duration = Duration::from_secs(3);
/// A confirming click this soon after arming is the second half of a double-click.
const CONFIRM_AFTER: Duration = Duration::from_millis(400);
const RESULT_FOR: Duration = Duration::from_secs(2);
pub const TIMEOUT: Duration = Duration::from_secs(5);
/// The answer is read (and thrown away) up to this size, so the connection ends cleanly.
const ANSWER_LIMIT: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Look {
    Idle,
    Armed,
    Sending,
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Idle,
    Armed(Instant),
    Sending,
    Result(bool, Instant),
}

#[derive(Clone, Debug)]
pub struct Button {
    confirm: bool,
    state: State,
}

impl Button {
    pub fn new(confirm: bool) -> Self {
        Button { confirm, state: State::Idle }
    }

    /// A click (or the `a` key). `true` = send the request now.
    pub fn press(&mut self, now: Instant) -> bool {
        match self.current(now) {
            State::Idle if self.confirm => {
                self.state = State::Armed(now);
                false
            }
            State::Idle => {
                self.state = State::Sending;
                true
            }
            State::Armed(at) if now.duration_since(at) >= CONFIRM_AFTER => {
                self.state = State::Sending;
                true
            }
            _ => false,
        }
    }

    /// The request has ended: `ok` = HTTP 2xx.
    pub fn finished(&mut self, ok: bool, now: Instant) {
        self.state = State::Result(ok, now);
    }

    /// Back to idle without sending (e.g. the settings preview changed the action).
    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    /// Armed, sending or showing a result: the window must not close.
    pub fn busy(&self, now: Instant) -> bool {
        self.look(now) != Look::Idle
    }

    pub fn look(&self, now: Instant) -> Look {
        match self.current(now) {
            State::Idle => Look::Idle,
            State::Armed(_) => Look::Armed,
            State::Sending => Look::Sending,
            State::Result(true, _) => Look::Done,
            State::Result(false, _) => Look::Failed,
        }
    }

    /// The state at `now`: arming and results expire on their own.
    fn current(&self, now: Instant) -> State {
        match self.state {
            State::Armed(at) if now.duration_since(at) >= ARMED_FOR => State::Idle,
            State::Result(_, at) if now.duration_since(at) >= RESULT_FOR => State::Idle,
            s => s,
        }
    }
}

/// The button's text: "Gate", "Gate?", "Gate …", "Gate ✓", "Gate ✗".
pub fn text(label: &str, look: Look) -> String {
    match look {
        Look::Idle => label.to_string(),
        Look::Armed => crate::i18n::fmt(crate::i18n::t().action_ask, &[&label]),
        Look::Sending => format!("{label} …"),
        Look::Done => format!("{label} ✓"),
        Look::Failed => format!("{label} ✗"),
    }
}

/// The label set for the camera, or "Action" in the current language.
pub fn label_or_default(label: Option<&str>) -> String {
    label.map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).unwrap_or_else(|| crate::i18n::t().action_default.to_string())
}

/// An `http://` or `https://` address `net::request` can use; empty (no button) is valid too.
pub fn valid_url(url: &str) -> bool {
    let u = url.trim();
    u.is_empty() || crate::net::Url::parse(u).is_some_and(|p| p.scheme == "http" || p.scheme == "https")
}

fn body(camera: &str) -> String {
    format!("{{\"camera\": {}}}", crate::json::quote(camera))
}

/// The error in a few fixed words. The message is looked at but never printed: errors from
/// `net` and `tls` may quote the address.
fn describe(e: &io::Error) -> String {
    use io::ErrorKind as K;
    let msg = e.to_string();
    // DNS: `net::connect` (NotFound), getaddrinfo on Linux (message) or Windows (WSAHOST_NOT_FOUND…WSANO_DATA).
    let dns =
        e.kind() == K::NotFound || msg.starts_with("failed to lookup address") || (cfg!(windows) && matches!(e.raw_os_error(), Some(11001..=11004)));
    let words = if dns {
        "host not found"
    } else if msg.starts_with("the certificate of") {
        // `tls::first_use_or_pinned`: the pinned certificate no longer matches.
        "the server certificate has changed"
    } else if msg.starts_with("TLS:") {
        "TLS handshake failed"
    } else {
        match e.kind() {
            K::TimedOut | K::WouldBlock => "timed out",
            K::ConnectionRefused => "connection refused",
            K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe | K::UnexpectedEof => "connection closed",
            K::InvalidData => "invalid HTTP response",
            K::InvalidInput => "invalid address",
            K::Unsupported => "only http:// and https:// are supported",
            // The kind's own description ("host unreachable", "other error"…), never the message.
            k => return k.to_string(),
        }
    };
    words.to_string()
}

/// Sends the request (call it on its own thread). Logs "action <camera>: HTTP 200" or the
/// error; `true` = HTTP 2xx, i.e. the request was received (not that the gate opened).
pub fn send(url: &str, camera: &str) -> bool {
    let headers = [("Content-Type", "application/json".to_string())];
    match crate::net::request("POST", url.trim(), &headers, body(camera).as_bytes(), TIMEOUT, None) {
        Ok(resp) => {
            let status = resp.status;
            // The answer itself does not matter; reading it lets the server finish cleanly.
            let _ = resp.read_all(ANSWER_LIMIT);
            crate::log!("action {camera}: HTTP {status}");
            (200..300).contains(&status)
        }
        Err(e) => {
            crate::log!("action {camera}: {}", describe(&e));
            false
        }
    }
}

/// Runs `send` on its own thread and hands the result to `done`.
pub fn spawn(url: String, camera: String, done: impl FnOnce(bool) + Send + 'static) {
    let _ = std::thread::Builder::new().name("action".into()).spawn(move || done(send(&url, &camera)));
}

/// The pill in the bottom-left corner, in pixels: text size plus padding, 8 px (scaled) margin.
pub fn pill_rect(height: i32, text_w: f32, text_h: f32, scale: f32) -> (i32, i32, i32, i32) {
    let margin = (8.0 * scale).round() as i32;
    let pad_x = (10.0 * scale).round() as i32;
    let h = (text_h + 10.0 * scale).round() as i32;
    (margin, height - margin - h, margin + text_w.round() as i32 + 2 * pad_x, height - margin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    fn net_error_for_test(url: &str) -> io::Error {
        io::Error::other(format!("connect {url}: refused"))
    }

    #[test]
    fn with_confirmation_two_clicks_send() {
        let t = Instant::now();
        let mut b = Button::new(true);
        assert!(!b.press(t), "the first click only arms");
        assert_eq!(b.look(t), Look::Armed);
        assert!(b.press(t + ms(800)), "the second click sends");
        assert_eq!(b.look(t + ms(800)), Look::Sending);
    }

    #[test]
    fn a_double_click_does_not_confirm() {
        let t = Instant::now();
        let mut b = Button::new(true);
        b.press(t);
        assert!(!b.press(t + ms(200)), "too soon after arming");
        assert_eq!(b.look(t + ms(200)), Look::Armed);
    }

    #[test]
    fn arming_expires_after_three_seconds() {
        let t = Instant::now();
        let mut b = Button::new(true);
        b.press(t);
        assert_eq!(b.look(t + ms(3100)), Look::Idle);
        assert!(!b.press(t + ms(3100)), "arms again instead of sending");
    }

    #[test]
    fn without_confirmation_one_click_sends() {
        let t = Instant::now();
        let mut b = Button::new(false);
        assert!(b.press(t));
    }

    #[test]
    fn no_second_send_while_sending_and_results_show_for_two_seconds() {
        let t = Instant::now();
        let mut b = Button::new(false);
        b.press(t);
        assert!(!b.press(t + ms(100)));
        b.finished(true, t + ms(500));
        assert_eq!(b.look(t + ms(600)), Look::Done);
        assert!(b.busy(t + ms(2400)));
        assert_eq!(b.look(t + ms(2600)), Look::Idle);
        assert!(!b.busy(t + ms(2600)));
        b.press(t + ms(3000));
        b.finished(false, t + ms(3100));
        assert_eq!(b.look(t + ms(3200)), Look::Failed);
    }

    #[test]
    fn reset_returns_to_idle_without_sending() {
        let t = Instant::now();
        let mut b = Button::new(true);
        b.press(t);
        b.reset();
        assert!(!b.busy(t) && !b.press(t + ms(800)), "idle again: the next click only arms");
    }

    #[test]
    fn the_body_names_the_camera_as_json() {
        assert_eq!(body("front \"door\""), "{\"camera\": \"front \\\"door\\\"\"}");
    }

    #[test]
    fn errors_never_contain_the_url() {
        let url = "http://127.0.0.1:9/api/webhook/secret-id";
        let e = describe(&net_error_for_test(url));
        assert!(!e.contains("secret-id"), "{e}");
        // `net::request` itself quotes an address it cannot parse (no connection is made).
        let bad = "http://127.0.0.1:99999/api/webhook/secret-id";
        let e = describe(&crate::net::request("POST", bad, &[], b"", TIMEOUT, None).err().unwrap());
        assert_eq!(e, "invalid address");
    }

    /// A one-shot local HTTP server answering `status`; returns the request it got.
    fn serve_once(status: &str) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/api/webhook/x", l.local_addr().unwrap().port());
        let answer = format!("HTTP/1.1 {status}\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            // Headers, then the body announced by Content-Length.
            while !String::from_utf8_lossy(&got).contains("\"}") {
                let n = s.read(&mut buf).unwrap();
                assert!(n > 0, "the client closed early");
                got.extend_from_slice(&buf[..n]);
            }
            s.write_all(answer.as_bytes()).unwrap();
            String::from_utf8(got).unwrap()
        });
        (url, server)
    }

    #[test]
    fn send_posts_json_and_only_2xx_counts() {
        let (url, server) = serve_once("200 OK");
        assert!(send(&url, "gate"));
        let req = server.join().unwrap();
        assert!(req.starts_with("POST /api/webhook/x HTTP/1.1\r\n"), "{req}");
        assert!(req.contains("Content-Type: application/json\r\n") && req.ends_with("\r\n\r\n{\"camera\": \"gate\"}"), "{req}");
        let (url, server) = serve_once("404 Not Found");
        assert!(!send(&url, "gate"));
        server.join().unwrap();
    }

    #[test]
    fn only_http_urls_are_accepted() {
        assert!(valid_url("https://homeassistant.local:8123/api/webhook/x"));
        assert!(valid_url("http://10.0.0.2/relay/0?turn=on"));
        assert!(!valid_url("homeassistant.local/api/webhook/x"));
        assert!(!valid_url("ftp://x/y"));
        assert!(valid_url(""), "empty = no button, which is valid");
    }

    #[test]
    fn the_pill_sits_in_the_bottom_left_corner() {
        // (window height, text width, text height, scale) -> (x0, y0, x1, y1)
        assert_eq!(pill_rect(300, 40.0, 16.0, 1.0), (8, 300 - 8 - 26, 8 + 40 + 2 * 10, 300 - 8));
    }
}
