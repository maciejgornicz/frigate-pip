//! Decides whether a Frigate message (`events` / `reviews`) should show a window.

use crate::config::{Config, TriggerMode};
use crate::json::Json;
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    pub camera: String,
    pub label: String,
    pub sub_label: Option<String>,
    /// Confidence 0.0–1.0 (0 = unknown).
    pub score: f32,
    pub id: String,
}

#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// New object matching the filters — show the window.
    Show(Detection),
    /// Object on the camera is still moving — extend the display if the window is open.
    Activity(String),
    Ignore,
}

pub struct EventFilter {
    mode: TriggerMode,
    cameras: Vec<String>,
    labels: Vec<String>,
    zones: Vec<String>,
    ignore_stationary: bool,
    min_score: f32,
    shown: VecDeque<String>,
    /// Ids met on the `events` topic and review items already seen. A review item that is
    /// neither — Frigate starts it with an "update" — is a manual event (Frigate's API, e.g.
    /// from a Home Assistant automation, or a dedicated LPR camera), with no `events` message.
    seen: Recent,
    /// Unix time this connection began (by Frigate's clock when known, see `set_since`):
    /// older review items do not open windows.
    since: f64,
}

/// Review items may start a little before `since` (the `Date` header has 1 s resolution).
const CLOCK_SLACK: f64 = 5.0;

/// Ids met lately; one met again counts as new, so the ids of objects that are still active
/// are never forgotten.
#[derive(Default)]
struct Recent {
    at: HashMap<String, u64>,
    clock: u64,
}

impl Recent {
    const KEEP: u64 = 512;

    fn note(&mut self, id: &str) {
        if id.is_empty() {
            return;
        }
        self.clock += 1;
        let now = self.clock;
        *self.at.entry(id.to_string()).or_default() = now;
        if self.at.len() as u64 > 2 * Self::KEEP {
            self.at.retain(|_, t| *t + Self::KEEP > now);
        }
    }

    fn has(&self, id: &str) -> bool {
        self.at.contains_key(id)
    }
}

fn unix_now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn any_in(items: impl Iterator<Item = impl AsRef<str>>, set: &[String]) -> bool {
    let mut items = items;
    items.any(|i| set.iter().any(|s| s.eq_ignore_ascii_case(i.as_ref())))
}

fn base_label(l: &str) -> &str {
    l.strip_suffix("-verified").unwrap_or(l)
}

impl EventFilter {
    pub fn new(cfg: &Config) -> Self {
        EventFilter {
            mode: cfg.mode,
            cameras: cfg.cameras.clone(),
            labels: cfg.labels.clone(),
            zones: cfg.zones.clone(),
            ignore_stationary: cfg.ignore_stationary,
            min_score: cfg.min_score,
            shown: VecDeque::new(),
            seen: Recent::default(),
            since: unix_now(),
        }
    }

    /// The connection's start by Frigate's clock (its HTTP `Date`), so that review items are
    /// dated against the same clock as their `start_time`.
    pub fn set_since(&mut self, t: f64) {
        self.since = t;
    }

    /// `events` mode also reads review items, for manual events.
    pub fn wants(&self, topic: &str) -> bool {
        match self.mode {
            TriggerMode::Events => topic == "events" || topic == "reviews",
            TriggerMode::Alerts | TriggerMode::Reviews => topic == "reviews",
        }
    }

    pub fn on_message(&mut self, topic: &str, payload: &Json) -> Verdict {
        if !self.wants(topic) {
            return Verdict::Ignore;
        }
        match (self.mode, topic) {
            (TriggerMode::Events, "events") => self.on_event(payload),
            (TriggerMode::Events, _) => self.on_manual_review(payload),
            _ => self.on_review(payload),
        }
    }

    /// Remembers the id; `true` if seen for the first time.
    fn mark_shown(&mut self, id: &str) -> bool {
        if self.shown.iter().any(|s| s == id) {
            return false;
        }
        if self.shown.len() >= 512 {
            self.shown.pop_front();
        }
        self.shown.push_back(id.to_string());
        true
    }

    fn known(&self, id: &str) -> bool {
        self.seen.has(id) || self.shown.iter().any(|s| s == id)
    }

    /// The review item began during this connection.
    fn recent(&self, item: &Json) -> bool {
        item.get("start_time").f64().is_some_and(|t| t >= self.since - CLOCK_SLACK)
    }

    fn camera_ok(&self, cam: &str) -> bool {
        self.cameras.is_empty() || any_in(std::iter::once(cam), &self.cameras)
    }

