//! The action buttons of the Linux windows: what the settings give a window, sending a request,
//! and the `a` key for outputs without a mouse (terminal, framebuffer, tmux popups).

use super::Ev;
use crate::action::{self, Buttons, Look, Shown};
use crate::config::{Config, ACTIONS};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

/// How long the choice among several actions stays open after `a`, and after a digit armed one
/// (as long as the arming lasts, so that the same digit again confirms it).
const CHOICE_FOR: Duration = Duration::from_secs(3);

/// The camera's actions with a URL as a window shows them: (number, label), in number order.
pub fn labels(cfg: &Config, camera: &str) -> Vec<(usize, String)> {
    cfg.actions_for(camera).into_iter().map(|a| (a.n, action::label_or_default(a.label))).collect()
}

/// The window's buttons follow the camera's settings, number by number (see `action::refresh`).
pub fn refresh(buttons: &mut Buttons, cfg: &Config, camera: &str) {
    let mut confirm = [None; ACTIONS];
    for a in cfg.actions_for(camera) {
        if let Some(c) = confirm.get_mut(a.n.wrapping_sub(1)) {
            *c = Some(a.confirm);
        }
    }
    action::refresh(buttons, &confirm);
}

/// The numbers of the actions the window has a button for.
pub fn numbers(buttons: &Buttons) -> Vec<usize> {
    (1..=ACTIONS).filter(|&n| buttons[n - 1].is_some()).collect()
}

/// Presses action `n` of a window showing `camera`: arms it or sends its request, whose result
/// comes back as `Ev::ActionDone`. Returns the action's look afterwards (`None` = no such
/// action). A request that cannot start counts as failed at once.
pub fn press(buttons: &mut Buttons, n: usize, cfg: &Config, camera: &str, tx: &Sender<Ev>, now: Instant) -> Option<Look> {
    if let Some(ticket) = action::press(buttons, n, now) {
        // Only here does the URL leave the settings, straight to the request's thread.
        let url = cfg.action_url(camera, n).map(str::to_string);
        let tx = tx.clone();
        let done = move |ok: bool| {
            let _ = tx.send(Ev::ActionDone(ticket, ok));
        };
        if !url.is_some_and(|url| action::spawn(url, camera.to_string(), n, done)) {
            if let Some(b) = buttons[n - 1].as_mut() {
                b.finished(ticket, false, now);
            }
        }
    }
    Some(buttons.get(n.checked_sub(1)?)?.as_ref()?.look(now))
}

/// The caption with the window's actions, for outputs without buttons: while the choice is open
/// every action with its number ("1 Gate · 2 Light?"), otherwise only the ones that are not idle
/// ("Gate?", "Gate …", "Gate ✓", "Gate ✗"). Nothing is added while all are idle (the status
/// line and `--help` name the `a` key). They come first: a terminal cuts a caption longer than
/// its window at the end, and a window there is often only some 30 columns wide.
pub fn with_actions(caption: &str, shown: &[Shown], choosing: bool) -> String {
    let parts = shown.iter().filter_map(|s| match (choosing, s.look) {
        (true, _) => Some(format!("{} {}", s.n, s.text)),
        (false, Look::Idle) => None,
        (false, _) => Some(s.text.clone()),
    });
    let all: Vec<String> = parts.chain(std::iter::once(caption.to_string()).filter(|c| !c.is_empty())).collect();
    all.join(" · ")
}

/// The open choice: for which window, its action numbers, until when.
#[derive(Clone, Debug, PartialEq)]
struct Open {
    win: u64,
    numbers: Vec<usize>,
    until: Instant,
}

/// The `a` key without a mouse. With one action `a` presses it (with confirmation: `a`, `a`).
/// With several `a` opens a choice for 3 s: the caption lists the actions with their numbers,
/// and a listed digit presses that action instead of showing a camera.
#[derive(Default)]
pub struct Picker {
    open: Option<Open>,
}

impl Picker {
    /// The choice is open for window `win`.
    pub fn choosing(&self, win: u64, now: Instant) -> bool {
        self.open.as_ref().is_some_and(|o| o.win == win && now < o.until)
    }

