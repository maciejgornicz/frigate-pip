//! The action buttons of the Linux windows: what the settings give a window, sending a request,
//! and the `a` key for outputs without a mouse (terminal, framebuffer, tmux popups). The `z` key
//! (snooze the window's camera) opens a choice of lengths the same way.

use super::Ev;
use crate::action::{self, Buttons, Look, Shown};
use crate::config::{Config, ACTIONS};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

/// How long the choice among several actions stays open after `a`, and after a digit armed one:
/// as long as the arming lasts, so that the same digit again confirms it.
const CHOICE_FOR: Duration = action::ARMED_FOR;
/// A key byte this soon after the previous one of the same key is the keyboard's auto-repeat.
const REPEAT_GAP: Duration = Duration::from_millis(200);
/// A key press counts only when no byte of the same key follows this soon. Auto-repeat starts
/// only after a delay (often 250–660 ms), so its first byte looks like a second press; the
/// bytes after it come every 25–100 ms and show that the key is held. This needs a repeat
/// interval under 150 ms (what desktops and the Linux console use); a double tap within 150 ms
/// counts as nothing.
const SETTLE: Duration = Duration::from_millis(150);
/// A digit that pressed an action is still swallowed this long after its last byte when the
/// choice has ended meanwhile: the first auto-repeat byte comes up to some 660 ms after the
/// press, and must not show a camera.
const HELD_AFTER: Duration = Duration::from_secs(1);

/// The camera's actions with a URL as a window shows them: (number, label), in number order.
pub fn labels(cfg: &Config, camera: &str) -> Vec<(usize, String)> {
    action::labels(&cfg.actions_for(camera))
}

/// The window's buttons follow the camera's settings, number by number (see `action::refresh`).
pub fn refresh(buttons: &mut Buttons, cfg: &Config, camera: &str) {
    action::refresh(buttons, &action::confirms(&cfg.actions_for(camera)));
}

/// The numbers of the actions the window has a button for.
pub fn numbers(buttons: &Buttons) -> Vec<usize> {
    (1..=ACTIONS).filter(|&n| buttons[n - 1].is_some()).collect()
}

/// One pass of an event loop: handles `first` (if any) and then every event already queued, and
/// returns whether to go on and the moment the queue was looked at. Key bytes carry the time
/// the reader thread got them, so a slow pass cannot make a held key's repeat look like a new
/// press; settling presses at that moment, after the queue is empty, means every byte sent
/// before it has been seen (see `Picker::next_settled`).
pub fn drain<E>(first: Option<E>, rx: &Receiver<E>, mut handle: impl FnMut(E) -> bool) -> (bool, Instant) {
    if let Some(e) = first {
        if !handle(e) {
            return (false, Instant::now());
        }
    }
    let t = Instant::now();
    while let Ok(e) = rx.try_recv() {
        if !handle(e) {
            return (false, t);
        }
    }
    (true, t)
}

/// The window the `a` key is for, from (id, has actions, on the screen) of every window: the
/// newest one on the screen that has actions. Ids grow with every new window, while the list
/// order does not tell the age (a replaced window takes the old one's place).
pub fn a_target(wins: &[(u64, bool, bool)]) -> Option<u64> {
    wins.iter().filter(|(_, actions, shown)| *actions && *shown).map(|(id, _, _)| *id).max()
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
        action::send_or_fail(buttons, ticket, url, camera.to_string(), n, done, now);
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

/// The caption with the open choice, for outputs without buttons (see `with_actions`).
pub fn with_choice(caption: &str, shown: &[Shown], choice: Choice) -> String {
    let caption = with_actions(caption, shown, choice == Choice::Actions);
    match choice {
        Choice::Snooze => [crate::snooze::choice_line(), caption].into_iter().filter(|c| !c.is_empty()).collect::<Vec<_>>().join(" · "),
        _ => caption,
    }
}

/// What a window's open choice is for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choice {
    None,
    /// The `a` key's: the actions by number.
    Actions,
    /// The `z` key's: the snooze lengths by number (`snooze::CHOICES`).
    Snooze,
}

/// The open choice: for which window, its numbers, until when, and what for.
#[derive(Clone, Debug, PartialEq)]
struct Open {
    win: u64,
    numbers: Vec<usize>,
    until: Instant,
    snooze: bool,
}

