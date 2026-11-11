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
    s.push_str(&format!(" · {}", crate::util::local_time()));
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
    // Cameras named in the settings: in the camera filter, and those with an object list.
    let mut listed: Vec<(&String, &crate::config::CamOverride)> = cfg.overrides.iter().filter(|(_, o)| !o.labels.is_empty()).collect();
    listed.sort_by_key(|(n, _)| n.to_ascii_lowercase());
    let named = cfg.cameras.iter().map(|c| (c.clone(), c.clone()));
    for (c, title) in named.chain(listed.into_iter().map(|(n, o)| (n.clone(), o.name.clone().unwrap_or_else(|| n.clone())))) {
        if !cameras.iter().any(|(n, _)| n.eq_ignore_ascii_case(&c)) {
            cameras.push((c, title));
        }
    }
    let mut known: Vec<String> = cams.iter().flat_map(|c| c.labels.iter().cloned()).collect();
    if known.is_empty() {
        known = vec!["person".into(), "car".into()];
    }
    let mut own: Vec<&String> = cfg.overrides.values().flat_map(|o| o.labels.iter()).collect();
    own.sort();
    let wanted: Vec<String> = cfg.labels.iter().chain(own).cloned().collect();
    let labels = union(known.iter(), &wanted);
    let zones = union(cams.iter().flat_map(|c| c.zones.iter()), &cfg.zones);
    (cameras, labels, zones)
}

/// `name` is in a filter list (empty = every entry), in any case.
pub fn listed(list: &[String], name: &str) -> bool {
    list.is_empty() || list.iter().any(|x| x.eq_ignore_ascii_case(name))
}

/// An object in the settings' lists: its name in the interface language, with Frigate's
/// name after it in brackets.
pub fn label_text(l: &str) -> String {
    let name = i18n::label(l);
    if name == l {
        l.to_string()
    } else {
        format!("{name} ({l})")
    }
}