    fn object_matches(&self, o: &Json) -> bool {
        if !o.is_obj() || o.get("false_positive").bool() == Some(true) {
            return false;
        }
        let (Some(cam), Some(label)) = (o.get("camera").str(), o.get("label").str()) else { return false };
        if !self.camera_ok(cam) {
            return false;
        }
        if !self.labels.is_empty() && !any_in(std::iter::once(label), &self.labels) {
            return false;
        }
        if !self.zones.is_empty() {
            let zones = o.get("entered_zones").strs().chain(o.get("current_zones").strs());
            if !any_in(zones, &self.zones) {
                return false;
            }
        }
        if self.ignore_stationary && o.get("stationary").bool() == Some(true) {
            return false;
        }
        let score = o.get("top_score").f64().unwrap_or(0.0).max(o.get("score").f64().unwrap_or(0.0)) as f32;
        score >= self.min_score
    }

    fn on_event(&mut self, p: &Json) -> Verdict {
        let typ = p.get("type").str().unwrap_or("");
        let after = p.get("after");
        self.seen.note(after.get("id").str().unwrap_or(""));
        if typ == "end" || !self.object_matches(after) {
            return Verdict::Ignore;
        }
        let id = after.get("id").str().unwrap_or("");
        let camera = after.get("camera").str().unwrap_or("").to_string();
        let newly = typ == "new" || (typ == "update" && !self.object_matches(p.get("before")));
        if newly && self.mark_shown(id) {
            let score = after.get("top_score").f64().or(after.get("score").f64()).unwrap_or(0.0) as f32;
            return Verdict::Show(Detection {
                camera,
                label: after.get("label").str().unwrap_or("").to_string(),
                sub_label: sub_label(after.get("sub_label")),
                score,
                id: id.to_string(),
            });
        }
        Verdict::Activity(camera)
    }

    fn review_matches(&self, r: &Json) -> bool {
        if !r.is_obj() {
            return false;
        }
        let Some(cam) = r.get("camera").str() else { return false };
        if !self.camera_ok(cam) {
            return false;
        }
        if self.mode == TriggerMode::Alerts && r.get("severity").str() != Some("alert") {
            return false;
        }
        let data = r.get("data");
        if !self.labels.is_empty() && !any_in(data.get("objects").strs().map(base_label), &self.labels) {
            return false;
        }
        self.zones.is_empty() || any_in(data.get("zones").strs(), &self.zones)
    }

    fn on_review(&mut self, p: &Json) -> Verdict {
        let typ = p.get("type").str().unwrap_or("");
        let after = p.get("after");
        if typ == "end" || !self.review_matches(after) {
            return Verdict::Ignore;
        }
        let id = after.get("id").str().unwrap_or("");
        let camera = after.get("camera").str().unwrap_or("").to_string();
        // Manual events and LPR cameras start their item with an "update" (so does sound,
        // which stays without a window, as before: no objects).
        let first = !self.known(id);
        self.seen.note(id);
        let objects = after.get("data").get("objects").strs().next().is_some();
        let newly = typ == "new" || (typ == "update" && (!self.review_matches(p.get("before")) || (first && objects && self.recent(after))));
        if newly && self.mark_shown(id) {
            return Verdict::Show(self.review_detection(camera, after));
        }
        Verdict::Activity(camera)
    }

    /// `events` mode: a manual event has no `events` message, only a review item that
    /// starts with an "update" (the items of tracked objects start with "new", and their
    /// objects come through `events`). Such an item opens a window like a new object.
    fn on_manual_review(&mut self, p: &Json) -> Verdict {
        let typ = p.get("type").str().unwrap_or("");
        let after = p.get("after");
        let id = after.get("id").str().unwrap_or("");
        let camera = after.get("camera").str().unwrap_or("").to_string();
        if self.shown.iter().any(|s| s == id) {
            return if typ == "update" { Verdict::Activity(camera) } else { Verdict::Ignore };
        }
        let first = !self.seen.has(id);
        self.seen.note(id);
        let data = after.get("data");
        let tracked = data.get("detections").strs().any(|d| self.seen.has(d));
        let objects = data.get("objects").strs().next().is_some();
        if typ != "update" || !first || tracked || !objects || !self.recent(after) || !self.review_matches(after) {
            return Verdict::Ignore;
        }
        self.mark_shown(id);
        Verdict::Show(self.review_detection(camera, after))
    }

