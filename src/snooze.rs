//! Snoozing a camera from its window: for a while its events open no window. Shared by the
//! Windows and Linux front ends; the snoozes live in memory (a restart wakes every camera).

use crate::i18n::{self, t};
use std::time::{Duration, Instant};

/// The lengths offered under the snooze button, in minutes.
pub const CHOICES: [u64; 5] = [15, 30, 60, 120, 240];

/// One snoozed camera: its Frigate name, its title (for the menus) and when it wakes up.
#[derive(Clone, Debug, PartialEq)]
pub struct Snoozed {
    pub camera: String,
    pub title: String,
    pub until: Instant,
}

#[derive(Default)]
pub struct Snoozes(Vec<Snoozed>);

impl Snoozes {
    /// Snoozes `camera` for `minutes` from `now`; snoozing it again sets a new end.
    pub fn snooze(&mut self, camera: &str, title: &str, minutes: u64, now: Instant) {
        self.wake(camera);
        let until = now + Duration::from_secs(minutes * 60);
        self.0.push(Snoozed { camera: camera.to_string(), title: title.to_string(), until });
        self.0.sort_by_key(|s| s.title.to_lowercase());
    }

    /// Wakes `camera` up; `false` if it was not snoozed.
    pub fn wake(&mut self, camera: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|s| !s.camera.eq_ignore_ascii_case(camera));
        self.0.len() != before
    }

    /// `camera` is snoozed at `now`.
    pub fn is_snoozed(&self, camera: &str, now: Instant) -> bool {
        self.0.iter().any(|s| s.camera.eq_ignore_ascii_case(camera) && now < s.until)
    }

    /// Drops the snoozes that ran out by `now` and returns them.
    pub fn expire(&mut self, now: Instant) -> Vec<Snoozed> {
        let (gone, kept) = std::mem::take(&mut self.0).into_iter().partition(|s| now >= s.until);
        self.0 = kept;
        gone
    }

    /// The snoozed cameras, by title (some may have run out: see `expire`).
    pub fn list(&self) -> &[Snoozed] {
        &self.0
    }

    /// When the next snooze runs out.
    pub fn next_end(&self) -> Option<Instant> {
        self.0.iter().map(|s| s.until).min()
    }
}

/// A length from `CHOICES` for the list under the button: "15 min", "2 h".
pub fn choice_text(minutes: u64) -> String {
    if minutes >= 60 && minutes.is_multiple_of(60) {
        i18n::fmt(t().snooze_hours, &[&(minutes / 60)])
    } else {
        i18n::fmt(t().snooze_minutes, &[&minutes])
    }
}

/// The `z` key's choice in a caption: "z: 1 15 min · 2 30 min · …". Short, with the key that
/// opened it rather than a sentence: a terminal cuts a long caption at the end, and a window
/// there is often only some 30 columns wide.
#[cfg_attr(windows, allow(dead_code))]
pub fn choice_line() -> String {
    let lengths: Vec<String> = CHOICES.iter().enumerate().map(|(i, &m)| format!("{} {}", i + 1, choice_text(m))).collect();
    format!("z: {}", lengths.join(" · "))
}

/// Whole minutes left until `until`, rounded up (at least 1 while it has not run out).
pub fn minutes_left(until: Instant, now: Instant) -> u64 {
    until.saturating_duration_since(now).as_secs().div_ceil(60).max(1)
}

/// The menu entry that wakes a snoozed camera: "Wake Garden (52 min left)".
pub fn wake_text(s: &Snoozed, now: Instant) -> String {
    i18n::fmt(t().menu_wake_camera, &[&s.title, &minutes_left(s.until, now)])
}