/// The objects checked for one camera in the settings, as its own list: none (empty) when
/// they are the same as for all cameras (`global`; empty = every one of `all`) or when
/// nothing is checked.
pub fn own_labels(picked: &[String], global: &[String], all: &[String]) -> Vec<String> {
    let global = if global.is_empty() { all } else { global };
    let has = |list: &[String], x: &String| list.iter().any(|y| y.eq_ignore_ascii_case(x));
    let same = picked.iter().all(|p| has(global, p)) && global.iter().all(|g| has(picked, g));
    if picked.is_empty() || same {
        Vec::new()
    } else {
        picked.to_vec()
    }
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

/// The amplitude for a camera audio volume in percent. Hearing is not linear: on this
/// curve the middle of the slider sounds about half as loud.
pub fn gain(volume: u8) -> f32 {
    let v = volume.min(100) as f32 / 100.0;
    v * v
}

/// Volume slider of a window `height` pixels high: a bar under the sound button `button`
/// (left, top, right, bottom), `None` when the window is too low for it.
pub fn volume_slider((l, _, r, b): (i32, i32, i32, i32), height: i32, scale: f32) -> Option<(i32, i32, i32, i32)> {
    let gap = (6.0 * scale).round() as i32;
    let (top, bottom) = (b + gap, (b + gap + (90.0 * scale).round() as i32).min(height - gap));
    (bottom - top >= (r - l) * 3 / 2).then_some((l, top, r, bottom))
}

/// The part of the slider the knob moves along: (top, bottom).
pub fn volume_track((l, t, r, b): (i32, i32, i32, i32)) -> (i32, i32) {
    let end = (r - l) / 2;
    (t + end, b - end)
}

/// Where the knob of `slider` is at `volume` percent.
pub fn volume_y(slider: (i32, i32, i32, i32), volume: u8) -> f32 {
    let (top, bottom) = volume_track(slider);
    bottom as f32 - (bottom - top) as f32 * volume.min(100) as f32 / 100.0
}

/// The volume in percent for a mouse at height `y`.
pub fn volume_at(slider: (i32, i32, i32, i32), y: i32) -> u8 {
    let (top, bottom) = volume_track(slider);
    ((bottom - y) as f32 / (bottom - top).max(1) as f32 * 100.0).round().clamp(0.0, 100.0) as u8
}

/// A round button of the same size left of `rect` (left, top, right, bottom), 6 px apart at 96 dpi.
pub fn left_of((l, t, r, b): (i32, i32, i32, i32), scale: f32) -> (i32, i32, i32, i32) {
    let gap = (6.0 * scale).round() as i32;
    (l - gap - (r - l), t, l - gap, b)
}

/// The replay icon: a ring open at the top with an anticlockwise arrow head.
pub struct ReplayIcon {
    pub radius: f32,
    /// The ring runs clockwise from `from` (at `end`) through the bottom to `to` (radians, y down).
    #[cfg_attr(windows, allow(dead_code))]
    pub from: f32,
    pub to: f32,
    /// The end of the ring that carries the head, and the head (tip first).
    pub end: (f32, f32),
    pub head: [(f32, f32); 3],
}

/// The replay icon in a round button with this centre and radius `rad`.
pub fn replay_icon((cx, cy): (f32, f32), rad: f32) -> ReplayIcon {
    let radius = rad * 0.45;
    let (from, to) = ((-60f32).to_radians(), 210f32.to_radians());
    let end = (cx + radius * from.cos(), cy + radius * from.sin());
    // Anticlockwise on the screen (y down) is the way of decreasing angles.
    let (tx, ty) = (from.sin(), -from.cos());
    let (nx, ny) = (from.cos(), from.sin());
    let h = radius * 0.6;
    let head = [(end.0 + tx * h, end.1 + ty * h), (end.0 + nx * h * 0.6, end.1 + ny * h * 0.6), (end.0 - nx * h * 0.6, end.1 - ny * h * 0.6)];
    ReplayIcon { radius, from, to, end, head }
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
    fn a_button_left_of_another() {
        assert_eq!(left_of((100, 8, 124, 32), 1.0), (70, 8, 94, 32));
        assert_eq!(left_of((200, 16, 248, 64), 2.0), (140, 16, 188, 64));
    }

    #[test]
    fn the_replay_arrow_points_into_the_gap_at_the_top() {
        let icon = replay_icon((50.0, 50.0), 20.0);
        let (tip, end) = (icon.head[0], icon.end);
        assert!(end.1 < 50.0 && end.0 > 50.0, "the ring ends at the top right: {end:?}");
        assert!(tip.1 < end.1, "the tip lies above the end of the ring: {tip:?}");
        assert!(tip.0 < end.0, "and left of it, anticlockwise: {tip:?}");
    }

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
    fn volume_curve_and_slider() {
        assert_eq!((gain(0), gain(50), gain(100), gain(200)), (0.0, 0.25, 1.0, 1.0));
        // Under a 24 px button: 6 px apart, 90 px long, the knob stays 12 px from the ends.
        let button = (418, 8, 442, 32);
        let slider = volume_slider(button, 270, 1.0).unwrap();
        assert_eq!(slider, (418, 38, 442, 128));
        assert_eq!(volume_track(slider), (50, 116));
        assert_eq!((volume_at(slider, 116), volume_at(slider, 83), volume_at(slider, 50)), (0, 50, 100));
        assert_eq!((volume_at(slider, 500), volume_at(slider, -5)), (0, 100));
        assert_eq!((volume_y(slider, 0), volume_y(slider, 50), volume_y(slider, 100)), (116.0, 83.0, 50.0));
        // The smallest window (160 x 90) still has a shorter one, a very wide one none.
        assert_eq!(volume_slider(button, 90, 1.0), Some((418, 38, 442, 84)));
        assert_eq!(volume_slider(button, 70, 1.0), None);
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

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn object_entries_in_the_lists() {
        // An empty list holds every object; names in any case.
        assert!(listed(&[], "dog") && listed(&names(&["Dog"]), "dog") && !listed(&names(&["cat"]), "dog"));
        // A label without a translation is shown as it is; one with, with Frigate's name after it.
        assert_eq!(label_text("doorbell"), "doorbell");
        let dog = i18n::label("dog");
        assert_eq!(label_text("dog"), if dog == "dog" { "dog".to_string() } else { format!("{dog} (dog)") });
    }

    #[test]
    fn camera_list_equal_to_the_global_one_is_no_list() {
        let all = names(&["person", "car", "dog", "cat"]);
        let global = names(&["person", "car"]);
        assert!(own_labels(&names(&["car", "Person"]), &global, &all).is_empty());
        assert_eq!(own_labels(&names(&["person", "dog"]), &global, &all), names(&["person", "dog"]));
        // Nothing checked: as for all cameras.
        assert!(own_labels(&[], &global, &all).is_empty());
        // Every object for all cameras (an empty list): only all checked is the same.
        assert!(own_labels(&all, &[], &all).is_empty());
        assert_eq!(own_labels(&names(&["dog"]), &[], &all), names(&["dog"]));
        // Every object for one camera while the others have a list: a list of them all.
        assert_eq!(own_labels(&all, &global, &all), all);
    }

    #[test]
    fn choices_offer_the_objects_of_camera_lists() {
        let cfg = Config::from_ini(&crate::config::IniDoc::parse("[trigger]\nlabels = car\n[camera.yard]\nlabels = dog, person\n"));
        let cams = [CameraInfo {
            name: "gate".into(),
            title: "Gate".into(),
            main: None,
            sub: None,
            aspect: 1.78,
            labels: names(&["person"]),
            zones: vec![],
        }];
        let (_, labels, _) = choices(&cams, &cfg);
        assert_eq!(labels, names(&["person", "car", "dog"]));
    }

    #[test]
    fn choices_offer_cameras_with_object_lists_while_frigate_is_away() {
        let cfg = Config::from_ini(&crate::config::IniDoc::parse(
            "[trigger]\ncameras = drive\n[camera.yard]\nname = Yard\nlabels = dog\n[camera.Gate]\nlabels = car\n[camera.pool]\nname = Pool\n",
        ));
        // No camera list from Frigate: the cameras named in the settings are offered.
        let (cameras, _, _) = choices(&[], &cfg);
        let pair = |n: &str, t: &str| (n.to_string(), t.to_string());
        assert_eq!(cameras, [pair("drive", "drive"), pair("Gate", "Gate"), pair("yard", "Yard")]);
        // Known to Frigate already (any case): not twice.
        let gate = CameraInfo { name: "gate".into(), title: "Gate".into(), main: None, sub: None, aspect: 1.78, labels: vec![], zones: vec![] };
        let (cameras, _, _) = choices(&[gate], &cfg);
        assert_eq!(cameras, [pair("gate", "Gate"), pair("drive", "drive"), pair("yard", "Yard")]);
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
