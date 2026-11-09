//! Desktop output: each preview window is a borderless always-on-top X11 window (an X11
//! desktop, or Wayland through XWayland) in the chosen corner of the chosen monitor.
//! The mouse works like on Windows: × closes, the speaker mutes, the slider under it sets
//! the volume, dragging moves (with Ctrl a zoomed picture, otherwise the window), a click
//! pins, a double click opens the camera in Frigate, the right button closes.

use super::canvas::{Font, Image};
use super::x11::{Event, Monitor, BUTTON1_MASK, CONTROL_MASK, X11};
use super::{Action, Output, Rect};
use std::collections::HashMap;
use std::time::Duration;

/// The button under the mouse.
#[derive(Clone, Copy, PartialEq)]
enum Hot {
    None,
    Close,
    Sound,
    Volume,
}

/// Left button held on a window.
enum Press {
    /// It moves the window: where it started (screen, window) and whether it moved.
    Window { root: (i32, i32), start: (i32, i32), moved: bool },
    /// Ctrl + drag on a zoomed picture: the last mouse position on the screen.
    Picture { last: (i32, i32) },
    /// On the volume slider, at this volume (percent).
    Volume(u8),
}

/// What a mouse move with the button held does.
#[derive(Debug, PartialEq)]
enum Drag {
    /// The window goes to this place on the screen.
    Window(i32, i32),
    /// The picture follows the mouse, see `Action::Pan`.
    Picture(f32, f32),
}

impl Press {
    /// The mouse moved to `to` on the screen, over a window of `size`.
    fn drag(&mut self, to: (i32, i32), size: (u32, u32)) -> Option<Drag> {
        match self {
            Press::Picture { last } => {
                let (dx, dy) = (to.0 - last.0, to.1 - last.1);
                *last = to;
                Some(Drag::Picture(dx as f32 / size.0.max(1) as f32, dy as f32 / size.1.max(1) as f32))
            }
            // After a few pixels, so that a click stays a click.
            Press::Window { root, start, moved } => {
                let (dx, dy) = (to.0 - root.0, to.1 - root.1);
                *moved |= dx * dx + dy * dy > 16;
                moved.then_some(Drag::Window(start.0 + dx, start.1 + dy))
            }
            Press::Volume(_) => None,
        }
    }

    /// The button went up, with the window at `at` in the usable area: a click or the end of
    /// a move. Moving the picture is neither a click nor a double click.
    fn released(&self, at: (i32, i32)) -> Option<Action> {
        match self {
            Press::Window { moved: true, .. } => Some(Action::Moved(at.0, at.1)),
            Press::Window { .. } => Some(Action::Pin),
            Press::Picture { .. } => None,
            // ffmpeg needs a moment for a new volume, so it gets the final one.
            Press::Volume(volume) => Some(Action::Volume(*volume)),
        }
    }
}

struct Win {
    xid: u32,
    /// On the screen (root window coordinates).
    rect: Rect,
    /// Last picture, caption and live flag (drawn again when the buttons change).
    last: Option<(Image, String, bool)>,
    hover: bool,
    hot: Hot,
    /// 0 = no camera audio, 1 = sound on, 2 = muted.
    sound: u8,
    /// Volume in percent.
    volume: u8,
    /// The picture is zoomed in.
    zoomed: bool,
    press: Option<Press>,
    /// Time and place of the last left click (double clicks).
    last_click: Option<(u32, i32, i32)>,
    /// Above all other windows (`topmost`), or an ordinary managed window.
    on_top: bool,
}

pub struct Desktop {
    x: X11,
    /// Usable part of the chosen monitor, in screen coordinates.
    area: Rect,
    scale: f32,
    monitor: String,
    monitors: Vec<Monitor>,
    /// New windows stay above all others (`topmost`).
    topmost: bool,
    changed: bool,
    /// Calls of `refresh` (every 0.5 s).
    ticks: u32,
    /// One of our windows was covered: check at the next `refresh` whether to raise it.
    recheck: bool,
    font: Option<Font>,
    wins: HashMap<u64, Win>,
}

type Area = (i32, i32, i32, i32);

/// Close and sound buttons of a window `w` pixels wide, (x0, y0, x1, y1) each: the same
/// places as on Windows.
fn button_rects(w: u32, scale: f32) -> (Area, Area) {
    let size = (24.0 * scale).round() as i32;
    let margin = (8.0 * scale).round() as i32;
    let close = (w as i32 - margin - size, margin, w as i32 - margin, margin + size);
    (close, crate::view::left_of(close, scale))
}

