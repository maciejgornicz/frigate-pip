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

#[cfg(test)]
mod tests {
    use super::*;

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
