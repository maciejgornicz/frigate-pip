//! Translations. The language follows the Windows display language; when it is
//! not supported, English is used. Every language is a `Strings` value, so the
//! compiler rejects a translation with a missing text.
//!
//! Adding a language: copy `en.rs`, translate every value, add a `Lang` variant
//! and wire it in `Lang::ALL`, `code`, `native_name`, `strings` and `detect`.

mod de;
mod en;
mod es;
mod fr;
mod pl;
mod pt;

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lang {
    En,
    Pl,
    De,
    Es,
    Pt,
    Fr,
}

impl Lang {
    pub const ALL: [Lang; 6] = [Lang::En, Lang::Pl, Lang::De, Lang::Es, Lang::Pt, Lang::Fr];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Pl => "pl",
            Lang::De => "de",
            Lang::Es => "es",
            Lang::Pt => "pt",
            Lang::Fr => "fr",
        }
    }

    /// Name of the language in that language (for the language selector).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Pl => "Polski",
            Lang::De => "Deutsch",
            Lang::Es => "Español",
            Lang::Pt => "Português",
            Lang::Fr => "Français",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code().eq_ignore_ascii_case(code.trim()))
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &en::STRINGS,
            Lang::Pl => &pl::STRINGS,
            Lang::De => &de::STRINGS,
            Lang::Es => &es::STRINGS,
            Lang::Pt => &pt::STRINGS,
            Lang::Fr => &fr::STRINGS,
        }
    }

    /// Language for a Windows primary language id; English when unsupported.
    pub fn from_primary_langid(primary: u16) -> Lang {
        match primary {
            0x15 => Lang::Pl,
            0x07 => Lang::De,
            0x0A => Lang::Es,
            0x16 => Lang::Pt,
            0x0C => Lang::Fr,
            _ => Lang::En,
        }
    }
}

/// Language of the Windows user interface (English when unsupported).
pub fn detect() -> Lang {
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    Lang::from_primary_langid(id & 0x3FF)
}

/// `auto` (or empty) = Windows language, otherwise a language code.
pub fn resolve(setting: &str) -> Lang {
    Lang::from_code(setting).unwrap_or_else(detect)
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    let i = Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0);
    CURRENT.store(i as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

/// Texts in the current language.
pub fn t() -> &'static Strings {
    current().strings()
}

/// Fills `{0}`, `{1}`, … placeholders.
pub fn fmt(template: &str, args: &[&dyn Display]) -> String {
    let mut out = template.to_string();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &a.to_string());
    }
    out
}

/// Translated Frigate object label (unknown labels are returned unchanged).
pub fn label(l: &str) -> &str {
    t().labels.iter().find(|(k, _)| *k == l).map(|(_, v)| *v).unwrap_or(l)
}

/// All user-visible texts of the application.
pub struct Strings {
    // ---- tray icon, menu, status
    pub status_connected: &'static str,
    pub status_disconnected: &'static str,
    pub status_connecting: &'static str,
    pub status_paused_forever: &'static str,
    /// {0} = minutes left
    pub status_paused_minutes: &'static str,
    pub menu_settings: &'static str,
    pub menu_show_camera: &'static str,
    pub menu_no_cameras: &'static str,
    pub menu_pause: &'static str,
    pub menu_paused: &'static str,
    pub menu_pause_15: &'static str,
    pub menu_pause_60: &'static str,
    pub menu_pause_forever: &'static str,
    pub menu_resume: &'static str,
    pub menu_close_all: &'static str,
    pub menu_open_frigate: &'static str,
    pub menu_help: &'static str,
    pub menu_log: &'static str,
    pub menu_config_file: &'static str,
    pub menu_exit: &'static str,

    // ---- caption on the preview window
    pub overlay_pinned: &'static str,
    pub overlay_settings_preview: &'static str,