/// The snooze icon in a round button with this centre and radius `rad`: a crescent moon as a
/// closed outline (y down), the outer edge first, then the inner one back.
pub fn moon_icon((cx, cy): (f32, f32), rad: f32) -> Vec<(f32, f32)> {
    let r = rad * 0.5;
    // The bite: a circle as big, this far up and to the right.
    let (alpha, off) = ((-50f32).to_radians(), r * 0.8);
    let (vx, vy) = (off * alpha.cos(), off * alpha.sin());
    // The crescent's weight lies away from the bite: move it back to the button's middle.
    let (cx, cy) = (cx + vx * 0.3, cy + vy * 0.3);
    let beta = (off / (2.0 * r)).acos();
    const STEPS: usize = 24;
    let mut out = Vec::with_capacity(2 * STEPS + 2);
    // The outer edge runs round the far side, from one tip to the other.
    for i in 0..=STEPS {
        let a = alpha + beta + (2.0 * std::f32::consts::PI - 2.0 * beta) * i as f32 / STEPS as f32;
        out.push((cx + r * a.cos(), cy + r * a.sin()));
    }
    // The inner edge: the bite's circle between the tips, back to the start.
    let back = alpha + std::f32::consts::PI;
    for i in 1..STEPS {
        let a = back + beta - 2.0 * beta * i as f32 / STEPS as f32;
        out.push((cx + vx + r * a.cos(), cy + vy + r * a.sin()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mins(m: u64) -> Duration {
        Duration::from_secs(m * 60)
    }

    #[test]
    fn a_snoozed_camera_wakes_up_on_time() {
        let now = Instant::now();
        let mut s = Snoozes::default();
        s.snooze("garden", "Garden", 60, now);
        assert!(s.is_snoozed("garden", now) && s.is_snoozed("Garden", now + mins(59)));
        assert!(!s.is_snoozed("gate", now));
        assert!(!s.is_snoozed("garden", now + mins(60)));
        assert_eq!(s.next_end(), Some(now + mins(60)));
        assert!(s.expire(now + mins(30)).is_empty());
        let gone = s.expire(now + mins(60));
        assert_eq!(gone.len(), 1);
        assert!(s.list().is_empty() && s.next_end().is_none());
    }

    #[test]
    fn snoozing_again_sets_a_new_end_and_waking_ends_it() {
        let now = Instant::now();
        let mut s = Snoozes::default();
        s.snooze("yard", "Yard", 240, now);
        s.snooze("garden", "Garden", 15, now);
        s.snooze("YARD", "Yard", 30, now);
        assert_eq!(s.list().iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), ["Garden", "Yard"]);
        assert_eq!(s.list()[1].until, now + mins(30));
        assert!(s.wake("Garden") && !s.wake("garden"));
        assert!(!s.is_snoozed("garden", now) && s.is_snoozed("yard", now));
    }

    #[test]
    fn texts_for_the_lengths_and_what_is_left() {
        i18n::set(i18n::Lang::En);
        let texts: Vec<String> = CHOICES.iter().map(|&m| choice_text(m)).collect();
        assert_eq!(texts, ["15 min", "30 min", "1 h", "2 h", "4 h"]);
        let now = Instant::now();
        assert_eq!(minutes_left(now + Duration::from_secs(61), now), 2);
        assert_eq!(minutes_left(now + mins(52), now), 52);
        assert_eq!(minutes_left(now, now), 1);
        let s = Snoozed { camera: "garden".into(), title: "Garden".into(), until: now + mins(52) };
        assert_eq!(wake_text(&s, now), "Wake Garden (52 min left)");
    }

    #[test]
    fn the_moon_stays_inside_its_button() {
        let (cx, cy, rad) = (50.0, 50.0, 12.0);
        let moon = moon_icon((cx, cy), rad);
        assert!(moon.len() > 20);
        for &(x, y) in &moon {
            assert!(((x - cx).powi(2) + (y - cy).powi(2)).sqrt() < rad * 0.8, "({x}, {y}) leaves the button");
        }
        // The outline closes: the inner edge ends next to where the outer one started.
        let (first, last) = (moon[0], moon[moon.len() - 1]);
        assert!(((first.0 - last.0).powi(2) + (first.1 - last.1).powi(2)).sqrt() < rad * 0.25);
        // The bite is up and to the right: the crescent's weight is down-left of the middle.
        let n = moon.len() as f32;
        let (mx, my) = (moon.iter().map(|p| p.0).sum::<f32>() / n, moon.iter().map(|p| p.1).sum::<f32>() / n);
        assert!(mx < cx + 1.0 && my > cy - 1.0, "weight at ({mx}, {my})");
    }
}
