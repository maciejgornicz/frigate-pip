//! The action buttons on a camera window: up to four per camera, each one HTTP POST to a URL
//! set per camera (e.g. a Home Assistant webhook). Shared by Windows and Linux: the state
//! machine, the request and the buttons' places. The URL is a secret and never reaches the log.

use std::io;
use std::sync::atomic::{AtomicU32, Ordering};
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
/// Tickets are unique across all buttons, so a result can be offered to every window's button
/// and only the one that sent the request takes it (a closed window's handle may be reused).
static NEXT_TICKET: AtomicU32 = AtomicU32::new(1);

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
    /// The current request's ticket; a new one for every send, so a late result of an older
    /// request is dropped.
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

    /// Asks for a confirming second click.
    pub fn confirm(&self) -> bool {
        self.confirm
    }

    /// Drops an armed first click without sending (another action of the window was pressed,
    /// and only one may be armed); no effect in any other state.
    pub fn disarm(&mut self) {
        if let State::Armed(_) = self.state {
            self.state = State::Idle;
        }
    }

    fn start(&mut self, now: Instant) -> u32 {
        self.ticket = NEXT_TICKET.fetch_add(1, Ordering::Relaxed);
        self.state = State::Sending(now);
        self.ticket
    }

    /// The request with `ticket` has ended: `ok` = HTTP 2xx. Ignored unless that request is
    /// still the one being sent (not replaced or past `SENDING_MAX`); `true` = taken.
    pub fn finished(&mut self, ticket: u32, ok: bool, now: Instant) -> bool {
        let mine = ticket == self.ticket && matches!(self.current(now), State::Sending(_));
        if mine {
            self.state = State::Result(ok, now);
        }
        mine
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

/// A window's action buttons by number: `[n - 1]` is action `n`; `None` = it has no URL.
pub type Buttons = [Option<Button>; crate::config::ACTIONS];

/// A rectangle in pixels: (left, top, right, bottom); right and bottom are outside.
pub type Rect = (i32, i32, i32, i32);

/// Where an action's pill was drawn: (number, rectangle).
pub type Placed = (usize, Rect);

/// One action button as a window draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// The action's number, 1 to `ACTIONS`.
    pub n: usize,
    pub text: String,
    pub look: Look,
}

/// Action `n`'s button, if `n` is a valid number and the action has one.
fn slot(buttons: &Buttons, n: usize) -> Option<&Button> {
    buttons.get(n.checked_sub(1)?)?.as_ref()
}

fn slot_mut(buttons: &mut Buttons, n: usize) -> Option<&mut Button> {
    buttons.get_mut(n.checked_sub(1)?)?.as_mut()
}

/// Matches a window's buttons to its actions: `confirm[n - 1]` is action `n`'s confirmation
/// setting, `None` when it has no URL. A button goes when its action loses its URL, a new one
/// comes when an action appears or its confirmation setting changes, and one whose action only
/// got another label or URL is kept (with whatever it is doing).
pub fn refresh(buttons: &mut Buttons, confirm: &[Option<bool>; crate::config::ACTIONS]) {
    for (b, want) in buttons.iter_mut().zip(confirm) {
        match (*want, &*b) {
            (None, _) => *b = None,
            (Some(confirm), Some(old)) if old.confirm() == confirm => {}
            (Some(confirm), _) => *b = Some(Button::new(confirm)),
        }
    }
}

/// A click on action `n`. When it does something (arms `n` or sends it), an armed first click
/// of any other action of the window is dropped: only one may be armed. A click that does
/// nothing (`n` has no button, is sending or showing a result, or this is the second half of a
/// double-click) leaves the others alone. `Some(ticket)` = send action `n` now, as
/// `Button::press`.
pub fn press(buttons: &mut Buttons, n: usize, now: Instant) -> Option<u32> {
    let b = slot_mut(buttons, n)?;
    let before = b.look(now);
    let ticket = b.press(now);
    if ticket.is_some() || b.look(now) != before {
        for (k, other) in buttons.iter_mut().enumerate() {
            if let (true, Some(other)) = (k != n - 1, other) {
                other.disarm();
            }
        }
    }
    ticket
}

/// Any of the buttons is armed, sending or showing a result: the window must not close.
pub fn any_busy(buttons: &Buttons, now: Instant) -> bool {
    buttons.iter().flatten().any(|b| b.busy(now))
}