    // ---- settings window
    pub settings_title: &'static str,
    pub tab_connection: &'static str,
    pub tab_detection: &'static str,
    pub tab_window: &'static str,
    pub tab_video: &'static str,
    pub frigate_url: &'static str,
    pub source: &'static str,
    pub source_websocket: &'static str,
    pub source_mqtt: &'static str,
    pub mqtt_server: &'static str,
    pub port: &'static str,
    pub mqtt_login: &'static str,
    pub mqtt_prefix: &'static str,
    pub rtsp: &'static str,
    pub rtsp_hint: &'static str,
    pub test_button: &'static str,
    pub test_placeholder: &'static str,
    pub language: &'static str,
    pub language_auto: &'static str,
    pub autostart: &'static str,
    pub show_for: &'static str,
    pub mode_events: &'static str,
    pub mode_alerts: &'static str,
    pub mode_reviews: &'static str,
    pub cameras: &'static str,
    pub objects: &'static str,
    pub zones: &'static str,
    pub detection_note: &'static str,
    pub ignore_stationary: &'static str,
    pub min_score: &'static str,
    pub monitor: &'static str,
    pub monitor_primary: &'static str,
    /// {0} = number, {1} = width, {2} = height
    pub monitor_item: &'static str,
    pub monitor_primary_suffix: &'static str,
    pub corner: &'static str,
    pub corner_top_left: &'static str,
    pub corner_top_right: &'static str,
    pub corner_bottom_left: &'static str,
    pub corner_bottom_right: &'static str,
    pub width: &'static str,
    pub margin: &'static str,
    pub px: &'static str,
    pub duration: &'static str,
    pub duration_max: &'static str,
    pub seconds: &'static str,
    pub max_windows: &'static str,
    pub when_full: &'static str,
    pub replace_oldest: &'static str,
    pub skip_new: &'static str,
    pub topmost: &'static str,
    pub overlay: &'static str,
    pub sound: &'static str,
    pub skip_fullscreen: &'static str,
    pub window_hint: &'static str,
    pub live: &'static str,
    pub live_rtsp: &'static str,
    pub live_mjpeg: &'static str,
    pub live_snapshot: &'static str,
    pub stream: &'static str,
    pub stream_auto: &'static str,
    pub stream_sub: &'static str,
    pub stream_main: &'static str,
    pub auto_main_width: &'static str,
    pub auto_main_hint: &'static str,
    pub camera_audio: &'static str,
    pub prebuffer: &'static str,
    pub gpu_warm: &'static str,
    pub preview: &'static str,
    pub preview_live: &'static str,
    pub ok: &'static str,
    pub cancel: &'static str,
    pub apply: &'static str,
    pub help: &'static str,

    // ---- tooltips (one per option)
    pub tip_url: &'static str,
    pub tip_source: &'static str,
    pub tip_mqtt_server: &'static str,
    pub tip_mqtt_port: &'static str,
    pub tip_mqtt_login: &'static str,
    pub tip_mqtt_prefix: &'static str,
    pub tip_rtsp: &'static str,
    pub tip_test: &'static str,
    pub tip_language: &'static str,
    pub tip_autostart: &'static str,
    pub tip_mode: &'static str,
    pub tip_cameras: &'static str,
    pub tip_objects: &'static str,
    pub tip_zones: &'static str,
    pub tip_ignore_stationary: &'static str,
    pub tip_min_score: &'static str,
    pub tip_monitor: &'static str,
    pub tip_corner: &'static str,
    pub tip_width: &'static str,
    pub tip_margin: &'static str,
    pub tip_duration: &'static str,
    pub tip_max_duration: &'static str,
    pub tip_max_windows: &'static str,
    pub tip_when_full: &'static str,
    pub tip_topmost: &'static str,
    pub tip_overlay: &'static str,
    pub tip_sound: &'static str,
    pub tip_skip_fullscreen: &'static str,
    pub tip_live: &'static str,
    pub tip_stream: &'static str,
    pub tip_auto_main_width: &'static str,
    pub tip_camera_audio: &'static str,
    pub tip_prebuffer: &'static str,
    pub tip_gpu_warm: &'static str,
    pub tip_preview_camera: &'static str,
    pub tip_preview_live: &'static str,
    pub tip_help: &'static str,