/// What a key press that counts does (see `Picker::next_settled`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    /// `a`: see `Picker::key_a`.
    A,
    /// A digit during the choice: press action `n` of window `win`, then `Picker::pressed`.
    Digit(u64, usize),
    /// `z`: see `Picker::key_z`.
    Z,
    /// A digit during the `z` key's choice: snooze the camera of window `win` this many minutes.
    Snooze(u64, u64),
}

/// A press waiting for `SETTLE`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pending {
    Key(Key),
    /// A digit typed while an `a` (or `z`) waited: it goes to the choice that key opens, or nowhere.
    AfterA(usize),
}

/// The `a` key without a mouse. With one action `a` presses it (with confirmation: `a`, `a`).
/// With several `a` opens a choice for 3 s: the caption lists the actions with their numbers,
/// and a listed digit presses that action instead of showing a camera.
///
/// A terminal sends no key-up, so a held key would arm and then confirm an action by itself.
/// Key bytes therefore go through `key`, with the time the reader thread got them: a byte less
/// than `REPEAT_GAP` after the previous one of a key the picker took is swallowed, and a press
/// counts (`next_settled`) only when no byte of that key follows within `SETTLE`.
#[derive(Default)]
pub struct Picker {
    open: Option<Open>,
    /// When each key the picker took last sent a byte (taken or swallowed).
    last: HashMap<u8, Instant>,
    /// Presses waiting for `SETTLE`, oldest first: (key, when, what they do).
    pending: Vec<(u8, Instant, Pending)>,
}

impl Picker {
    /// A key byte the reader thread got `at`. `false` = not for the actions (a digit then shows a
    /// camera at once): neither `a` nor a digit the open choice lists, typed while an `a` waits
    /// or still held from a press the picker took. `true` = taken; what it does comes from
    /// `next_settled`.
    pub fn key(&mut self, byte: u8, at: Instant) -> bool {
        let k = byte.to_ascii_lowercase();
        if k != b'a' && k != b'z' && !(b'1'..=b'9').contains(&k) {
            return false;
        }
        let since = self.last.get(&k).map(|t| at.saturating_duration_since(*t));
        // A repeat of a key the picker took is swallowed before anything else: a held digit
        // must not show a camera once its action has ended the choice.
        let what = if since.is_some_and(|d| d < REPEAT_GAP) {
            None
        } else if k == b'a' {
            Some(Pending::Key(Key::A))
        } else if k == b'z' {
            Some(Pending::Key(Key::Z))
        } else {
            let d = (k - b'0') as usize;
            match self.chosen(d, at) {
                Some(key) => Some(Pending::Key(key)),
                None if self.pending.iter().any(|p| p.0 == b'a' || p.0 == b'z') => Some(Pending::AfterA(d)),
                // Its first auto-repeat byte, after the choice ended.
                None if since.is_some_and(|d| d < HELD_AFTER) => None,
                None => return false,
            }
        };
        self.last.insert(k, at);
        // A byte right after a waiting press shows that the key is held: neither counts.
        let waiting = self.pending.len();
        self.pending.retain(|(pk, t, _)| *pk != k || at.saturating_duration_since(*t) >= SETTLE);
        if let (Some(what), true) = (what, self.pending.len() == waiting) {
            self.pending.push((k, at, what));
        }
        true
    }

    /// The oldest press that counts by `t` (taken right after `drain`), if any. Call it until
    /// `None`, handling each one before the next: a digit typed while an `a` waited goes to the
    /// choice that `a` opened, so the `a` must be handled first.
    pub fn next_settled(&mut self, t: Instant) -> Option<Key> {
        loop {
            let &(_, at, what) = self.pending.first()?;
            if t.saturating_duration_since(at) < SETTLE {
                return None;
            }
            self.pending.remove(0);
            match what {
                Pending::Key(key) => return Some(key),
                Pending::AfterA(d) => {
                    if let Some(key) = self.chosen(d, t) {
                        return Some(key);
                    }
                }
            }
        }
    }