    /// `a` for window `win` with these action numbers (the newest window that has actions).
    /// `Some(n)` = press action `n` now; with several, the choice opens (or starts over).
    pub fn key_a(&mut self, win: u64, numbers: &[usize], now: Instant) -> Option<usize> {
        match numbers {
            [] => None,
            [n] => {
                self.open = None;
                Some(*n)
            }
            _ => {
                self.open = Some(Open { win, numbers: numbers.to_vec(), until: now + CHOICE_FOR });
                None
            }
        }
    }

    /// Digit `d`: `Some((window, n))` = press that window's action `n`, then report with
    /// `pressed`. `None` = the digit does what it does without the choice (shows a camera):
    /// no choice is open, or it does not list `d`.
    pub fn digit(&self, d: usize, now: Instant) -> Option<(u64, usize)> {
        self.open.as_ref().filter(|o| now < o.until && o.numbers.contains(&d)).map(|o| (o.win, d))
    }

    /// After a digit pressed an action: an armed one keeps the choice open as long as it stays
    /// armed (the same digit confirms it); anything else ends the choice.
    pub fn pressed(&mut self, armed: bool, now: Instant) {
        match (&mut self.open, armed) {
            (Some(o), true) => o.until = now + CHOICE_FOR,
            _ => self.open = None,
        }
    }

