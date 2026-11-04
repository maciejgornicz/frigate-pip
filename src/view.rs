//! Decisions about preview windows shared by the Windows and Linux front ends:
//! where a new window goes, its caption and which stream it plays.

use crate::config::{Config, StreamPref};
use crate::filter::Detection;
use crate::frigate::CameraInfo;
use crate::i18n;

#[derive(Debug, PartialEq)]
pub enum Slot {
    New,
    /// Replace the window at this index (in the order of the given states).
    Replace(usize),
    Skip,
}

/// Where to show a new event. `states`: (closing, pinned), oldest first.
pub fn choose_slot(states: &[(bool, bool)], max: usize, replace: bool) -> Slot {
    if states.iter().filter(|s| !s.0).count() < max {
        return Slot::New;
    }
    if !replace {
        return Slot::Skip;
    }
    match states.iter().position(|s| !s.0 && !s.1) {
        Some(i) => Slot::Replace(i),
        None => Slot::Skip,
    }
}

/// Caption on the picture: camera, object, sub label, confidence and time.
pub fn caption(title: &str, d: &Detection) -> String {
    let mut s = format!("{title} · {}", i18n::label(&d.label));
    if let Some(sl) = &d.sub_label {
        s.push_str(&format!(" ({sl})"));
    }
    if d.score > 0.0 {
        s.push_str(&format!(" {:.0}%", d.score * 100.0));
    }
    s.push_str(&format!(" · {}", crate::util::local_time(false)));
    s
}

/// go2rtc stream for a window `phys_width` pixels wide.
pub fn choose_stream(cfg: &Config, info: Option<&CameraInfo>, camera: &str, phys_width: i32) -> Option<String> {
    let Some(c) = info else { return Some(camera.to_string()) };
    let want_main = match cfg.stream {
        StreamPref::Main => true,
        StreamPref::Sub => false,
        StreamPref::Auto => phys_width > cfg.auto_main_width,
    };
    if want_main {
        c.main.clone().or_else(|| c.sub.clone())
    } else {
        c.sub.clone().or_else(|| c.main.clone())
    }
}

/// List entries: cameras (name, title), objects and zones — those known to Frigate
/// plus those from the settings (so nothing is lost while Frigate is unreachable).
pub fn choices(cams: &[CameraInfo], cfg: &Config) -> (Vec<(String, String)>, Vec<String>, Vec<String>) {
    let mut cameras: Vec<(String, String)> = cams.iter().map(|c| (c.name.clone(), c.title.clone())).collect();
    for c in &cfg.cameras {
        if !cameras.iter().any(|(n, _)| n.eq_ignore_ascii_case(c)) {
            cameras.push((c.clone(), c.clone()));
        }
    }
    let mut known: Vec<String> = cams.iter().flat_map(|c| c.labels.iter().cloned()).collect();
    if known.is_empty() {
        known = vec!["person".into(), "car".into()];
    }
    let labels = union(known.iter(), &cfg.labels);
    let zones = union(cams.iter().flat_map(|c| c.zones.iter()), &cfg.zones);
    (cameras, labels, zones)
}

fn union<'a>(base: impl Iterator<Item = &'a String>, extra: &'a [String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in base.chain(extra.iter()) {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(s)) {
            out.push(s.clone());
        }
    }
    out
}

/// The caption of a window with its state: pinned, zoomed (the same on Windows and Linux).
pub fn state_caption(caption: &str, pinned: bool, zoom: Zoom) -> String {
    let mut text = caption.to_string();
    if pinned {
        text = format!("{text} · {}", i18n::t().overlay_pinned);
    }
    if let Some(z) = zoom.label() {
        text = format!("{text} · {z}");
    }
    text
}

/// Zoom of a preview window (Ctrl + mouse wheel): the part of the picture that is shown —
/// its left and top edge and its size, as fractions of the whole picture. The part keeps
/// the picture's aspect ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zoom {
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

impl Zoom {
    pub const FULL: Zoom = Zoom { x: 0.0, y: 0.0, size: 1.0 };
    /// The largest magnification.
    pub const MAX: f32 = 8.0;
    /// Magnification per wheel notch.
    const STEP: f32 = 1.25;

    pub fn is_full(self) -> bool {
        self.size >= 1.0
    }