    fn review_detection(&self, camera: String, item: &Json) -> Detection {
        let data = item.get("data");
        let objects: Vec<&str> = data.get("objects").strs().map(base_label).collect();
        let label = objects
            .iter()
            .find(|o| self.labels.is_empty() || any_in(std::iter::once(**o), &self.labels))
            .or(objects.first())
            .or(data.get("audio").strs().next().as_ref())
            .copied()
            .unwrap_or("")
            .to_string();
        Detection {
            camera,
            label,
            sub_label: data.get("sub_labels").strs().next().map(str::to_string),
            score: 0.0,
            id: item.get("id").str().unwrap_or("").to_string(),
        }
    }
}

/// `sub_label` is either a string or a `[name, confidence]` pair.
fn sub_label(v: &Json) -> Option<String> {
    v.str().or_else(|| v.arr().first().and_then(|x| x.str())).filter(|s| !s.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IniDoc;

    fn cfg(extra: &str) -> Config {
        Config::from_ini(&IniDoc::parse(&format!("[trigger]\n{extra}\n")))
    }

    fn obj(id: &str, cam: &str, label: &str, stationary: bool, zones: &[&str]) -> String {
        let z: Vec<String> = zones.iter().map(|z| format!("\"{z}\"")).collect();
        format!(
            r#"{{"id": "{id}", "camera": "{cam}", "label": "{label}", "sub_label": null, "top_score": 0.81, "score": 0.7, "false_positive": false, "stationary": {stationary}, "entered_zones": [{}], "current_zones": [{}]}}"#,
            z.join(","),
            z.join(",")
        )
    }

    fn event(typ: &str, before: &str, after: &str) -> Json {
        Json::parse(&format!(r#"{{"type": "{typ}", "before": {before}, "after": {after}}}"#)).unwrap()
    }

    #[test]
    fn new_object_shows_once() {
        let mut f = EventFilter::new(&cfg(""));
        let a = obj("e1", "brama", "person", false, &[]);
        let v = f.on_message("events", &event("new", &a, &a));
        let Verdict::Show(d) = v else { panic!("{v:?}") };
        assert_eq!(d.camera, "brama");
        assert_eq!(d.label, "person");
        assert!((d.score - 0.81).abs() < 1e-6);
        assert_eq!(f.on_message("events", &event("new", &a, &a)), Verdict::Activity("brama".into()));
        assert_eq!(f.on_message("events", &event("update", &a, &a)), Verdict::Activity("brama".into()));
        assert_eq!(f.on_message("events", &event("end", &a, &a)), Verdict::Ignore);
    }

    #[test]
    fn old_stationary_object_after_app_start_is_ignored() {
        let mut f = EventFilter::new(&cfg(""));
        let a = obj("old", "podjazd", "car", true, &[]);
        assert_eq!(f.on_message("events", &event("update", &a, &a)), Verdict::Ignore);
    }

    #[test]
    fn old_moving_object_after_app_start_only_extends() {
        let mut f = EventFilter::new(&cfg(""));
        let a = obj("old", "podjazd", "car", false, &[]);
        assert_eq!(f.on_message("events", &event("update", &a, &a)), Verdict::Activity("podjazd".into()));
    }

    #[test]
    fn parked_car_starting_to_move_shows() {
        let mut f = EventFilter::new(&cfg(""));
        let b = obj("p1", "podjazd", "car", true, &[]);
        let a = obj("p1", "podjazd", "car", false, &[]);
        assert!(matches!(f.on_message("events", &event("update", &b, &a)), Verdict::Show(_)));
    }

    #[test]
    fn entering_required_zone_shows() {
        let mut f = EventFilter::new(&cfg("zones = Droga"));
        let outside = obj("z1", "brama", "person", false, &[]);
        let inside = obj("z1", "brama", "person", false, &["Droga"]);
        assert_eq!(f.on_message("events", &event("new", &outside, &outside)), Verdict::Ignore);
        assert!(matches!(f.on_message("events", &event("update", &outside, &inside)), Verdict::Show(_)));
    }

    #[test]
    fn camera_and_label_filters() {
        let mut f = EventFilter::new(&cfg("cameras = brama\nlabels = person, car"));
        let dog = obj("d", "brama", "dog", false, &[]);
        let other_cam = obj("o", "ogrod", "person", false, &[]);
        let ok = obj("k", "Brama", "Person", false, &[]);
        assert_eq!(f.on_message("events", &event("new", &dog, &dog)), Verdict::Ignore);
        assert_eq!(f.on_message("events", &event("new", &other_cam, &other_cam)), Verdict::Ignore);
        assert!(matches!(f.on_message("events", &event("new", &ok, &ok)), Verdict::Show(_)));
        assert_eq!(f.on_message("reviews", &event("new", &ok, &ok)), Verdict::Ignore);
    }

    #[test]
    fn min_score_and_false_positive() {
        let mut f = EventFilter::new(&cfg("min_score = 0.9"));
        let a = obj("s", "brama", "person", false, &[]);
        assert_eq!(f.on_message("events", &event("new", &a, &a)), Verdict::Ignore);
        let mut f = EventFilter::new(&cfg(""));
        let fp = a.replace("\"false_positive\": false", "\"false_positive\": true");
        assert_eq!(f.on_message("events", &event("new", &fp, &fp)), Verdict::Ignore);
    }

    fn review(id: &str, severity: &str, objects: &[&str]) -> String {
        let o: Vec<String> = objects.iter().map(|x| format!("\"{x}\"")).collect();
        format!(
            r#"{{"id": "{id}", "camera": "brama", "severity": "{severity}", "data": {{"objects": [{}], "sub_labels": ["Jan"], "zones": [], "audio": []}}}}"#,
            o.join(",")
        )
    }

    #[test]
    fn alerts_mode() {
        let mut f = EventFilter::new(&cfg("mode = alerts"));
        let det = review("r1", "detection", &["dog"]);
        let alert = review("r1", "alert", &["person-verified", "dog"]);
        assert_eq!(f.on_message("events", &event("new", &alert, &alert)), Verdict::Ignore);
        assert_eq!(f.on_message("reviews", &event("new", &det, &det)), Verdict::Ignore);
        let v = f.on_message("reviews", &event("update", &det, &alert));
        let Verdict::Show(d) = v else { panic!("{v:?}") };
        assert_eq!(d.label, "person");
        assert_eq!(d.sub_label.as_deref(), Some("Jan"));
        assert_eq!(f.on_message("reviews", &event("update", &alert, &alert)), Verdict::Activity("brama".into()));
    }

    #[test]
    fn reviews_mode_accepts_detections() {
        let mut f = EventFilter::new(&cfg("mode = reviews\nlabels = dog"));
        let det = review("r2", "detection", &["dog"]);
        assert!(matches!(f.on_message("reviews", &event("new", &det, &det)), Verdict::Show(_)));
        let cat = review("r3", "detection", &["cat"]);
        assert_eq!(f.on_message("reviews", &event("new", &cat, &cat)), Verdict::Ignore);
    }

    /// A review item as Frigate sends it for a manual event: `detections` holds the ids of
    /// the events, `start_time` is seconds after the filter was made (negative = before).
    fn item(f: &EventFilter, id: &str, severity: &str, objects: &[&str], detections: &[&str], start: f64) -> String {
        let q = |v: &[&str]| v.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(",");
        format!(
            r#"{{"id": "{id}", "camera": "brama", "severity": "{severity}", "start_time": {}, "end_time": null, "data": {{"detections": [{}], "objects": [{}], "sub_labels": [], "zones": [], "audio": []}}}}"#,
            f.since + start,
            q(detections),
            q(objects)
        )
    }

    #[test]
    fn manual_event_opens_a_window_in_events_mode() {
        let mut f = EventFilter::new(&cfg(""));
        let m = item(&f, "r1", "alert", &["doorbell"], &["e9"], 1.0);
        // Frigate sends no `events` message and starts the item with an "update".
        let v = f.on_message("reviews", &event("update", &m, &m));
        let Verdict::Show(d) = v else { panic!("{v:?}") };
        assert_eq!((d.camera.as_str(), d.label.as_str(), d.score), ("brama", "doorbell", 0.0));
        assert_eq!(f.on_message("reviews", &event("update", &m, &m)), Verdict::Activity("brama".into()));
        assert_eq!(f.on_message("reviews", &event("end", &m, &m)), Verdict::Ignore);
    }

    #[test]
    fn tracked_objects_items_do_not_open_a_second_window() {
        let mut f = EventFilter::new(&cfg(""));
        let a = obj("e1", "brama", "person", false, &[]);
        assert!(matches!(f.on_message("events", &event("new", &a, &a)), Verdict::Show(_)));
        // Its review item: through "new", or an "update" that holds the known object.
        let r = item(&f, "r1", "alert", &["person"], &["e1"], 1.0);
        assert_eq!(f.on_message("reviews", &event("update", &r, &r)), Verdict::Ignore);
        let r2 = item(&f, "r2", "alert", &["person"], &["e2"], 1.0);
        assert_eq!(f.on_message("reviews", &event("new", &r2, &r2)), Verdict::Ignore);
        assert_eq!(f.on_message("reviews", &event("update", &r2, &r2)), Verdict::Ignore);
        // A car filtered out on `events` is known too, so its item is no manual event.
        let mut f = EventFilter::new(&cfg("labels = person"));
        let car = obj("e3", "brama", "car", false, &[]);
        assert_eq!(f.on_message("events", &event("new", &car, &car)), Verdict::Ignore);
        let r3 = item(&f, "r3", "detection", &["car"], &["e3"], 1.0);
        assert_eq!(f.on_message("reviews", &event("update", &r3, &r3)), Verdict::Ignore);
    }

    #[test]
    fn manual_events_follow_filters_and_age() {
        let mut f = EventFilter::new(&cfg("cameras = ogrod"));
        let m = item(&f, "r1", "alert", &["person"], &["e1"], 1.0);
        assert_eq!(f.on_message("reviews", &event("update", &m, &m)), Verdict::Ignore);
        // An item that began before FrigatePiP connected only goes on.
        let mut f = EventFilter::new(&cfg(""));
        let old = item(&f, "r2", "alert", &["person"], &["e2"], -60.0);
        assert_eq!(f.on_message("reviews", &event("update", &old, &old)), Verdict::Ignore);
        // Sound only (no objects): not a manual event.
        let sound = item(&f, "r3", "detection", &[], &[], 1.0);
        assert_eq!(f.on_message("reviews", &event("update", &sound, &sound)), Verdict::Ignore);
    }

    #[test]
    fn manual_event_in_reviews_mode_but_not_sound() {
        let mut f = EventFilter::new(&cfg("mode = reviews"));
        let m = item(&f, "r1", "detection", &["doorbell"], &["e1"], 1.0);
        assert!(matches!(f.on_message("reviews", &event("update", &m, &m)), Verdict::Show(_)));
        // Sound starts its item with an "update" too, but opens no window (as in 1.6.0).
        let sound = item(&f, "r2", "detection", &[], &[], 1.0).replace("\"audio\": []", "\"audio\": [\"speech\"]");
        assert_eq!(f.on_message("reviews", &event("update", &sound, &sound)), Verdict::Activity("brama".into()));
    }

    #[test]
    fn object_filter_applies_to_manual_events() {
        let mut f = EventFilter::new(&cfg("labels = person, car"));
        let bell = item(&f, "r1", "detection", &["doorbell"], &["e1"], 1.0);
        assert_eq!(f.on_message("reviews", &event("update", &bell, &bell)), Verdict::Ignore);
        let person = item(&f, "r2", "alert", &["person"], &["e2"], 1.0);
        assert!(matches!(f.on_message("reviews", &event("update", &person, &person)), Verdict::Show(_)));
    }

    #[test]
    fn since_follows_the_server_clock() {
        let mut f = EventFilter::new(&cfg(""));
        // Frigate's clock is two minutes behind the PC: its items are dated by that clock.
        f.set_since(f.since - 120.0);
        let m = item(&f, "r1", "alert", &["person"], &["e1"], 1.0);
        assert!(matches!(f.on_message("reviews", &event("update", &m, &m)), Verdict::Show(_)));
    }

    #[test]
    fn active_ids_are_not_forgotten() {
        let mut r = Recent::default();
        r.note("old");
        r.note("keep");
        for i in 0..1100 {
            r.note(&format!("id{i}"));
            if i % 100 == 0 {
                r.note("keep");
            }
        }
        assert!(r.has("keep"));
        assert!(!r.has("old"));
        assert!(r.at.len() as u64 <= 2 * Recent::KEEP);
    }

    #[test]
    fn manual_event_in_alerts_mode() {
        let mut f = EventFilter::new(&cfg("mode = alerts"));
        let m = item(&f, "r1", "alert", &["person"], &["e1"], 1.0);
        let v = f.on_message("reviews", &event("update", &m, &m));
        assert!(matches!(v, Verdict::Show(ref d) if d.label == "person"), "{v:?}");
        assert_eq!(f.on_message("reviews", &event("update", &m, &m)), Verdict::Activity("brama".into()));
        // An alert that began before the connection does not pop up.
        let old = item(&f, "r2", "alert", &["person"], &["e2"], -60.0);
        assert_eq!(f.on_message("reviews", &event("update", &old, &old)), Verdict::Activity("brama".into()));
    }

    #[test]
    fn sub_label_forms() {
        assert_eq!(sub_label(&Json::parse("\"Jan\"").unwrap()).as_deref(), Some("Jan"));
        assert_eq!(sub_label(&Json::parse("[\"WX123\", 0.9]").unwrap()).as_deref(), Some("WX123"));
        assert_eq!(sub_label(&Json::Null), None);
    }
}
