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
/// A request still running after this counts as failed, so a lost result cannot keep the
/// button (and the window) busy forever. `TIMEOUT` applies to each connect and read and DNS
/// has none, so a request can legitimately take much longer than 5 s.
const SENDING_MAX: Duration = Duration::from_secs(60);

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
    Sending(Instant),
    Result(bool, Instant),
}

#[derive(Clone, Debug)]
pub struct Button {
    confirm: bool,
    state: State,
    /// The current request's ticket; bumped by every send and by `reset`, so a late result
    /// of an older request is dropped.
    ticket: u32,
}

impl Button {
    pub fn new(confirm: bool) -> Self {
        Button { confirm, state: State::Idle, ticket: 0 }
    }

    /// A click (or the `a` key). `Some(ticket)` = send the request now and hand its result
    /// to `finished` with this ticket.
    pub fn press(&mut self, now: Instant) -> Option<u32> {
        match self.current(now) {
            State::Idle if self.confirm => {
                self.state = State::Armed(now);
                None
            }
            State::Idle => Some(self.start(now)),
            State::Armed(at) if now.duration_since(at) >= CONFIRM_AFTER => Some(self.start(now)),
            _ => None,
        }
    }

    fn start(&mut self, now: Instant) -> u32 {
        self.ticket = self.ticket.wrapping_add(1);
        self.state = State::Sending(now);
        self.ticket
    }

    /// The request with `ticket` has ended: `ok` = HTTP 2xx. Ignored unless that request is
    /// still the one being sent (not reset, replaced or past `SENDING_MAX`).
    pub fn finished(&mut self, ticket: u32, ok: bool, now: Instant) {
        if ticket == self.ticket && matches!(self.current(now), State::Sending(_)) {
            self.state = State::Result(ok, now);
        }
    }