    /// Ends the choice (its window went away).
    pub fn close(&mut self) {
        self.open = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    fn shown(n: usize, label: &str, look: Look) -> Shown {
        Shown { n, text: action::text(label, look), look }
    }

    #[test]
    fn the_caption_shows_busy_actions_or_the_choice() {
        i18n::set(i18n::Lang::En);
        let cap = "Gate · person";
        let idle = [shown(1, "Gate", Look::Idle), shown(2, "Light", Look::Idle)];
        assert_eq!(with_actions(cap, &idle, false), cap, "nothing while all are idle");
        assert_eq!(with_actions(cap, &idle, true), "1 Gate · 2 Light · Gate · person");
        for (look, want) in [(Look::Armed, "Gate?"), (Look::Sending, "Gate …"), (Look::Done, "Gate ✓"), (Look::Failed, "Gate ✗")] {
            let s = [shown(1, "Gate", look), shown(2, "Light", Look::Idle)];
            assert_eq!(with_actions(cap, &s, false), format!("{want} · {cap}"), "{look:?}");
            assert_eq!(with_actions(cap, &s, true), format!("1 {want} · 2 Light · {cap}"), "{look:?}");
        }
        let both = [shown(1, "Gate", Look::Sending), shown(3, "Light", Look::Failed)];
        assert_eq!(with_actions(cap, &both, false), "Gate … · Light ✗ · Gate · person");
        // Without a caption (no overlay) only the actions.
        assert_eq!(with_actions("", &both, false), "Gate … · Light ✗");
        assert_eq!(with_actions("", &idle, false), "");
        assert_eq!(with_actions(cap, &[], true), cap);
    }

    #[test]
    fn one_action_is_pressed_at_once() {
        let t = Instant::now();
        let mut p = Picker::default();
        assert_eq!(p.key_a(7, &[2], t), Some(2));
        assert!(!p.choosing(7, t));
        assert_eq!(p.digit(2, t), None, "digits keep showing cameras");
        assert_eq!(p.key_a(7, &[], t), None);
    }

    #[test]
    fn several_actions_open_a_choice_for_three_seconds() {
        let t = Instant::now();
        let mut p = Picker::default();
        assert_eq!(p.key_a(7, &[1, 3], t), None);
        assert!(p.choosing(7, t) && !p.choosing(8, t));
        assert_eq!(p.digit(2, t), None, "not listed: shows camera 2");
        assert_eq!(p.digit(5, t), None);
        assert_eq!(p.digit(3, t + ms(2999)), Some((7, 3)));
        assert_eq!(p.digit(3, t + ms(3000)), None, "expired");
        assert!(!p.choosing(7, t + ms(3000)));
    }

    #[test]
    fn an_armed_choice_waits_for_the_same_digit() {
        let t = Instant::now();
        let mut p = Picker::default();
        p.key_a(7, &[1, 2], t);
        assert_eq!(p.digit(1, t + ms(2500)), Some((7, 1)));
        p.pressed(true, t + ms(2500));
        assert!(p.choosing(7, t + ms(5000)), "open as long as the arming lasts");
        assert_eq!(p.digit(1, t + ms(5000)), Some((7, 1)), "the same digit confirms");
        p.pressed(false, t + ms(5000));
        assert!(!p.choosing(7, t + ms(5000)), "sent: the choice ends");
        assert_eq!(p.digit(1, t + ms(5100)), None);
    }

    #[test]
    fn a_again_starts_the_choice_over() {
        let t = Instant::now();
        let mut p = Picker::default();
        p.key_a(7, &[1, 2], t);
        p.key_a(9, &[2, 4], t + ms(2000));
        assert!(!p.choosing(7, t + ms(2000)) && p.choosing(9, t + ms(4500)));
        assert_eq!(p.digit(4, t + ms(4500)), Some((9, 4)));
        p.close();
        assert_eq!(p.digit(4, t + ms(4500)), None);
    }

    #[test]
    fn buttons_follow_the_settings() {
        let mut cfg = Config::from_ini(&crate::config::IniDoc::parse(""));
        cfg.set_action("gate", 1, Some("http://h/a".into()), Some("Gate".into()), true);
        cfg.set_action("gate", 3, Some("http://h/b".into()), None, false);
        let mut b: Buttons = Default::default();
        refresh(&mut b, &cfg, "gate");
        assert_eq!(numbers(&b), [1, 3]);
        assert_eq!(labels(&cfg, "gate"), [(1, "Gate".to_string()), (3, i18n::t().action_default.to_string())]);
        assert!(b[0].as_ref().unwrap().confirm() && !b[2].as_ref().unwrap().confirm());
        refresh(&mut b, &cfg, "garden");
        assert!(numbers(&b).is_empty());
    }

    #[test]
    fn a_press_sends_and_the_result_comes_back_as_an_event() {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/api/webhook/x", l.local_addr().unwrap().port());
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let (mut got, mut buf) = (Vec::new(), [0u8; 4096]);
            // The head, then the JSON body, which ends the request.
            while !got.ends_with(b"}") {
                match s.read(&mut buf).unwrap() {
                    0 => break,
                    n => got.extend_from_slice(&buf[..n]),
                }
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            String::from_utf8_lossy(&got).to_string()
        });
        let mut cfg = Config::from_ini(&crate::config::IniDoc::parse(""));
        cfg.set_action("gate", 2, Some(url), None, false);
        let mut b: Buttons = Default::default();
        refresh(&mut b, &cfg, "gate");
        let (tx, rx) = std::sync::mpsc::channel();
        assert_eq!(press(&mut b, 2, &cfg, "gate", &tx, Instant::now()), Some(Look::Sending));
        let Ok(Ev::ActionDone(ticket, ok)) = rx.recv_timeout(Duration::from_secs(15)) else { panic!("no result") };
        assert!(ok, "HTTP 200");
        let now = Instant::now();
        assert!(b[1].as_mut().unwrap().finished(ticket, ok, now), "the button that sent it takes the result");
        assert_eq!(b[1].as_ref().unwrap().look(now), Look::Done);
        let req = server.join().unwrap();
        assert!(req.starts_with("POST /api/webhook/x ") && req.ends_with("{\"camera\": \"gate\", \"action\": 2}"), "{req}");
    }

    #[test]
    fn a_press_without_a_url_fails_at_once() {
        let cfg = Config::from_ini(&crate::config::IniDoc::parse(""));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut b: Buttons = Default::default();
        b[1] = Some(action::Button::new(false));
        let t = Instant::now();
        assert_eq!(press(&mut b, 2, &cfg, "gate", &tx, t), Some(Look::Failed), "the settings lost the URL meanwhile");
        assert!(rx.try_recv().is_err(), "nothing was sent");
        assert_eq!(press(&mut b, 1, &cfg, "gate", &tx, t), None, "no such action");
        assert_eq!(press(&mut b, 9, &cfg, "gate", &tx, t), None);
    }
}