fn inside((x0, y0, x1, y1): Area, x: i32, y: i32) -> bool {
    x >= x0 && x < x1 && y >= y0 && y < y1
}

/// Monitors in the order of their numbers in the `monitor` setting: from the left, then
/// from the top (like on Windows).
fn numbered(monitors: &[Monitor]) -> Vec<Monitor> {
    let mut sorted = monitors.to_vec();
    sorted.sort_by_key(|m| (m.x, m.y));
    sorted
}

/// The monitor from the `monitor` setting: "primary" or a number (see `numbered`).
fn pick_monitor(monitors: &[Monitor], setting: &str) -> Option<Monitor> {
    if let Ok(n) = setting.trim().parse::<usize>() {
        if let Some(m) = n.checked_sub(1).and_then(|i| numbered(monitors).get(i).copied()) {
            return Some(m);
        }
    }
    monitors.iter().find(|m| m.primary).or(monitors.first()).copied()
}

/// The monitor without panels and docks: its overlap with the work area (when that is sensible).
fn usable(m: Monitor, work: Option<(i32, i32, u32, u32)>) -> Rect {
    let full = Rect { x: m.x, y: m.y, w: m.w, h: m.h };
    let Some((wx, wy, ww, wh)) = work else { return full };
    let (x0, y0) = (m.x.max(wx), m.y.max(wy));
    let (x1, y1) = ((m.x + m.w as i32).min(wx + ww as i32), (m.y + m.h as i32).min(wy + wh as i32));
    if x1 - x0 < m.w as i32 / 2 || y1 - y0 < m.h as i32 / 2 {
        return full;
    }
    Rect { x: x0, y: y0, w: (x1 - x0) as u32, h: (y1 - y0) as u32 }
}

impl Desktop {
    /// Connects to the desktop; pointer events go to `on_event` (from another thread).
    pub fn open(cfg: &crate::config::Config, font: Option<Font>, on_event: impl Fn(Event) + Send + 'static) -> Result<Desktop, String> {
        let x = X11::connect(on_event)?;
        let mut d = Desktop {
            x,
            area: Rect { x: 0, y: 0, w: 0, h: 0 },
            scale: 1.0,
            monitor: cfg.monitor.clone(),
            monitors: Vec::new(),
            topmost: cfg.topmost,
            changed: false,
            ticks: 0,
            recheck: false,
            font,
            wins: HashMap::new(),
        };
        d.find_area();
        Ok(d)
    }

    fn find_area(&mut self) {
        let monitors = self.x.monitors();
        let (w, h) = self.x.root_size;
        let m = pick_monitor(&monitors, &self.monitor).unwrap_or(Monitor { x: 0, y: 0, w, h, primary: true });
        self.area = usable(m, self.x.work_area());
        self.scale = self.x.scale();
        self.monitors = monitors.clone();
        let a = self.area;
        crate::log!("Desktop: {} monitor(s), windows in {}x{} at {},{}, scale {:.2}", monitors.len().max(1), a.w, a.h, a.x, a.y, self.scale);
    }

    /// Draws the window again: picture, caption and the buttons that are shown.
    fn paint(&mut self, id: u64) {
        let Some(w) = self.wins.get(&id) else { return };
        let Some((img, caption, live)) = &w.last else { return };
        let mut pic = if (img.w, img.h) == (w.rect.w, w.rect.h) { img.clone() } else { img.resized(w.rect.w, w.rect.h) };
        let (close, sound) = button_rects(pic.w, self.scale);
        // The speaker stays while muted, so that it is clear why the window is quiet.
        let show_sound = w.sound > 0 && (w.hover || w.sound == 2);
        let left = if show_sound {
            sound.0
        } else if w.hover {
            close.0
        } else {
            pic.w as i32
        };
        let reserve = (pic.w as i32 - left) as f32;
        if let (Some(font), false) = (&self.font, caption.is_empty()) {
            font.caption(&mut pic, caption, *live, reserve);
        }
        if w.hover {
            pic.close_button(close, w.hot == Hot::Close);
        }
        if show_sound {
            pic.sound_button(sound, w.sound == 2, w.hot == Hot::Sound);
        }
        if let Some(slider) = crate::view::volume_slider(sound, pic.h as i32, self.scale).filter(|_| w.hover && w.sound > 0) {
            pic.volume_slider(slider, w.volume);
        }
        let xid = w.xid;
        self.x.put_image(xid, &pic);
    }
}