/// The earliest moment one of the buttons' looks changes by itself (see `Button::next_change`).
pub fn next_change(buttons: &Buttons, now: Instant) -> Option<Instant> {
    buttons.iter().flatten().filter_map(|b| b.next_change(now)).min()
}

/// What a window draws for its actions, given as (number, label) in number order: one
/// `Shown` per action that has a button, with the text for its look at `now`.
pub fn shown(buttons: &Buttons, actions: &[(usize, String)], now: Instant) -> Vec<Shown> {
    actions
        .iter()
        .filter_map(|(n, label)| {
            let look = slot(buttons, *n)?.look(now);
            Some(Shown { n: *n, text: text(label, look), look })
        })
        .collect()
}

/// The drawn pills that still have a button: one drawn just before its action went is not
/// clickable.
pub fn live_rects(buttons: &Buttons, mut drawn: Vec<Placed>) -> Vec<Placed> {
    drawn.retain(|(n, _)| slot(buttons, *n).is_some());
    drawn
}

/// The action whose pill holds the point, given where the pills were drawn.
pub fn hit(pills: &[Placed], x: i32, y: i32) -> Option<usize> {
    pills.iter().find(|(_, (l, t, r, b))| x >= *l && x < *r && y >= *t && y < *b).map(|(n, _)| *n)
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

/// `{"camera": "<camera>", "action": <n>}`.
fn body(camera: &str, n: usize) -> String {
    format!("{{\"camera\": {}, \"action\": {n}}}", crate::json::quote(camera))
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

/// Sends the camera's action `n` (call it on its own thread). Logs "action <camera> <n>: HTTP
/// 200" or the error, never the URL; `true` = HTTP 2xx, i.e. the request was received (not
/// that the gate opened).
pub fn send(url: &str, camera: &str, n: usize) -> bool {
    let headers = [("Content-Type", "application/json".to_string())];
    match crate::net::request("POST", url.trim(), &headers, body(camera, n).as_bytes(), TIMEOUT, None) {
        // The body does not matter. `net::request` asks for `Connection: close`, so dropping the
        // response simply closes the connection; reading the body would only delay the result.
        Ok(resp) => {
            crate::log!("action {camera} {n}: HTTP {}", resp.status);
            (200..300).contains(&resp.status)
        }
        Err(e) => {
            crate::log!("action {camera} {n}: {}", describe(&e));
            false
        }
    }
}

/// Runs `send` on its own thread and hands the result to `done`. `false` = the thread could
/// not start (logged as "action <camera> <n>: cannot start"; `done` is never called): the
/// caller then calls `Button::finished(ticket, false, now)` itself.
pub fn spawn(url: String, camera: String, n: usize, done: impl FnOnce(bool) + Send + 'static) -> bool {
    let name = camera.clone();
    match std::thread::Builder::new().name("action".into()).spawn(move || done(send(&url, &camera, n))) {
        Ok(_) => true,
        Err(_) => {
            crate::log!("action {name} {n}: cannot start");
            false
        }
    }
}

/// The pills' margin to the window edges (and to a control beside them), their padding and the
/// gap between two pills, in unscaled pixels.
const PILL_MARGIN: f32 = 8.0;
const PILL_PAD_X: f32 = 10.0;
const PILL_PAD_Y: f32 = 5.0;
const PILL_GAP: f32 = 6.0;
/// The least text room a pill keeps when the row is squeezed, in unscaled pixels: a letter or
/// two with the trimming sign.
const PILL_MIN_TEXT: f32 = 16.0;

fn px(v: f32, scale: f32) -> i32 {
    (v * scale).round() as i32
}

/// The pills' top and bottom in pixels: one line of text plus padding, a margin above the
/// window's bottom edge.
pub fn pill_band(height: i32, text_h: f32, scale: f32) -> (i32, i32) {
    let margin = px(PILL_MARGIN, scale);
    (height - margin - (text_h + 2.0 * PILL_PAD_Y * scale).round() as i32, height - margin)
}

/// The widest text a pill is measured with, in pixels: wider than any screen, small enough that
/// sums of a few pills never overflow.
const PILL_TEXT_CAP: f32 = 100_000.0;

/// A row of pills from the bottom-left corner, one per text width, left to right with a gap
/// between them, as (left, top, right, bottom) in pixels. Each pill is its text plus padding.
/// When that row would pass a margin before `right_limit` (the window's right edge or the
/// leftmost control level with the row; `None` = no limit), the room is shared fairly
/// ("water-filling", see `fair_widths`): a pill no wider than an equal share keeps its own
/// width, and the longer pills split what is left, their texts trimmed to `pill_text_room`. A
/// trimmed pill keeps at least `PILL_MIN_TEXT` of text room; a pill narrower than that needs
/// only its own width. Pills that do not fit even so are dropped from the right, so the result
/// can be shorter than `text_widths` (empty when not even one fits). A row that fits is never
/// changed.
pub fn pill_row(height: i32, text_widths: &[f32], text_h: f32, scale: f32, right_limit: Option<i32>) -> Vec<Rect> {
    let (margin, pad_x, gap) = (px(PILL_MARGIN, scale), px(PILL_PAD_X, scale), px(PILL_GAP, scale));
    let (top, bottom) = pill_band(height, text_h, scale);
    // Rounded up, so that the text room of a pill that is not shared is never short of its
    // text; NaN and negative widths count as none, huge ones as `PILL_TEXT_CAP`.
    let natural: Vec<i32> = text_widths.iter().map(|w| (w.clamp(0.0, PILL_TEXT_CAP).ceil() as i32).saturating_add(2 * pad_x)).collect();
    let widths = match right_limit.map(|r| r.saturating_sub(2 * margin)) {
        Some(room) if row_width(&natural, gap) > room => {
            let least = 2 * pad_x + px(PILL_MIN_TEXT, scale).max(1);
            fair_widths(&natural[..fitting_count(&natural, least, gap, room)], room, gap)
        }
        _ => natural,
    };
    let mut x = margin;
    widths
        .into_iter()
        .map(|w| {
            let pill = (x, top, x.saturating_add(w), bottom);
            x = x.saturating_add(w).saturating_add(gap);
            pill
        })
        .collect()
}

/// The pills' widths with a gap between each two.
fn row_width(widths: &[i32], gap: i32) -> i32 {
    let gaps = gap.saturating_mul(widths.len().saturating_sub(1) as i32);
    widths.iter().fold(gaps, |sum, &w| sum.saturating_add(w))
}

/// How many pills from the left fit in `room` at their smallest (`least`, or their own width
/// when that is less), with the gaps.
fn fitting_count(natural: &[i32], least: i32, gap: i32, room: i32) -> usize {
    let mut used = gap.saturating_neg();
    natural
        .iter()
        .take_while(|&&w| {
            used = used.saturating_add(w.min(least)).saturating_add(gap);
            used <= room
        })
        .count()
}

/// Max-min fair widths ("water-filling") for pills that fit in `room` at their smallest:
/// pills no wider than an equal share of what is left keep their own width, until the rest are
/// all wider than the share; those get the share, and the pixels the division leaves over go
/// one each to the leftmost of them, so the row fills `room` exactly. The share only grows from
/// round to round, and it never falls below the smallest width the caller allowed, since the
/// pills fit at that.
fn fair_widths(natural: &[i32], room: i32, gap: i32) -> Vec<i32> {
    let mut widths = natural.to_vec();
    let mut avail = room.saturating_sub(gap.saturating_mul(widths.len().saturating_sub(1) as i32));
    let mut long: Vec<usize> = (0..widths.len()).collect();
    while !long.is_empty() {
        let share = avail / long.len() as i32;
        let (fit, rest): (Vec<usize>, Vec<usize>) = long.iter().partition(|&&i| widths[i] <= share);
        if fit.is_empty() {
            let extra = (avail % long.len() as i32) as usize;
            for (k, &i) in long.iter().enumerate() {
                widths[i] = share + i32::from(k < extra);
            }
            break;
        }
        avail -= fit.iter().map(|&i| widths[i]).sum::<i32>();
        long = rest;
    }
    widths
}

/// The room for the text inside a pill from `pill_row`, in pixels: its width less the padding.
pub fn pill_text_room(pill: Rect, scale: f32) -> f32 {
    (pill.2 - pill.0 - 2 * px(PILL_PAD_X, scale)).max(0) as f32
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

    fn buttons(confirm: [Option<bool>; crate::config::ACTIONS]) -> Buttons {
        confirm.map(|c| c.map(Button::new))
    }

    #[test]
    fn finished_says_whether_the_result_was_taken() {
        let t = Instant::now();
        let mut b = Button::new(false);
        let ticket = b.press(t).unwrap();
        assert!(!b.finished(ticket.wrapping_add(1), true, t + ms(10)), "another ticket");
        assert!(b.finished(ticket, true, t + ms(20)));
        assert!(!b.finished(ticket, true, t + ms(30)), "already taken");
    }

    #[test]
    fn refresh_works_per_action_number() {
        let t = Instant::now();
        let mut b = buttons([Some(true), None, Some(false), Some(true)]);
        press(&mut b, 1, t);
        press(&mut b, 4, t + ms(10));
        refresh(&mut b, &[Some(true), Some(false), None, Some(false)]);
        assert_eq!(b[0].as_ref().map(|b| b.look(t + ms(10))), Some(Look::Idle), "1: kept, but no longer armed (4 was pressed)");
        assert!(b[1].as_ref().is_some_and(|b| !b.confirm()), "2: appeared");
        assert!(b[2].is_none(), "3: lost its URL");
        assert!(b[3].as_ref().is_some_and(|b| !b.confirm() && b.look(t + ms(10)) == Look::Idle), "4: new confirmation setting, new button");

        let mut b = buttons([Some(true), Some(true), None, None]);
        press(&mut b, 2, t);
        refresh(&mut b, &[None, Some(true), None, None]);
        assert!(b[0].is_none());
        assert_eq!(b[1].as_ref().unwrap().look(t), Look::Armed, "kept with its state: nothing moves to another number");
    }

    #[test]
    fn only_one_action_is_armed_at_a_time() {
        let t = Instant::now();
        let mut b = buttons([Some(true), Some(true), Some(false), None]);
        assert_eq!(press(&mut b, 1, t), None);
        assert_eq!(press(&mut b, 2, t + ms(500)), None, "2 arms…");
        assert_eq!(b[0].as_ref().unwrap().look(t + ms(500)), Look::Idle, "…and 1 is disarmed");
        let ticket = press(&mut b, 3, t + ms(600)).expect("3 needs no confirmation: it sends");
        assert_eq!(b[1].as_ref().unwrap().look(t + ms(600)), Look::Idle, "pressing 3 disarmed 2");
        assert_eq!(press(&mut b, 2, t + ms(700)), None, "2 arms again");
        assert_eq!(b[2].as_ref().unwrap().look(t + ms(700)), Look::Sending, "a request running is left alone");
        b[2].as_mut().unwrap().finished(ticket, true, t + ms(800));
        assert_eq!(press(&mut b, 4, t + ms(900)), None, "no button 4");
        assert_eq!(b[1].as_ref().unwrap().look(t + ms(900)), Look::Armed, "a click on nothing disarms nothing");
        assert!(press(&mut b, 2, t + ms(1200)).is_some(), "the second click sends 2");
        assert_eq!(b[2].as_ref().unwrap().look(t + ms(1200)), Look::Done, "results are left alone");
        assert_eq!(press(&mut b, 0, t), None);
        assert_eq!(press(&mut b, 9, t), None);
    }

    #[test]
    fn a_click_that_does_nothing_disarms_nothing() {
        let t = Instant::now();
        let mut b = buttons([Some(true), Some(false), Some(true), None]);
        let ticket = press(&mut b, 2, t).expect("2 sends");
        press(&mut b, 1, t + ms(100));
        assert_eq!(press(&mut b, 2, t + ms(200)), None, "2 is sending: the click does nothing…");
        assert_eq!(b[0].as_ref().unwrap().look(t + ms(200)), Look::Armed, "…so 1 stays armed");
        b[1].as_mut().unwrap().finished(ticket, true, t + ms(300));
        assert_eq!(press(&mut b, 2, t + ms(400)), None, "2 shows its result: nothing again");
        assert_eq!(b[0].as_ref().unwrap().look(t + ms(400)), Look::Armed);
        press(&mut b, 3, t + ms(500));
        assert_eq!(b[0].as_ref().unwrap().look(t + ms(500)), Look::Idle, "arming 3 disarms 1");
        assert_eq!(press(&mut b, 3, t + ms(600)), None, "the second half of a double-click on 3…");
        assert_eq!(b[2].as_ref().unwrap().look(t + ms(600)), Look::Armed, "…keeps 3 armed");
    }

    #[test]
    fn shown_lists_the_actions_with_a_button_by_number() {
        let t = Instant::now();
        let mut b = buttons([Some(false), None, Some(true), None]);
        press(&mut b, 1, t);
        let actions = [(1, "Gate".to_string()), (2, "Lost".to_string()), (3, "Lights".to_string())];
        let want = [Shown { n: 1, text: text("Gate", Look::Sending), look: Look::Sending }, Shown { n: 3, text: "Lights".into(), look: Look::Idle }];
        assert_eq!(shown(&b, &actions, t), want, "2 has no button (yet)");
        assert_eq!(shown(&b, &[], t), []);
    }

    #[test]
    fn only_drawn_pills_with_a_button_count() {
        let b = buttons([Some(false), None, Some(true), None]);
        let drawn = vec![(1, (8, 100, 60, 126)), (2, (66, 100, 120, 126)), (3, (126, 100, 180, 126)), (0, (0, 0, 1, 1)), (9, (0, 0, 1, 1))];
        assert_eq!(live_rects(&b, drawn), [(1, (8, 100, 60, 126)), (3, (126, 100, 180, 126))]);
    }

    #[test]
    fn busy_and_next_change_cover_all_buttons() {
        let t = Instant::now();
        let mut b = buttons([None, Some(true), Some(false), None]);
        assert!(!any_busy(&b, t));
        assert_eq!(next_change(&b, t), None);
        press(&mut b, 3, t);
        press(&mut b, 2, t + ms(1000));
        assert!(any_busy(&b, t + ms(1000)));
        assert_eq!(next_change(&b, t + ms(1000)), Some(t + ms(4000)), "2's arming expires before 3's request");
    }

    #[test]
    fn the_pill_under_the_point_names_its_action() {
        let pills = [(1, (8, 100, 60, 126)), (3, (66, 100, 120, 126))];
        assert_eq!(hit(&pills, 8, 100), Some(1));
        assert_eq!(hit(&pills, 59, 125), Some(1));
        assert_eq!(hit(&pills, 60, 110), None, "the gap");
        assert_eq!(hit(&pills, 70, 110), Some(3), "by number, not by place");
        assert_eq!(hit(&pills, 70, 126), None);
        assert_eq!(hit(&[], 10, 110), None);
    }

    #[test]
    fn tickets_differ_between_buttons() {
        let t = Instant::now();
        let (mut a, mut b) = (Button::new(false), Button::new(false));
        let ta = a.press(t).unwrap();
        let tb = b.press(t).unwrap();
        assert_ne!(ta, tb);
        b.finished(ta, true, t + ms(100));
        assert_eq!(b.look(t + ms(100)), Look::Sending, "another button's result is ignored");
        a.finished(ta, true, t + ms(100));
        assert_eq!(a.look(t + ms(100)), Look::Done);
    }

    #[test]
    fn a_late_result_of_an_expired_request_is_dropped() {
        let t = Instant::now();
        let mut b = Button::new(false);
        let old = b.press(t).unwrap();
        assert_eq!(b.look(t + SENDING_MAX), Look::Failed, "past SENDING_MAX the request counts as failed");
        let again = t + SENDING_MAX + RESULT_FOR;
        assert_eq!(b.look(again), Look::Idle);
        let new = b.press(again).expect("idle again: one click sends");
        assert_ne!(old, new);
        b.finished(old, true, again + ms(100));
        assert_eq!(b.look(again + ms(100)), Look::Sending, "the old request's result is ignored");
        b.finished(new, false, again + ms(200));
        assert_eq!(b.look(again + ms(200)), Look::Failed);
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
    fn disarm_drops_an_armed_click_only() {
        let t = Instant::now();
        let mut b = Button::new(true);
        b.press(t);
        b.disarm();
        assert_eq!(b.look(t), Look::Idle, "disarmed without sending");
        assert_eq!(b.press(t + ms(800)), None, "the next click arms again");
        assert_eq!(b.look(t + ms(800)), Look::Armed);

        let mut b = Button::new(false);
        let ticket = b.press(t).unwrap();
        b.disarm();
        assert_eq!(b.look(t), Look::Sending, "a request being sent is not dropped");
        b.finished(ticket, true, t + ms(100));
        b.disarm();
        assert_eq!(b.look(t + ms(100)), Look::Done, "nor its result");

        let mut b = Button::new(true);
        b.disarm();
        assert_eq!(b.look(t), Look::Idle, "idle stays idle");
    }

    #[test]
    fn the_body_names_the_camera_and_the_action_as_json() {
        assert_eq!(body("front \"door\"", 3), "{\"camera\": \"front \\\"door\\\"\", \"action\": 3}");
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
    fn one_pill_sits_in_the_bottom_left_corner() {
        // (window height, text widths, text height, scale, limit) -> [(x0, y0, x1, y1)]
        let pill = (8, 300 - 8 - 26, 8 + 40 + 2 * 10, 300 - 8);
        assert_eq!(pill_row(300, &[40.0], 16.0, 1.0, None), [pill]);
        assert_eq!(pill_row(300, &[40.0], 16.0, 1.0, Some(400)), [pill]);
        assert_eq!(pill_band(300, 16.0, 1.0), (pill.1, pill.3));
        assert_eq!(pill_text_room(pill, 1.0), 40.0);
        assert_eq!(pill_row(300, &[39.2], 16.0, 1.0, None), [pill], "rounded up: the text always fits");
        assert!(pill_row(300, &[], 16.0, 1.0, Some(400)).is_empty());
    }

    #[test]
    fn pills_follow_each_other_with_a_gap() {
        let (t, b) = (300 - 8 - 26, 300 - 8);
        let two = [(8, t, 68, b), (74, t, 124, b)];
        assert_eq!(pill_row(300, &[40.0, 30.0], 16.0, 1.0, Some(400)), two);
        let four = [(8, t, 68, b), (74, t, 124, b), (130, t, 170, b), (176, t, 206, b)];
        assert_eq!(pill_row(300, &[40.0, 30.0, 20.0, 10.0], 16.0, 1.0, Some(400)), four);
        assert_eq!(pill_row(300, &[40.0, 30.0, 20.0], 16.0, 1.0, None), four[..3]);
        // A row ending exactly a margin before the limit is not squeezed.
        assert_eq!(pill_row(300, &[40.0, 30.0, 20.0, 10.0], 16.0, 1.0, Some(206 + 8)), four);
        // At 150 % everything grows: margin 12, padding 15, gap 9.
        let (t, b) = (450 - 12 - 39, 450 - 12);
        assert_eq!(pill_row(450, &[60.0, 45.0], 24.0, 1.5, None), [(12, t, 102, b), (111, t, 186, b)]);
    }

    #[test]
    fn long_pills_share_equally_and_fill_the_row() {
        // Room: 200 - 8 - 8 = 184; three pills and two gaps of 6: 172 / 3 = 57, one pixel over,
        // which goes to the leftmost pill, so the row ends exactly at 192.
        let (t, b) = (300 - 8 - 26, 300 - 8);
        let row = pill_row(300, &[100.0, 100.0, 100.0], 16.0, 1.0, Some(200));
        assert_eq!(row, [(8, t, 66, b), (72, t, 129, b), (135, t, 192, b)]);
        assert_eq!(row.iter().map(|&p| pill_text_room(p, 1.0)).collect::<Vec<_>>(), [38.0, 37.0, 37.0]);
    }

    #[test]
    fn the_helpers_count_and_share() {
        // (natural widths, least, gap, room) -> how many fit at their smallest
        assert_eq!(fitting_count(&[70, 70, 70], 36, 6, 78), 2);
        assert_eq!(fitting_count(&[70, 70, 70], 36, 6, 77), 1);
        assert_eq!(fitting_count(&[25, 70], 36, 6, 30), 1, "a pill below the minimum counts at its own width");
        assert_eq!(fitting_count(&[70], 36, 6, 35), 0);
        assert_eq!(fitting_count(&[], 36, 6, 100), 0);
        assert_eq!(fitting_count(&[i32::MAX, i32::MAX], 36, i32::MAX, i32::MAX), 2, "saturates instead of overflowing");
        // (natural widths, room, gap) -> widths
        assert_eq!(fair_widths(&[30, 70, 220], 184, 6), [30, 70, 72]);
        assert_eq!(fair_widths(&[120, 40, 120], 184, 6), [66, 40, 66]);
        assert_eq!(fair_widths(&[100, 100, 100, 100], 100, 6), [21, 21, 20, 20], "82 = 4 * 20 + 2");
        assert_eq!(fair_widths(&[], 100, 6), Vec::<i32>::new());
        assert_eq!(row_width(&[10, 20, 30], 6), 72);
        assert_eq!(row_width(&[], 6), 0);
        assert_eq!(row_width(&[i32::MAX, 1], 6), i32::MAX, "saturates");
    }

    #[test]
    fn huge_or_odd_text_widths_do_not_overflow() {
        let texts = [f32::INFINITY, f32::MAX, f32::NAN, -5.0, f32::NEG_INFINITY];
        let free = pill_row(300, &texts, 16.0, 1.0, None);
        assert_eq!(free.len(), texts.len());
        assert_eq!(free[0].2 - free[0].0, PILL_TEXT_CAP as i32 + 20, "capped");
        assert_eq!(free[2].2 - free[2].0, 20, "NaN = no text");
        assert_eq!(free[3].2 - free[3].0, 20, "negative = no text");
        assert!(free.windows(2).all(|w| w[1].0 > w[0].2), "{free:?}");
        let row = pill_row(300, &texts, 16.0, 1.0, Some(400));
        assert!(!row.is_empty() && row.iter().all(|p| p.0 >= 8 && p.2 <= 392), "{row:?}");
        assert!(pill_row(300, &texts, 16.0, 1.0, Some(i32::MIN)).is_empty());
        assert_eq!(pill_row(300, &texts, 16.0, 1.0, Some(i32::MAX)).len(), texts.len());
    }

    #[test]
    fn a_short_pill_keeps_its_width_and_leaves_the_rest_to_the_long_ones() {
        // Share 172 / 3 = 57: the 40 wide pill keeps its width; the others split 132 into 66.
        let (t, b) = (300 - 8 - 26, 300 - 8);
        let row = pill_row(300, &[100.0, 20.0, 100.0], 16.0, 1.0, Some(200));
        assert_eq!(row, [(8, t, 74, b), (80, t, 120, b), (126, t, 192, b)]);
        assert_eq!(pill_text_room(row[1], 1.0), 20.0, "not trimmed");
        // Water-filling over several rounds: share 172 / 3 = 57 fixes the 30 wide pill, then
        // 142 / 2 = 71 fixes the 70 wide one, and the last pill gets the 72 left.
        let row = pill_row(300, &[10.0, 50.0, 200.0], 16.0, 1.0, Some(200));
        assert_eq!(row, [(8, t, 38, b), (44, t, 114, b), (120, t, 192, b)]);
    }

    #[test]
    fn pills_narrower_than_the_minimum_are_not_dropped_while_they_fit() {
        let (t, b) = (300 - 8 - 26, 300 - 8);
        // Room 100: four pills of 25 and three gaps need 118; three need 87 and keep their width.
        assert_eq!(pill_row(300, &[5.0; 4], 16.0, 1.0, Some(116)), [(8, t, 33, b), (39, t, 64, b), (70, t, 95, b)]);
        // Room 30: the 25 wide pill fits; the long one would need at least 36 more.
        assert_eq!(pill_row(300, &[5.0, 50.0], 16.0, 1.0, Some(30 + 16)), [(8, t, 33, b)]);
    }

    #[test]
    fn pills_never_pass_the_limit_or_overlap() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (margin, gap) = (px(PILL_MARGIN, scale), px(PILL_GAP, scale));
            let least = 2 * px(PILL_PAD_X, scale) + px(PILL_MIN_TEXT, scale);
            // Text widths of all shapes: mixed, short, one long and two short, all exactly at the
            // minimum, and around the minimum and the share.
            let m = px(PILL_MIN_TEXT, scale) as f32;
            let sets: [&[f32]; 7] = [
                &[120.0, 0.0, 33.3, 250.0],
                &[5.0, 5.0, 5.0, 5.0],
                &[5.0, 50.0, 5.0, 200.0],
                &[300.0, 5.0, 5.0],
                &[m, m, m, m],
                &[m - 1.0, m, m + 1.0, 3.0 * m],
                &[56.0, 57.0, 58.0, 59.0],
            ];
            for texts in sets {
                for count in 1..=texts.len() {
                    let texts = &texts[..count];
                    let free = pill_row(300, texts, 16.0, scale, None);
                    let natural: Vec<i32> = free.iter().map(|p| p.2 - p.0).collect();
                    let mut before: Option<Vec<i32>> = None;
                    for limit in -20..=800 {
                        let row = pill_row(300, texts, 16.0, scale, Some(limit));
                        let case = format!("scale {scale}, texts {texts:?}, limit {limit}: {row:?}");
                        let room = limit - 2 * margin;
                        if row_width(&natural, gap) <= room {
                            assert_eq!(row, free, "a row that fits is not changed: {case}");
                        }
                        assert!(row.len() <= count, "{case}");
                        if row.len() < count {
                            let smallest = row_width(&natural[..=row.len()].iter().map(|&w| w.min(least)).collect::<Vec<_>>(), gap);
                            assert!(smallest > room, "dropped only when the next pill cannot fit: {case}");
                        }
                        let widths: Vec<i32> = row.iter().map(|p| p.2 - p.0).collect();
                        for (i, &p) in row.iter().enumerate() {
                            assert!(p.0 >= margin && p.2 <= limit - margin, "{case}");
                            if i > 0 {
                                assert_eq!(p.0, row[i - 1].2 + gap, "exact gaps, no overlap: {case}");
                            }
                            assert!(widths[i] <= natural[i], "never wider than its own width: {case}");
                            assert!(widths[i] >= natural[i].min(least), "at least the minimum: {case}");
                        }
                        let trimmed: Vec<i32> = widths.iter().zip(&natural).filter(|(w, n)| w < n).map(|(&w, _)| w).collect();
                        if let Some(&share) = trimmed.iter().min() {
                            assert!(trimmed.iter().all(|&w| w == share || w == share + 1), "trimmed pills share equally: {case}");
                            assert!(trimmed.windows(2).all(|w| w[0] >= w[1]), "the pixels left over go to the left: {case}");
                            assert!(widths.iter().all(|&w| w <= share + 1), "a pill that keeps its width fits in the share: {case}");
                            assert_eq!(row_width(&widths, gap), room, "the row fills the room exactly: {case}");
                        }
                        if let Some(before) = &before {
                            assert!(widths.len() >= before.len(), "more room, never fewer pills: {case}");
                            if widths.len() == before.len() {
                                assert!(widths.iter().zip(before).all(|(w, b)| w >= b), "more room, never narrower pills: {case}");
                            }
                        }
                        before = Some(widths);
                    }
                }
            }
        }
    }

    #[test]
    fn tiny_windows_drop_pills_from_the_right() {
        // The smallest pill at 100 %: padding 2 * 10 and 16 of text = 36; gap 6.
        let (t, b) = (300 - 8 - 26, 300 - 8);
        let texts = [50.0, 50.0, 50.0, 50.0];
        // Room 78 = two smallest pills and a gap: the third and fourth go.
        assert_eq!(pill_row(300, &texts, 16.0, 1.0, Some(78 + 16)), [(8, t, 44, b), (50, t, 86, b)]);
        // Room 77: one pill, at its own width of 70; room 60: one pill, trimmed to the room.
        assert_eq!(pill_row(300, &texts, 16.0, 1.0, Some(77 + 16)), [(8, t, 78, b)]);
        assert_eq!(pill_row(300, &texts, 16.0, 1.0, Some(60 + 16)), [(8, t, 68, b)]);
        // Room 35: not even one.
        assert!(pill_row(300, &texts, 16.0, 1.0, Some(35 + 16)).is_empty());
        assert!(pill_row(300, &texts, 16.0, 1.0, Some(0)).is_empty());
        assert!(pill_row(300, &texts, 16.0, 1.0, Some(-50)).is_empty());
        // A short text in a tiny window keeps its own small pill when it fits.
        assert_eq!(pill_row(300, &[5.0], 16.0, 1.0, Some(41)), [(8, t, 33, b)]);
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
        assert!(send(&url, "gate", 1));
        let req = received(rx);
        assert!(req.starts_with("POST /api/webhook/x HTTP/1.1\r\n"), "{req}");
        assert!(req.contains("Content-Type: application/json\r\n") && req.ends_with("\r\n\r\n{\"camera\": \"gate\", \"action\": 1}"), "{req}");

        let (url, rx) = serve_once("404 Not Found");
        assert!(!send(&url, "gate", 4));
        let req = received(rx);
        assert!(req.starts_with("POST /api/webhook/x HTTP/1.1\r\n") && req.ends_with("{\"camera\": \"gate\", \"action\": 4}"), "{req}");
    }
}