    /// Back to idle without sending (e.g. the settings preview changed the action); the result
    /// of a request still running is dropped. A change to `confirm` needs a new `Button`.
    pub fn reset(&mut self) {
        self.ticket = self.ticket.wrapping_add(1);
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
            State::Sending(_) => Look::Sending,
            State::Result(true, _) => Look::Done,
            State::Result(false, _) => Look::Failed,
        }
    }

    /// When the look changes by itself (arming, a result or a request expires); `None` while
    /// idle. One timer for this moment is all a window needs.
    pub fn next_change(&self, now: Instant) -> Option<Instant> {
        match self.current(now) {
            State::Idle => None,
            State::Armed(at) => Some(at + ARMED_FOR),
            State::Sending(at) => Some(at + SENDING_MAX),
            State::Result(_, at) => Some(at + RESULT_FOR),
        }
    }

    /// The state at `now`: arming, results and requests expire on their own.
    fn current(&self, now: Instant) -> State {
        let s = match self.state {
            State::Sending(at) if now.duration_since(at) >= SENDING_MAX => State::Result(false, at + SENDING_MAX),
            s => s,
        };
        match s {
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

/// An `http://` or `https://` address `net::request` can use, with no space or control
/// character inside (it goes into the request line as it is); empty (no button) is valid too.
pub fn valid_url(url: &str) -> bool {
    let u = url.trim();
    u.is_empty()
        || (!u.chars().any(|c| c.is_whitespace() || c.is_control())
            && crate::net::Url::parse(u).is_some_and(|p| p.scheme == "http" || p.scheme == "https"))
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
        // The body does not matter. `net::request` asks for `Connection: close`, so dropping the
        // response simply closes the connection; reading the body would only delay the result.
        Ok(resp) => {
            crate::log!("action {camera}: HTTP {}", resp.status);
            (200..300).contains(&resp.status)
        }
        Err(e) => {
            crate::log!("action {camera}: {}", describe(&e));
            false
        }
    }
}

/// Runs `send` on its own thread and hands the result to `done`. `false` = the thread could
/// not start (logged as "action <camera>: cannot start"; `done` is never called): the caller
/// then calls `Button::finished(ticket, false, now)` itself.
pub fn spawn(url: String, camera: String, done: impl FnOnce(bool) + Send + 'static) -> bool {
    let name = camera.clone();
    match std::thread::Builder::new().name("action".into()).spawn(move || done(send(&url, &camera))) {
        Ok(_) => true,
        Err(_) => {
            crate::log!("action {name}: cannot start");
            false
        }
    }
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
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    #[test]
    fn with_confirmation_two_clicks_send() {
        let t = Instant::now();
        let mut b = Button::new(true);
        assert_eq!(b.press(t), None, "the first click only arms");
        assert_eq!(b.look(t), Look::Armed);
        assert!(b.press(t + ms(800)).is_some(), "the second click sends");
        assert_eq!(b.look(t + ms(800)), Look::Sending);
    }

    #[test]
    fn a_double_click_does_not_confirm() {
        let t = Instant::now();
        let mut b = Button::new(true);
        b.press(t);
        assert_eq!(b.press(t + ms(200)), None, "too soon after arming");
        assert_eq!(b.look(t + ms(200)), Look::Armed);
        assert_eq!(b.press(t + ms(399)), None);
        assert!(b.press(t + ms(400)).is_some(), "exactly 400 ms after arming confirms");
    }

    #[test]
    fn arming_expires_after_three_seconds() {
        let t = Instant::now();
        let mut b = Button::new(true);
        b.press(t);
        assert_eq!(b.look(t + ms(2999)), Look::Armed);
        assert_eq!(b.look(t + ms(3000)), Look::Idle, "expires at exactly 3 s");
        assert_eq!(b.press(t + ms(3000)), None, "arms again instead of sending");
        assert_eq!(b.look(t + ms(3000)), Look::Armed);
    }

    #[test]
    fn without_confirmation_one_click_sends() {
        let t = Instant::now();
        let mut b = Button::new(false);
        assert!(b.press(t).is_some());
    }

    #[test]
    fn no_second_send_while_sending_and_results_show_for_two_seconds() {
        let t = Instant::now();
        let mut b = Button::new(false);
        let ticket = b.press(t).unwrap();
        assert_eq!(b.press(t + ms(100)), None);
        b.finished(ticket, true, t + ms(500));
        assert_eq!(b.look(t + ms(600)), Look::Done);
        assert!(b.busy(t + ms(2499)));
        assert_eq!(b.look(t + ms(2500)), Look::Idle, "the result goes at exactly 2 s");
        assert!(!b.busy(t + ms(2500)));
        let ticket = b.press(t + ms(3000)).expect("idle again: one click sends");
        b.finished(ticket, false, t + ms(3100));
        assert_eq!(b.look(t + ms(3200)), Look::Failed);
    }

    #[test]
    fn a_result_with_another_ticket_is_ignored() {
        let t = Instant::now();
        let mut b = Button::new(false);
        let ticket = b.press(t).unwrap();
        b.finished(ticket.wrapping_add(1), true, t + ms(100));
        assert_eq!(b.look(t + ms(100)), Look::Sending);
    }

    #[test]
    fn reset_returns_to_idle_without_sending() {
        let t = Instant::now();
        let mut b = Button::new(true);
        b.press(t);
        b.reset();
        assert!(!b.busy(t));
        assert_eq!(b.press(t + ms(800)), None, "idle again: the next click only arms");
    }

    #[test]
    fn a_late_result_after_reset_is_dropped() {
        let t = Instant::now();
        let mut b = Button::new(false);
        let old = b.press(t).unwrap();
        b.reset();
        let new = b.press(t + ms(100)).unwrap();
        assert_ne!(old, new);
        b.finished(old, true, t + ms(200));
        assert_eq!(b.look(t + ms(200)), Look::Sending, "the old request's result is ignored");
        b.finished(new, false, t + ms(300));
        assert_eq!(b.look(t + ms(300)), Look::Failed);
    }

    #[test]
    fn a_request_running_for_a_minute_counts_as_failed() {
        let t = Instant::now();
        let mut b = Button::new(false);
        let ticket = b.press(t).unwrap();
        assert_eq!(b.look(t + ms(59_999)), Look::Sending);
        assert_eq!(b.look(t + ms(60_000)), Look::Failed);
        b.finished(ticket, true, t + ms(61_000));
        assert_eq!(b.look(t + ms(61_000)), Look::Failed, "too late: the request already counts as failed");
        assert!(b.busy(t + ms(61_999)));
        assert_eq!(b.look(t + ms(62_000)), Look::Idle, "shown 2 s from the expiry");
    }

    #[test]
    fn next_change_is_when_the_look_changes_by_itself() {
        let t = Instant::now();
        let mut b = Button::new(true);
        assert_eq!(b.next_change(t), None);
        b.press(t);
        assert_eq!(b.next_change(t), Some(t + ms(3000)));
        let ticket = b.press(t + ms(500)).unwrap();
        assert_eq!(b.next_change(t + ms(500)), Some(t + ms(60_500)));
        assert_eq!(b.next_change(t + ms(60_500)), Some(t + ms(62_500)), "the expired request shows as failed");
        b.finished(ticket, true, t + ms(1000));
        assert_eq!(b.next_change(t + ms(1000)), Some(t + ms(3000)));
        assert_eq!(b.next_change(t + ms(3000)), None);
    }

    #[test]
    fn the_body_names_the_camera_as_json() {
        assert_eq!(body("front \"door\""), "{\"camera\": \"front \\\"door\\\"\"}");
    }

    #[test]
    fn errors_never_contain_the_url() {
        let host = "secret-host.local";
        let url = format!("https://{host}:8123/api/webhook/secret-id");
        let cases = [
            (io::Error::other(format!("TLS: handshake with {url} failed")), "TLS handshake failed"),
            (io::Error::new(io::ErrorKind::InvalidData, format!("the certificate of {host}:8123 has changed")), "the server certificate has changed"),
            (io::Error::other(format!("failed to lookup address information: {host}")), "host not found"),
            (io::Error::new(io::ErrorKind::NotFound, format!("cannot resolve {host}")), "host not found"),
            (io::Error::new(io::ErrorKind::InvalidInput, format!("invalid address {url}")), "invalid address"),
            (io::Error::other(format!("connect {url}: refused")), "other error"),
        ];
        for (e, want) in cases {
            let got = describe(&e);
            assert_eq!(got, want, "{e}");
            assert!(!got.contains("secret"), "{got}");
        }
        // `net::request` itself quotes an address it cannot parse (no connection is made).
        let bad = "http://127.0.0.1:99999/api/webhook/secret-id";
        assert_eq!(describe(&crate::net::request("POST", bad, &[], b"", TIMEOUT, None).err().unwrap()), "invalid address");
    }

    #[test]
    fn only_http_urls_are_accepted() {
        assert!(valid_url("https://homeassistant.local:8123/api/webhook/x"));
        assert!(valid_url("http://10.0.0.2/relay/0?turn=on"));
        assert!(valid_url("  http://10.0.0.2/x  "), "spaces around are trimmed");
        assert!(!valid_url("homeassistant.local/api/webhook/x"));
        assert!(!valid_url("ftp://x/y"));
        assert!(!valid_url("http://h/a b"));
        assert!(!valid_url("http://h/a\tb"));
        assert!(!valid_url("http://h/a\r\nX-Injected: 1"));
        assert!(valid_url(""), "empty = no button, which is valid");
    }

    #[test]
    fn the_pill_sits_in_the_bottom_left_corner() {
        // (window height, text width, text height, scale) -> (x0, y0, x1, y1)
        assert_eq!(pill_rect(300, 40.0, 16.0, 1.0), (8, 300 - 8 - 26, 8 + 40 + 2 * 10, 300 - 8));
    }

    /// A one-shot local HTTP server answering `status`. The channel gets the request it
    /// received (head and body) or its error; it never waits more than about 10 s.
    fn serve_once(status: &'static str) -> (String, mpsc::Receiver<io::Result<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        let url = format!("http://127.0.0.1:{}/api/webhook/x", l.local_addr().unwrap().port());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(answer_once(&l, status));
        });
        (url, rx)
    }

    fn answer_once(l: &TcpListener, status: &str) -> io::Result<String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let s = loop {
            match l.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(ms(10)),
                Err(e) => return Err(e),
            }
        };
        s.set_nonblocking(false)?;
        s.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut r = BufReader::new(s.try_clone()?);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            if r.read_line(&mut line)? == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed in the headers"));
            }
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let len = head
            .lines()
            .filter_map(|h| h.split_once(':'))
            .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
            .and_then(|(_, v)| v.trim().parse::<usize>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no Content-Length"))?;
        let mut body = vec![0; len];
        r.read_exact(&mut body)?;
        (&s).write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())?;
        Ok(head + &String::from_utf8_lossy(&body))
    }

    fn received(rx: mpsc::Receiver<io::Result<String>>) -> String {
        rx.recv_timeout(Duration::from_secs(15)).expect("the server thread answers").expect("the server got a request")
    }

    #[test]
    fn send_posts_json_and_only_2xx_counts() {
        let (url, rx) = serve_once("200 OK");
        assert!(send(&url, "gate"));
        let req = received(rx);
        assert!(req.starts_with("POST /api/webhook/x HTTP/1.1\r\n"), "{req}");
        assert!(req.contains("Content-Type: application/json\r\n") && req.ends_with("\r\n\r\n{\"camera\": \"gate\"}"), "{req}");

        let (url, rx) = serve_once("404 Not Found");
        assert!(!send(&url, "gate"));
        let req = received(rx);
        assert!(req.starts_with("POST /api/webhook/x HTTP/1.1\r\n") && req.ends_with("{\"camera\": \"gate\"}"), "{req}");
    }
}