impl Output for Desktop {
    fn size(&self) -> (u32, u32) {
        (self.area.w, self.area.h)
    }

    fn scale(&self) -> f32 {
        self.scale
    }

    fn buttons(&self) -> bool {
        true
    }

    fn monitors(&self) -> Vec<(i32, u32, u32, bool)> {
        numbered(&self.monitors).iter().map(|m| (m.x, m.w, m.h, m.primary)).collect()
    }

    fn draw(&mut self, id: u64, rect: Rect, img: Image, caption: &str, live: bool) {
        let r = Rect { x: self.area.x + rect.x, y: self.area.y + rect.y, w: rect.w, h: rect.h };
        let Desktop { x, wins, topmost, .. } = self;
        let w = wins.entry(id).or_insert_with(|| Win {
            xid: x.create_window(r.x, r.y, r.w, r.h, *topmost),
            rect: r,
            last: None,
            hover: false,
            hot: Hot::None,
            sound: 0,
            volume: 100,
            zoomed: false,
            press: None,
            last_click: None,
            on_top: *topmost,
        });
        // Not while the user drags it.
        if w.rect != r && !matches!(w.press, Some(Press::Window { moved: true, .. })) {
            x.configure(w.xid, r.x, r.y, r.w, r.h, w.on_top);
            w.rect = r;
        }
        w.last = Some((img, caption.to_string(), live));
        self.paint(id);
    }

    fn clear(&mut self, id: u64) {
        if let Some(w) = self.wins.remove(&id) {
            self.x.destroy(w.xid);
        }
    }

    fn refresh(&mut self) -> bool {
        // Window managers of plain X11 desktops (Xfce, Openbox…) may stack a clicked window
        // above ours; GNOME and KDE keep such windows on top themselves. Ours go back on top
        // when covered (checked at once, and every 2 s in case the event was missed), but
        // never above an open menu or popup.
        self.ticks += 1;
        if std::mem::take(&mut self.recheck) || self.ticks.is_multiple_of(4) {
            let xids: Vec<u32> = self.wins.values().filter(|w| w.on_top && w.press.is_none()).map(|w| w.xid).collect();
            if !xids.is_empty() && self.x.should_raise(&xids) {
                for xid in xids {
                    self.x.raise(xid);
                }
            }
        }
        std::mem::take(&mut self.changed)
    }

    fn configure(&mut self, cfg: &crate::config::Config) {
        // Windows opened from now on.
        self.topmost = cfg.topmost;
        if cfg.monitor != self.monitor {
            self.monitor = cfg.monitor.clone();
            self.find_area();
            self.changed = true;
        }
    }

    fn fullscreen(&mut self) -> bool {
        self.x.fullscreen_active()
    }

    fn set_sound(&mut self, id: u64, state: u8) {
        if let Some(w) = self.wins.get_mut(&id) {
            if w.sound != state {
                w.sound = state;
                self.paint(id);
            }
        }
    }

    fn set_volume(&mut self, id: u64, volume: u8) {
        if let Some(w) = self.wins.get_mut(&id).filter(|w| !matches!(w.press, Some(Press::Volume(_)))) {
            w.volume = volume;
        }
    }

    fn set_zoomed(&mut self, id: u64, zoomed: bool) {
        if let Some(w) = self.wins.get_mut(&id) {
            w.zoomed = zoomed;
        }
    }

    fn panning(&self, id: u64) -> bool {
        self.wins.get(&id).is_some_and(|w| matches!(w.press, Some(Press::Picture { .. })))
    }

    /// The mouse reports far more often than pictures are needed (Ctrl + drag).
    fn min_interval(&self) -> Duration {
        Duration::from_millis(16)
    }

