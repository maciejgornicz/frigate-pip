//! Translations. The language follows the Windows display language (on Linux: the
//! locale, `LC_ALL` / `LC_MESSAGES` / `LANG`); when it is not supported, English is used. Every language is a `Strings` value, so the
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

/// Language from the locale environment (`LC_ALL`, `LC_MESSAGES`, `LANG`, e.g. `pl_PL.UTF-8`).
#[cfg(not(windows))]
pub fn detect() -> Lang {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .and_then(|v| Lang::from_code(v.get(..2)?))
        .unwrap_or(Lang::En)
}

/// Language of the Windows user interface (English when unsupported).
#[cfg(windows)]
pub fn detect() -> Lang {
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    Lang::from_primary_langid(id & 0x3FF)
}

/// `auto` (or empty) = system language, otherwise a language code.
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
    pub frigate_login: &'static str,
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
    pub camera_volume: &'static str,
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
    pub tip_frigate_login: &'static str,
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
    pub tip_camera_volume: &'static str,
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
    /// {0} = user name
    pub test_login_ok: &'static str,
    pub test_login_hint: &'static str,
    pub test_cert_hint: &'static str,
    pub test_tls_public: &'static str,
    /// {0} = SHA-256 fingerprint
    pub test_tls_first_use: &'static str,
    /// {0} = SHA-256 fingerprint
    pub test_tls_remembered: &'static str,
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

    // ---- updates
    /// {0} = version
    pub menu_update: &'static str,
    pub check_updates: &'static str,
    pub update_check_now: &'static str,
    /// {0} = version
    pub update_install: &'static str,
    pub update_checking: &'static str,
    /// {0} = version
    pub update_latest: &'static str,
    /// {0} = version
    pub update_available: &'static str,
    pub update_downloading: &'static str,
    /// {0} = error
    pub update_failed: &'static str,
    /// {0} = error
    pub update_check_failed: &'static str,
    /// {0} = version
    pub update_balloon_title: &'static str,
    pub update_balloon_text: &'static str,
    pub tip_check_updates: &'static str,
    pub tip_update: &'static str,

    // ---- Linux console version
    /// Keys shown in the status line.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_keys: &'static str,
    /// Printed when the tmux mode starts.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_tmux_running: &'static str,
    /// Shown in the caption and the status line while the camera audio is muted (key m).
    #[cfg_attr(windows, allow(dead_code))]
    pub console_sound_off: &'static str,
    /// `--list`: {0} = Frigate address, {1} = number of cameras.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_list_title: &'static str,
    /// `--list`: label before a camera's objects.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_list_objects: &'static str,
    /// `--list`: label before a camera's zones.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_list_zones: &'static str,
    /// `--list`: how to use the names.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_list_hint: &'static str,
    /// Printed when the desktop windows start in a terminal.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_window_running: &'static str,
    /// A second start of the desktop windows.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_already_running: &'static str,
    /// `--quit` when nothing runs.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_not_running: &'static str,
    /// `--autostart on`: {0} = the desktop entry written.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_autostart_on: &'static str,
    /// `--autostart off`.
    #[cfg_attr(windows, allow(dead_code))]
    pub console_autostart_off: &'static str,
    /// Settings page: shown after saving.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_settings_saved: &'static str,
    /// Settings page: saving failed, {0} = the reason.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_settings_not_saved: &'static str,
    /// Settings page: the automatic language (from LANG).
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_language_auto: &'static str,
    /// Settings page: start with the desktop session.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_autostart: &'static str,
    /// Settings page: its description.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_tip_autostart: &'static str,
    /// Settings page: description of the language.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_tip_language: &'static str,
    /// Settings page: RTSP (decoded by ffmpeg).
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_live_rtsp: &'static str,
    /// Settings page: description of the live picture.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_tip_live: &'static str,
    /// Settings page: what the mouse does in a window.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_window_hint: &'static str,
    /// Settings page: description of the window width.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_tip_width: &'static str,
    /// Settings page: description of the notification sound.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_tip_sound: &'static str,
    /// Notification at the first start.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_notify_setup: &'static str,
    /// Notification when the desktop has no tray.
    #[cfg_attr(windows, allow(dead_code))]
    pub linux_notify_no_tray: &'static str,

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
        assert_eq!(Lang::from_primary_langid(0x11), Lang::En); // Japanese → English
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
            // every template with parameters must keep the same {n}
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
                (en.console_list_title, s.console_list_title),
                (en.console_autostart_on, s.console_autostart_on),
                (en.linux_settings_not_saved, s.linux_settings_not_saved),
                (en.status_connected, s.status_connected),
                (en.status_disconnected, s.status_disconnected),
                (en.menu_update, s.menu_update),
                (en.update_install, s.update_install),
                (en.update_latest, s.update_latest),
                (en.update_available, s.update_available),
                (en.update_failed, s.update_failed),
                (en.update_check_failed, s.update_check_failed),
                (en.update_balloon_title, s.update_balloon_title),
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
                assert!(s.labels.iter().any(|(l, _)| l == k), "{:?}: missing label {k}", lang);
            }
        }
    }
}