    /// How long until the next waiting press counts, if one waits.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        self.pending.iter().map(|(_, at, _)| (*at + SETTLE).saturating_duration_since(now)).min()
    }

    /// A choice is open for window `win`.
    pub fn choosing(&self, win: u64, now: Instant) -> bool {
        self.choice(win, now) != Choice::None
    }

    /// The choice open for window `win`, if any.
    pub fn choice(&self, win: u64, now: Instant) -> Choice {
        match &self.open {
            Some(o) if o.win == win && now < o.until => {
                if o.snooze {
                    Choice::Snooze
                } else {
                    Choice::Actions
                }
            }
            _ => Choice::None,
        }
    }

    /// `z` for window `win` (the newest one on the screen): the choice of snooze lengths opens
    /// (or starts over, also in place of the `a` key's).
    pub fn key_z(&mut self, win: u64, now: Instant) {
        let numbers = (1..=crate::snooze::CHOICES.len()).collect();
        self.open = Some(Open { win, numbers, until: now + CHOICE_FOR, snooze: true });
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
                self.open = Some(Open { win, numbers: numbers.to_vec(), until: now + CHOICE_FOR, snooze: false });
                None
            }
        }
    }

    /// Digit `d`: `Some((window, n))` = the open choice lists it (press that window's action
    /// `n`). `None` = the digit does what it does without the choice (shows a camera): no choice
    /// is open, or it does not list `d`.
    pub fn digit(&self, d: usize, now: Instant) -> Option<(u64, usize)> {
        self.open.as_ref().filter(|o| now < o.until && o.numbers.contains(&d)).map(|o| (o.win, d))
    }

    /// What digit `d` does in the open choice: an action, or a snooze length (see `digit`).
    fn chosen(&self, d: usize, now: Instant) -> Option<Key> {
        let (win, n) = self.digit(d, now)?;
        match self.open.as_ref()?.snooze {
            true => Some(Key::Snooze(win, *crate::snooze::CHOICES.get(n - 1)?)),
            false => Some(Key::Digit(win, n)),
        }
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
    fn z_opens_the_snooze_lengths_and_a_digit_picks_one() {
        i18n::set(i18n::Lang::En);
        let t = Instant::now();
        let mut p = Picker::default();
        assert!(p.key(b'z', t));
        assert_eq!(p.next_settled(t + ms(200)), Some(Key::Z));
        p.key_z(7, t + ms(200));
        assert_eq!(p.choice(7, t + ms(300)), Choice::Snooze);
        assert!(p.choosing(7, t + ms(300)));
        // 3 = the third length (1 h); 9 lists nothing and shows camera 9.
        assert!(!p.key(b'9', t + ms(400)));
        assert!(p.key(b'3', t + ms(1000)));
        assert_eq!(p.next_settled(t + ms(1200)), Some(Key::Snooze(7, 60)));
        // `a` replaces the choice with its own.
        p.key_a(7, &[1, 2], t + ms(1300));
        assert_eq!(p.choice(7, t + ms(1300)), Choice::Actions);
        let cap = "Garden · person";
        assert_eq!(with_choice(cap, &[], Choice::Snooze), "z: 1 15 min · 2 30 min · 3 1 h · 4 2 h · 5 4 h · Garden · person");
        assert_eq!(with_choice("", &[], Choice::Snooze), "z: 1 15 min · 2 30 min · 3 1 h · 4 2 h · 5 4 h");
        assert_eq!(with_choice(cap, &[], Choice::None), cap);
    }

    #[test]
    fn a_digit_typed_right_after_z_goes_to_its_choice() {
        let t = Instant::now();
        let mut p = Picker::default();
        assert!(p.key(b'z', t) && p.key(b'2', t + ms(60)), "not camera 2");
        assert_eq!(p.next_settled(t + ms(200)), Some(Key::Z));
        p.key_z(4, t + ms(200));
        assert_eq!(p.next_settled(t + ms(250)), Some(Key::Snooze(4, 30)));
    }

    #[test]
    fn a_is_for_the_newest_window_with_actions() {
        // Window 9 replaced the first one and sits in its place: still the newest.
        assert_eq!(a_target(&[(9, true, true), (4, true, true), (7, false, true)]), Some(9));
        assert_eq!(a_target(&[(9, true, false), (4, true, true)]), Some(4), "not on the screen");
        assert_eq!(a_target(&[(9, false, true), (4, false, true)]), None);
        assert_eq!(a_target(&[]), None);
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

    /// Feeds key bytes at the given milliseconds to a picker for one window (id 1) with one
    /// button per number in `confirm`, running settled presses every 10 ms like the main loop.
    /// Returns how many requests were sent and the looks at the end.
    fn type_keys(confirm: &[bool], bytes: &[(u64, u8)], until: u64) -> (usize, Vec<Look>) {
        let t = Instant::now();
        let mut p = Picker::default();
        let mut b: Buttons = Default::default();
        for (i, c) in confirm.iter().enumerate() {
            b[i] = Some(action::Button::new(*c));
        }
        let mut sent = 0;
        let mut act = |b: &mut Buttons, p: &mut Picker, n: usize, at: Instant, digit: bool| {
            sent += usize::from(action::press(b, n, at).is_some());
            if digit {
                p.pressed(b[n - 1].as_ref().is_some_and(|x| x.look(at) == Look::Armed), at);
            }
        };
        let mut next = bytes.iter().peekable();
        for now_ms in 0..=until {
            let now = t + ms(now_ms);
            while let Some(&&(at, byte)) = next.peek() {
                if at != now_ms {
                    break;
                }
                p.key(byte, now);
                next.next();
            }
            if now_ms % 10 == 0 {
                while let Some(k) = p.next_settled(now) {
                    match k {
                        Key::A => {
                            if let Some(n) = p.key_a(1, &numbers(&b), now) {
                                act(&mut b, &mut p, n, now, false);
                            }
                        }
                        Key::Digit(_, n) => act(&mut b, &mut p, n, now, true),
                        Key::Z | Key::Snooze(..) => panic!("no z typed here"),
                    }
                }
            }
        }
        let end = t + ms(until);
        (sent, b.iter().flatten().map(|x| x.look(end)).collect())
    }

    /// Bytes of `key` from `from` to `to` ms, one every `every` ms.
    fn held(key: u8, from: u64, to: u64, every: u64) -> Vec<(u64, u8)> {
        (from..=to).step_by(every as usize).map(|at| (at, key)).collect()
    }

    #[test]
    fn a_held_key_never_confirms() {
        // Bytes every 40 ms for a second: nothing at all, not even arming.
        assert_eq!(type_keys(&[true], &held(b'a', 0, 1000, 40), 1500), (0, vec![Look::Idle]));
        // A real keyboard: the press, then auto-repeat after 500 ms, every 33 ms. The press arms;
        // the first repeat, 500 ms later, must not count as the confirming press.
        let mut bytes = vec![(0, b'a')];
        bytes.extend(held(b'a', 500, 1500, 33));
        assert_eq!(type_keys(&[true], &bytes, 1600), (0, vec![Look::Armed]));
        // The same with a 250 ms delay and 10 bytes a second (the Linux console).
        let mut bytes = vec![(0, b'a')];
        bytes.extend(held(b'a', 250, 2000, 92));
        assert_eq!(type_keys(&[true], &bytes, 2100).0, 0);
        // Upper case is the same key.
        let mut bytes = vec![(0, b'A')];
        bytes.extend(held(b'A', 500, 1500, 33));
        assert_eq!(type_keys(&[true], &bytes, 1600).0, 0);
    }

    #[test]
    fn two_deliberate_presses_confirm() {
        assert_eq!(type_keys(&[true], &[(0, b'a'), (600, b'a')], 1000), (1, vec![Look::Sending]));
        // Without confirmation one press sends, and holding the key sends nothing more.
        let mut bytes = vec![(0, b'a')];
        bytes.extend(held(b'a', 500, 1500, 33));
        assert_eq!(type_keys(&[false], &bytes, 1600).0, 1);
    }

    #[test]
    fn a_held_digit_never_confirms_in_the_choice() {
        let mut bytes = vec![(0, b'a'), (1000, b'1')];
        bytes.extend(held(b'1', 1500, 2500, 33));
        assert_eq!(type_keys(&[true, true], &bytes, 2600), (0, vec![Look::Armed, Look::Idle]));
        // Deliberately: a, 1 arms, 1 again confirms.
        assert_eq!(type_keys(&[true, true], &[(0, b'a'), (1000, b'1'), (1700, b'1')], 2000).0, 1);
    }

    #[test]
    fn digits_outside_the_choice_are_not_taken() {
        let t = Instant::now();
        let mut p = Picker::default();
        assert!(!p.key(b'1', t), "no choice: shows camera 1");
        assert!(p.key(b'a', t));
        assert_eq!(p.next_settled(t + ms(150)), Some(Key::A));
        p.key_a(4, &[1, 3], t + ms(150));
        assert!(!p.key(b'2', t + ms(200)), "not listed: shows camera 2");
        assert!(!p.key(b'7', t + ms(200)));
        assert!(!p.key(b'x', t + ms(200)), "not a key for the actions");
        assert!(p.key(b'3', t + ms(200)));
        assert_eq!(p.wait(t + ms(250)), Some(ms(100)));
        assert_eq!(p.next_settled(t + ms(349)), None);
        assert_eq!(p.next_settled(t + ms(350)), Some(Key::Digit(4, 3)));
        assert_eq!(p.next_settled(t + ms(350)), None);
        assert!(!p.key(b'1', t + ms(3200)), "the choice expired: camera 1 again");
    }

    #[test]
    fn a_stalled_loop_still_sees_the_repeat() {
        let t = Instant::now();
        let mut p = Picker::default();
        p.key(b'a', t);
        assert_eq!(p.next_settled(t + ms(150)), Some(Key::A));
        p.key_a(1, &[1], t + ms(150));
        // Auto-repeat from 500 ms; the loop was busy and handles both bytes only at 733 ms, with
        // the times the reader thread gave them.
        assert!(p.key(b'a', t + ms(500)) && p.key(b'a', t + ms(533)));
        assert_eq!(p.next_settled(t + ms(733)), None, "the repeat cancels the byte before it");
    }

    #[test]
    fn a_pass_reads_the_whole_queue_before_presses_count() {
        let base = Instant::now().checked_sub(ms(2000)).unwrap();
        let mut p = Picker::default();
        p.key(b'a', base);
        assert_eq!(p.next_settled(base + ms(150)), Some(Key::A));
        // A notice, then the first repeat byte and the one after it, all waiting in the queue
        // while the loop was slow. The pass handles the notice first.
        let (tx, rx) = std::sync::mpsc::channel();
        for e in [None, Some(base + ms(500)), Some(base + ms(533))] {
            tx.send(e).unwrap();
        }
        let first = Some(rx.recv().unwrap());
        let (go, t) = drain(first, &rx, |e: Option<Instant>| {
            if let Some(at) = e {
                p.key(b'a', at);
            }
            true
        });
        assert!(go);
        assert_eq!(p.next_settled(t), None, "both repeat bytes were seen before settling");
        // A handler that asks to stop ends the pass.
        tx.send(None).unwrap();
        assert!(!drain(None, &rx, |_| false).0);
    }

    #[test]
    fn a_held_digit_never_shows_a_camera_after_its_action() {
        let t = Instant::now();
        let mut p = Picker::default();
        p.key(b'a', t);
        assert_eq!(p.next_settled(t + ms(150)), Some(Key::A));
        p.key_a(1, &[1, 2], t + ms(150));
        assert!(p.key(b'1', t + ms(1000)));
        assert_eq!(p.next_settled(t + ms(1150)), Some(Key::Digit(1, 1)));
        // It sent: the choice ends.
        p.pressed(false, t + ms(1150));
        // Auto-repeat from 500 ms after the press, every 33 ms: all swallowed, nothing counts.
        for at in (1500..=2500).step_by(33) {
            assert!(p.key(b'1', t + ms(at)), "byte at {at} ms shows no camera");
        }
        assert_eq!(p.next_settled(t + ms(3000)), None);
        assert!(!p.key(b'1', t + ms(4000)), "let go long ago: camera 1 again");
    }

    #[test]
    fn a_digit_typed_while_a_waits_goes_to_the_choice() {
        let t = Instant::now();
        let mut p = Picker::default();
        assert!(p.key(b'a', t) && p.key(b'2', t + ms(60)), "not camera 2");
        assert_eq!(p.next_settled(t + ms(250)), Some(Key::A));
        p.key_a(1, &[1, 2], t + ms(250));
        assert_eq!(p.next_settled(t + ms(250)), Some(Key::Digit(1, 2)), "the choice `a` opened takes it");
        // With one action `a` opens no choice: the digit goes nowhere.
        let mut p = Picker::default();
        assert!(p.key(b'a', t) && p.key(b'2', t + ms(60)));
        assert_eq!(p.next_settled(t + ms(250)), Some(Key::A));
        assert_eq!(p.key_a(1, &[1], t + ms(250)), Some(1));
        assert_eq!(p.next_settled(t + ms(250)), None);
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