    fn pointer(&mut self, e: &Event) -> Vec<(u64, Action)> {
        let window = match *e {
            Event::Press { window, .. } | Event::Release { window, .. } | Event::Motion { window, .. } => window,
            Event::Enter { window } | Event::Leave { window } | Event::Expose { window } => window,
            Event::Obscured { .. } => {
                self.recheck = true;
                return Vec::new();
            }
            // A monitor was added or removed, the resolution, the panels or the scaling
            // changed: the windows are placed again.
            Event::Screen { atom } => {
                if atom == 0 || self.x.is_screen_property(atom) {
                    self.find_area();
                    self.changed = true;
                }
                return Vec::new();
            }
            Event::Closed => return Vec::new(),
        };
        let Some(id) = self.wins.iter().find(|(_, w)| w.xid == window).map(|(id, _)| *id) else { return Vec::new() };
        let (scale, area) = (self.scale, self.area);
        let Some(w) = self.wins.get_mut(&id) else { return Vec::new() };
        let (close, sound) = button_rects(w.rect.w, scale);
        let slider = crate::view::volume_slider(sound, w.rect.h as i32, scale);
        let hot_at = |w: &Win, x: i32, y: i32| match () {
            _ if !w.hover => Hot::None,
            _ if inside(close, x, y) => Hot::Close,
            _ if w.sound > 0 && inside(sound, x, y) => Hot::Sound,
            _ if w.sound > 0 && slider.is_some_and(|s| inside(s, x, y)) => Hot::Volume,
            _ => Hot::None,
        };
        let mut actions = Vec::new();
        let mut repaint = false;
        match *e {
            Event::Enter { .. } => {
                w.hover = true;
                repaint = true;
                actions.push(Action::Hover(true));
            }
            Event::Leave { .. } if w.press.is_none() => {
                w.hover = false;
                w.hot = Hot::None;
                repaint = true;
                actions.push(Action::Hover(false));
            }
            Event::Expose { .. } => repaint = true,
            Event::Motion { x, y, root_x, root_y, buttons, .. } => {
                if let (Some(Press::Volume(volume)), Some(slider), true) = (w.press.as_mut(), slider, buttons & BUTTON1_MASK != 0) {
                    // A repaint only for a new level: the mouse reports far more often.
                    let level = crate::view::volume_at(slider, y);
                    if *volume != level {
                        *volume = level;
                        w.volume = level;
                        repaint = true;
                    }
                } else if let (Some(p), true) = (w.press.as_mut(), buttons & BUTTON1_MASK != 0) {
                    match p.drag((root_x, root_y), (w.rect.w, w.rect.h)) {
                        Some(Drag::Window(nx, ny)) => {
                            w.rect.x = nx;
                            w.rect.y = ny;
                            self.x.move_to(w.xid, nx, ny);
                        }
                        Some(Drag::Picture(du, dv)) => actions.push(Action::Pan(du, dv)),
                        None => {}
                    }
                } else {
                    let h = hot_at(w, x, y);
                    if h != w.hot {
                        w.hot = h;
                        repaint = true;
                    }
                }
            }
            Event::Press { button: 1, x, y, root_x, root_y, time, state, .. } => match hot_at(w, x, y) {
                Hot::Close => actions.push(Action::Close),
                Hot::Sound => actions.push(Action::Sound),
                Hot::Volume => {
                    if let Some(slider) = slider {
                        w.volume = crate::view::volume_at(slider, y);
                        w.press = Some(Press::Volume(w.volume));
                        repaint = true;
                    }
                }
                // With Ctrl a zoomed picture moves instead of the window; that is no click,
                // however quickly the drags follow each other.
                Hot::None if w.zoomed && state & CONTROL_MASK != 0 => {
                    w.press = Some(Press::Picture { last: (root_x, root_y) });
                }
                Hot::None => {
                    let double =
                        w.last_click.is_some_and(|(t, cx, cy)| time.wrapping_sub(t) < 400 && (root_x - cx).abs() < 6 && (root_y - cy).abs() < 6);
                    if double {
                        w.last_click = None;
                        actions.push(Action::Open);
                    } else {
                        w.last_click = Some((time, root_x, root_y));
                        // Ordinary windows may have been placed elsewhere by the window manager.
                        if !w.on_top {
                            if let Some((px, py)) = self.x.position(w.xid) {
                                (w.rect.x, w.rect.y) = (px, py);
                            }
                        }
                        w.press = Some(Press::Window { root: (root_x, root_y), start: (w.rect.x, w.rect.y), moved: false });
                    }
                }
            },
            Event::Release { button: 1, x, y, .. } => {
                actions.extend(w.press.take().and_then(|p| p.released((w.rect.x - area.x, w.rect.y - area.y))));
                // Released outside the window: the leave event came during the press.
                if !inside((0, 0, w.rect.w as i32, w.rect.h as i32), x, y) && w.hover {
                    w.hover = false;
                    w.hot = Hot::None;
                    repaint = true;
                    actions.push(Action::Hover(false));
                }
            }
            Event::Release { button: 3, .. } => actions.push(Action::Close),
            // Ctrl + wheel: the picture zooms in (4) or out (5) around the mouse.
            Event::Press { button: button @ (4 | 5), x, y, state, .. } if state & CONTROL_MASK != 0 => {
                let (u, v) = (x as f32 / w.rect.w.max(1) as f32, y as f32 / w.rect.h.max(1) as f32);
                actions.push(Action::Zoom(if button == 4 { 1 } else { -1 }, u, v));
            }
            // The wheel: 4 = up (bigger), 5 = down (smaller).
            Event::Press { button: 4, .. } => actions.push(Action::Wheel(1)),
            Event::Press { button: 5, .. } => actions.push(Action::Wheel(-1)),
            _ => {}
        }
        if repaint {
            self.paint(id);
        }
        actions.into_iter().map(|a| (id, a)).collect()
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        let xids: Vec<u32> = self.wins.values().map(|w| w.xid).collect();
        for xid in xids {
            self.x.destroy(xid);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_the_picture_is_no_click() {
        let mut press = Press::Picture { last: (100, 50) };
        // A tenth of the window at once: it needs no few pixels first.
        assert_eq!(press.drag((148, 77), (480, 270)), Some(Drag::Picture(0.1, 0.1)));
        // From the last place on.
        assert_eq!(press.drag((148, 77), (480, 270)), Some(Drag::Picture(0.0, 0.0)));
        assert_eq!(press.released((0, 0)), None);
    }

    #[test]
    fn the_volume_slider_reports_when_let_go() {
        let mut press = Press::Volume(30);
        assert_eq!(press.drag((148, 77), (480, 270)), None);
        assert_eq!(press.released((0, 0)), Some(Action::Volume(30)));
    }

    #[test]
    fn moving_the_window_starts_after_a_few_pixels() {
        let mut press = Press::Window { root: (100, 100), start: (500, 300), moved: false };
        assert_eq!(press.drag((102, 102), (480, 270)), None);
        assert_eq!(press.released((0, 0)), Some(Action::Pin));
        assert_eq!(press.drag((110, 100), (480, 270)), Some(Drag::Window(510, 300)));
        // Back near the start it is still a move.
        assert_eq!(press.drag((101, 100), (480, 270)), Some(Drag::Window(501, 300)));
        assert_eq!(press.released((10, 20)), Some(Action::Moved(10, 20)));
    }

    #[test]
    fn buttons_like_on_windows() {
        // 480 px at 100 %: × 24 px, 8 px from the edges; the speaker 6 px to its left.
        assert_eq!(button_rects(480, 1.0), ((448, 8, 472, 32), (418, 8, 442, 32)));
        assert_eq!(button_rects(960, 2.0).0, (896, 16, 944, 64));
    }

    #[test]
    fn monitor_choice_and_panels() {
        let mons = [Monitor { x: 0, y: 0, w: 2560, h: 1440, primary: true }, Monitor { x: -1920, y: 0, w: 1920, h: 1080, primary: false }];
        assert_eq!(pick_monitor(&mons, "primary").map(|m| m.x), Some(0));
        // Numbers count from the left.
        assert_eq!(pick_monitor(&mons, "1").map(|m| m.x), Some(-1920));
        assert_eq!(pick_monitor(&mons, "2").map(|m| m.x), Some(0));
        assert_eq!(pick_monitor(&mons, "9").map(|m| m.x), Some(0));
        assert_eq!(pick_monitor(&[], "primary"), None);
        // One above the other (same x): numbered from the top, whatever order RandR gives.
        let stacked = [Monitor { x: 0, y: 1080, w: 1920, h: 1080, primary: false }, Monitor { x: 0, y: 0, w: 1920, h: 1080, primary: true }];
        assert_eq!(pick_monitor(&stacked, "1").map(|m| m.y), Some(0));
        assert_eq!(numbered(&stacked)[1].y, 1080);
        // A 32 px top bar on the primary monitor.
        assert_eq!(usable(mons[0], Some((0, 32, 2560, 1408))), Rect { x: 0, y: 32, w: 2560, h: 1408 });
        assert_eq!(usable(mons[0], None), Rect { x: 0, y: 0, w: 2560, h: 1440 });
        // A work area that hardly touches the monitor is ignored.
        assert_eq!(usable(mons[1], Some((0, 32, 2560, 1408))), Rect { x: -1920, y: 0, w: 1920, h: 1080 });
    }
}