    // ---- connection test
    /// {0} = version, {1} = ms
    pub test_frigate_ok: &'static str,
    /// {0} = address, {1} = error
    pub test_frigate_fail: &'static str,
    pub test_frigate_hint: &'static str,
    /// {0} = error
    pub test_config_fail: &'static str,
    /// {0} = count, {1} = names, {2} = ms
    pub test_cameras: &'static str,
    /// {0} = ms
    pub test_websocket_ok: &'static str,
    /// {0} = address, {1} = error
    pub test_websocket_fail: &'static str,
    /// {0} = host:port, {1} = ms
    pub test_mqtt_ok: &'static str,
    /// {0} = host:port, {1} = error
    pub test_mqtt_fail: &'static str,
    /// {0} = stream, {1} = codec, {2} = audio part, {3} = ms
    pub test_rtsp_ok: &'static str,
    /// {0} = sample rate
    pub test_audio: &'static str,
    pub test_no_audio: &'static str,
    /// {0} = address, {1} = error
    pub test_rtsp_fail: &'static str,
    pub test_done: &'static str,

    /// Frigate labels: (label, translation)
    pub labels: &'static [(&'static str, &'static str)],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_language_mapping_falls_back_to_english() {
        assert_eq!(Lang::from_primary_langid(0x15), Lang::Pl);
        assert_eq!(Lang::from_primary_langid(0x07), Lang::De);
        assert_eq!(Lang::from_primary_langid(0x0A), Lang::Es);
        assert_eq!(Lang::from_primary_langid(0x16), Lang::Pt);
        assert_eq!(Lang::from_primary_langid(0x0C), Lang::Fr);
        assert_eq!(Lang::from_primary_langid(0x09), Lang::En);
        assert_eq!(Lang::from_primary_langid(0x11), Lang::En); // japoński → angielski
        assert_eq!(resolve("de"), Lang::De);
        assert_eq!(Lang::from_code("PT"), Some(Lang::Pt));
        assert_eq!(Lang::from_code("auto"), None);
    }

    #[test]
    fn placeholders() {
        assert_eq!(fmt("{0} – {1}×{2}", &[&1, &1920, &1080]), "1 – 1920×1080");
        assert_eq!(fmt("{1} {0}", &[&"a", &"b"]), "b a");
    }

    #[test]
    fn every_language_has_complete_texts_and_placeholders() {
        let en = Lang::En.strings();
        for lang in Lang::ALL {
            let s = lang.strings();
            // każdy szablon z parametrami musi zachować te same {n}
            let pairs = [
                (en.status_paused_minutes, s.status_paused_minutes),
                (en.monitor_item, s.monitor_item),
                (en.test_frigate_ok, s.test_frigate_ok),
                (en.test_frigate_fail, s.test_frigate_fail),
                (en.test_config_fail, s.test_config_fail),
                (en.test_cameras, s.test_cameras),
                (en.test_websocket_ok, s.test_websocket_ok),
                (en.test_websocket_fail, s.test_websocket_fail),
                (en.test_mqtt_ok, s.test_mqtt_ok),
                (en.test_mqtt_fail, s.test_mqtt_fail),
                (en.test_rtsp_ok, s.test_rtsp_ok),
                (en.test_audio, s.test_audio),
                (en.test_rtsp_fail, s.test_rtsp_fail),
                (en.status_connected, s.status_connected),
                (en.status_disconnected, s.status_disconnected),
            ];
            for (a, b) in pairs {
                for i in 0..4 {
                    let p = format!("{{{i}}}");
                    assert_eq!(a.contains(&p), b.contains(&p), "{:?}: {b:?} vs {a:?}", lang);
                }
            }
            assert!(s.rtsp_hint.contains("{host}") && s.rtsp_hint.contains("{stream}"), "{:?}", lang);
            assert!(!s.settings_title.is_empty() && !s.labels.is_empty(), "{:?}", lang);
            for (k, _) in en.labels {
                assert!(s.labels.iter().any(|(l, _)| l == k), "{:?}: brak etykiety {k}", lang);
            }
        }
    }
}