    /// Zoomed in (`notches` > 0) or out around the point (`u`, `v`) of the shown part
    /// (0–1 across and down): that point of the picture stays under the mouse (unless the
    /// part would reach past the picture's edge).
    pub fn at(self, notches: f32, u: f32, v: f32) -> Zoom {
        let (u, v) = (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let factor = (Self::STEP.powf(notches) / self.size).clamp(1.0, Self::MAX);
        if factor < 1.001 {
            return Zoom::FULL;
        }
        let size = 1.0 / factor;
        let (px, py) = (self.x + u * self.size, self.y + v * self.size);
        Zoom { x: (px - u * size).clamp(0.0, 1.0 - size), y: (py - v * size).clamp(0.0, 1.0 - size), size }
    }

    /// Ctrl + drag by (`du`, `dv`), fractions of the shown part: the picture follows the
    /// mouse, as far as its edges.
    pub fn panned(self, du: f32, dv: f32) -> Zoom {
        let moved = |at: f32, by: f32| (at - by * self.size).clamp(0.0, 1.0 - self.size);
        Zoom { x: moved(self.x, du), y: moved(self.y, dv), size: self.size }
    }

    /// For the caption: "2.4×" (nothing at 1×).
    pub fn label(self) -> Option<String> {
        (!self.is_full()).then(|| format!("{:.1}×", 1.0 / self.size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_point_under_the_mouse() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // Two notches in at a point a quarter across, two thirds down.
        let z = Zoom::FULL.at(2.0, 0.25, 0.66);
        assert!(close(1.0 / z.size, 1.5625));
        assert!(close(z.x + 0.25 * z.size, 0.25) && close(z.y + 0.66 * z.size, 0.66));
        // Again elsewhere: the picture point under the mouse stays.
        let (u, v) = (0.8, 0.1);
        let (px, py) = (z.x + u * z.size, z.y + v * z.size);
        let z2 = z.at(3.0, u, v);
        assert!(close(z2.x + u * z2.size, px) && close(z2.y + v * z2.size, py));
        // Never past the edge, never more than 8×, back to the whole picture when zoomed out.
        let corner = Zoom::FULL.at(30.0, 1.0, 1.0);
        assert!(close(corner.size, 1.0 / Zoom::MAX) && close(corner.x + corner.size, 1.0));
        assert_eq!(z2.at(-40.0, 0.5, 0.5), Zoom::FULL);
        assert_eq!(Zoom::FULL.at(-1.0, 0.5, 0.5), Zoom::FULL);
        assert_eq!((z.label().as_deref(), Zoom::FULL.label()), (Some("1.6×"), None));
        assert_eq!(state_caption("Gate", false, Zoom { x: 0.0, y: 0.0, size: 0.5 }), "Gate · 2.0×");
        assert_eq!(state_caption("Gate", false, Zoom::FULL), "Gate");
    }

    #[test]
    fn panning_follows_the_mouse_up_to_the_edges() {
        let z = Zoom { x: 0.25, y: 0.25, size: 0.5 };
        // Dragged a quarter of the window to the right: the part left of it comes into view.
        assert_eq!(z.panned(0.25, 0.0), Zoom { x: 0.125, y: 0.25, size: 0.5 });
        assert_eq!(z.panned(0.0, -0.5), Zoom { x: 0.25, y: 0.5, size: 0.5 });
        // Never past the picture.
        assert_eq!(z.panned(9.0, -9.0), Zoom { x: 0.0, y: 0.5, size: 0.5 });
        assert_eq!(Zoom::FULL.panned(0.5, -0.5), Zoom::FULL);
    }

    #[test]
    fn slot_choice_new_replace_or_skip() {
        // (closing, pinned)
        let open2 = [(false, false), (false, false)];
        assert_eq!(choose_slot(&open2, 3, true), Slot::New);
        assert_eq!(choose_slot(&open2, 2, true), Slot::Replace(0));
        assert_eq!(choose_slot(&open2, 2, false), Slot::Skip);
        // oldest pinned → replace the next one
        assert_eq!(choose_slot(&[(false, true), (false, false)], 2, true), Slot::Replace(1));
        // all pinned → skip
        assert_eq!(choose_slot(&[(false, true), (false, true)], 2, true), Slot::Skip);
        // closing windows take no slot and are not replaced
        assert_eq!(choose_slot(&[(true, false), (false, false)], 2, true), Slot::New);
        assert_eq!(choose_slot(&[(true, false), (false, false), (false, false)], 2, true), Slot::Replace(1));
    }
}
