//! Settings window: tabs, live camera preview, connection test.
//!
//! The window only reads and writes its controls; the logic (preview, saving,
//! testing) is handled by `App` on the UI thread.

use crate::config::{Config, Corner, LiveMode, Source, StreamPref, TriggerMode};
use crate::frigate::CameraInfo;
use crate::i18n::{self, Lang};
use crate::view::choices;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const CLASS: PCWSTR = w!("FrigatePiP.Settings");
pub const WM_APP_TEST_LINE: u32 = WM_APP + 30;

pub const ID_URL: i32 = 100;
pub const ID_SOURCE: i32 = 101;
pub const ID_MQTT_HOST: i32 = 102;
pub const ID_MQTT_PORT: i32 = 103;
pub const ID_MQTT_USER: i32 = 104;
pub const ID_MQTT_PASS: i32 = 105;
pub const ID_MQTT_PREFIX: i32 = 106;
pub const ID_RTSP: i32 = 107;
pub const ID_TEST: i32 = 108;
pub const ID_TEST_OUT: i32 = 109;
pub const ID_AUTOSTART: i32 = 110;
pub const ID_LANGUAGE: i32 = 111;
pub const ID_CHECK_UPDATES: i32 = 112;
pub const ID_UPDATE: i32 = 113;
pub const ID_UPDATE_STATUS: i32 = 114;
pub const ID_USER: i32 = 115;
pub const ID_PASS: i32 = 116;
pub const ID_MODE: i32 = 200;
pub const ID_CAMS: i32 = 201;
pub const ID_LABELS: i32 = 202;
pub const ID_ZONES: i32 = 203;
pub const ID_STATIONARY: i32 = 204;
pub const ID_MINSCORE: i32 = 205;
pub const ID_MINSCORE_LBL: i32 = 206;
/// Whose objects the object list shows: all cameras or one camera.
pub const ID_LABELS_FOR: i32 = 207;
/// "Same as all cameras": removes the shown camera's own object list.
pub const ID_LABELS_SAME: i32 = 208;
pub const ID_MONITOR: i32 = 300;
pub const ID_POSITION: i32 = 301;
pub const ID_WIDTH: i32 = 302;
pub const ID_WIDTH_LBL: i32 = 303;
pub const ID_MARGIN: i32 = 304;
pub const ID_DURATION: i32 = 305;
pub const ID_MAXDUR: i32 = 306;
pub const ID_MAXPOP: i32 = 307;
pub const ID_REPLACE: i32 = 308;
pub const ID_TOPMOST: i32 = 309;
pub const ID_OVERLAY: i32 = 310;
pub const ID_SOUND: i32 = 311;
pub const ID_FULLSCREEN: i32 = 312;
pub const ID_LIVE: i32 = 400;
pub const ID_STREAM: i32 = 401;
pub const ID_AUTOW: i32 = 402;
pub const ID_AUDIO: i32 = 403;
pub const ID_PREBUF: i32 = 404;
pub const ID_GPU: i32 = 405;
pub const ID_VOLUME: i32 = 406;
pub const ID_VOLUME_LBL: i32 = 407;
pub const ID_PREV_CAM: i32 = 500;
pub const ID_PREVIEW: i32 = 501;
pub const ID_APPLY: i32 = 502;
pub const ID_TAB: i32 = 503;
pub const ID_HELP: i32 = 504;

const TBM_GETPOS: u32 = WM_USER;
const SS_NOPREFIX: u32 = 0x80;
const SS_NOTIFY: u32 = 0x100;
const SS_ENDELLIPSIS: u32 = 0x4000;
const MAX_POPUPS: usize = 8;
/// Client area at 96 DPI.
const WIDTH: i32 = 680;
const HEIGHT: i32 = 580;
/// Right edge of the contents of a tab page.
const RIGHT: i32 = WIDTH - 28;
/// `GWLP_USERDATA` mark of controls drawn on a tab page.
const ON_PANE: isize = 0x5041_4E45;

/// Brush and colour of the tab page background (0 = unknown).
static PANE_BRUSH: AtomicIsize = AtomicIsize::new(0);
static PANE_COLOR: AtomicU32 = AtomicU32::new(0);

/// Data used to fill the window.
pub struct Init<'a> {
    pub cfg: &'a Config,
    pub autostart: bool,
    pub cameras: &'a [CameraInfo],
    pub monitors: Vec<String>,
    pub icon: HICON,
}

pub struct Settings {
    pub hwnd: HWND,
    font: HFONT,
    scale: f32,
    tab: HWND,
    tips: HWND,
    pages: Vec<(HWND, usize)>,
    cameras: Vec<(String, String)>,
    /// How many leading entries of `cameras` come from Frigate (the rest only from the settings).
    real_cameras: usize,
    labels: Vec<String>,
    zones: Vec<String>,
    monitors: usize,
    /// The object lists, as they were when the object list last showed another scope.
    objects: Objects,
    /// The scope shown in the object list: 0 = all cameras, n = `cameras[n - 1]`.
    scope: usize,
    /// Texts of the scope combo (a camera with its own list has " *").
    scope_texts: Vec<String>,
}

fn wide(s: &str) -> HSTRING {
    HSTRING::from(s)
}

/// Registers the settings window class (the window procedure lives in `App`).
pub fn register(hinst: HINSTANCE, proc_: WNDPROC) {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: proc_,
            hInstance: hinst,
            hCursor: LoadCursorW(HINSTANCE::default(), IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&wc);
    }
}

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency><dependentAssembly>
    <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
      processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
  </dependentAssembly></dependency>
</assembly>
"#;

/// Modern-looking controls (Common Controls 6) without a manifest in the exe:
/// an activation context from the system manifest of `shell32.dll` (resource 124),
/// or, when that is missing, from a manifest written next to the settings.
pub fn enable_visual_styles() {
    use windows::Win32::System::ApplicationInstallationAndServicing::{ActivateActCtx, CreateActCtxW, ACTCTXW};
    unsafe {
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let shell32 = wide(&format!("{sys}\\System32\\shell32.dll"));
        let from_shell = ACTCTXW {
            cbSize: std::mem::size_of::<ACTCTXW>() as u32,
            dwFlags: 0x8, // ACTCTX_FLAG_RESOURCE_NAME_VALID
            lpSource: PCWSTR(shell32.as_ptr()),
            lpResourceName: PCWSTR(124 as *const u16),
            ..Default::default()
        };
        let ctx = CreateActCtxW(&from_shell).or_else(|_| {
            let path = crate::util::app_dir().join("common-controls.manifest");
            let _ = std::fs::write(&path, MANIFEST);
            let file = HSTRING::from(path.as_os_str());
            let from_file = ACTCTXW { cbSize: std::mem::size_of::<ACTCTXW>() as u32, lpSource: PCWSTR(file.as_ptr()), ..Default::default() };
            CreateActCtxW(&from_file)
        });
        match ctx {
            Ok(h) => {
                let mut cookie = 0usize;
                if ActivateActCtx(h, &mut cookie).is_err() {
                    crate::log!("Cannot enable visual styles");
                }
            }
            Err(e) => crate::log!("No visual styles: {e}"),
        }
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES | ICC_LISTVIEW_CLASSES | ICC_BAR_CLASSES | ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
    }
}

fn mode_name(m: TriggerMode) -> &'static str {
    let t = i18n::t();
    match m {
        TriggerMode::Events => t.mode_events,
        TriggerMode::Alerts => t.mode_alerts,
        TriggerMode::Reviews => t.mode_reviews,
    }
}

fn corner_name(k: Corner) -> &'static str {
    let t = i18n::t();
    match k {
        Corner::TopLeft => t.corner_top_left,
        Corner::TopRight => t.corner_top_right,
        Corner::BottomLeft => t.corner_bottom_left,
        Corner::BottomRight => t.corner_bottom_right,
    }
}

fn live_name(l: LiveMode) -> &'static str {
    let t = i18n::t();
    match l {
        LiveMode::Rtsp => t.live_rtsp,
        LiveMode::Mjpeg => t.live_mjpeg,
        LiveMode::Snapshot => t.live_snapshot,
    }
}

fn stream_name(s: StreamPref) -> &'static str {
    let t = i18n::t();
    match s {
        StreamPref::Auto => t.stream_auto,
        StreamPref::Sub => t.stream_sub,
        StreamPref::Main => t.stream_main,
    }
}

/// Background for `WM_CTLCOLORSTATIC`: texts, check boxes and sliders on a tab page
/// get the colour of the page (with visual styles it is not the window colour).
pub fn pane_brush(ctl: HWND, hdc: HDC) -> Option<HBRUSH> {
    let brush = PANE_BRUSH.load(Ordering::Relaxed);
    if brush == 0 || unsafe { GetWindowLongPtrW(ctl, GWLP_USERDATA) } != ON_PANE {
        return None;
    }
    unsafe {
        SetBkColor(hdc, COLORREF(PANE_COLOR.load(Ordering::Relaxed)));
    }
    Some(HBRUSH(brush as _))
}

/// Colour of the tab page, read from the tab control drawn into memory.
fn sample_pane(tab: HWND) -> Option<COLORREF> {
    unsafe {
        let mut rc = RECT::default();
        GetClientRect(tab, &mut rc).ok()?;
        let screen = GetDC(tab);
        let dc = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, rc.right, rc.bottom);
        let old = SelectObject(dc, bmp);
        let _ = SendMessageW(tab, WM_PRINTCLIENT, WPARAM(dc.0 as usize), LPARAM((PRF_CLIENT | PRF_ERASEBKGND) as isize));
        let c = GetPixel(dc, rc.right / 2, rc.bottom - 8);
        SelectObject(dc, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(dc);
        ReleaseDC(tab, screen);
        // Black = nothing was drawn.
        (c.0 != CLR_INVALID && c.0 != 0).then_some(c)
    }
}

/// Address of the user guide in the current language.
pub fn help_url() -> String {
    format!("https://github.com/maciejgornicz/frigate-pip/blob/main/docs/{}/user-guide.md", i18n::current().code())
}

/// An object in the list: its name in the interface language, with Frigate's in brackets.
fn label_text(l: &str) -> String {
    let name = i18n::label(l);
    if name == l {
        l.to_string()
    } else {
        format!("{name} ({l})")
    }
}

/// The checked entries of a list: everything or nothing checked = everything (an empty
/// list, which also takes in cameras and objects added to Frigate later).
fn pick(names: &[String], checked: &[String]) -> Vec<String> {
    let on: Vec<String> = names.iter().filter(|n| checked.iter().any(|c| c.eq_ignore_ascii_case(n))).cloned().collect();
    if on.is_empty() || on.len() == names.len() {
        Vec::new()
    } else {
        on
    }
}

/// Objects as set in the window: for all cameras (empty = every object) and the own list of
/// each camera of the camera list (empty = none: the one for all cameras).
#[derive(Clone, Debug, Default, PartialEq)]
struct Objects {
    all: Vec<String>,
    own: Vec<Vec<String>>,
}

impl Objects {
    fn from_config(c: &Config, cameras: &[(String, String)]) -> Objects {
        Objects { all: c.labels.clone(), own: cameras.iter().map(|(n, _)| c.own_labels(n).map(<[String]>::to_vec).unwrap_or_default()).collect() }
    }

    /// The objects checked in the list for a scope: 0 = all cameras, n = the n-th camera.
    fn checks(&self, scope: usize, labels: &[String]) -> Vec<String> {
        let list = self.own.get(scope.wrapping_sub(1)).filter(|o| !o.is_empty()).unwrap_or(&self.all);
        labels.iter().filter(|l| list.is_empty() || list.iter().any(|x| x.eq_ignore_ascii_case(l))).cloned().collect()
    }

    /// With the checks of one scope changed; a camera list that is the same as the one for
    /// all cameras is none.
    fn with_checks(&self, scope: usize, checked: &[String], labels: &[String]) -> Objects {
        let mut o = self.clone();
        match scope {
            0 => o.all = pick(labels, checked),
            n => {
                if let Some(own) = o.own.get_mut(n - 1) {
                    *own = checked.to_vec();
                }
            }
        }
        for own in &mut o.own {
            *own = crate::view::own_labels(own, &o.all, labels);
        }
        o
    }

    fn apply(&self, c: &mut Config, cameras: &[(String, String)]) {
        c.labels = self.all.clone();
        for ((n, _), own) in cameras.iter().zip(&self.own) {
            c.set_own_labels(n, own.clone());
        }
    }
}

fn on_pane(h: HWND) -> HWND {
    unsafe {
        SetWindowLongPtrW(h, GWLP_USERDATA, ON_PANE);
    }
    h
}

impl Settings {
    pub fn create(hinst: HINSTANCE, init: Init) -> Option<Settings> {
        unsafe {
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
            let (mut dpi, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dpi, &mut dy);
            let scale = dpi.max(96) as f32 / 96.0;
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
            let ex = WS_EX_CONTROLPARENT;
            let mut rc = RECT { left: 0, top: 0, right: (WIDTH as f32 * scale) as i32, bottom: (HEIGHT as f32 * scale) as i32 };
            let _ = AdjustWindowRectExForDpi(&mut rc, style, false, ex, dpi);
            let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
            let work = mi.rcWork;
            let x = work.left + ((work.right - work.left) - w) / 2;
            let y = work.top + ((work.bottom - work.top) - h) / 2;
            let title = wide(i18n::t().settings_title);
            let hwnd = match CreateWindowExW(ex, CLASS, &title, style, x, y, w, h, HWND::default(), HMENU::default(), hinst, None) {
                Ok(h) => h,
                Err(e) => {
                    crate::log!("Cannot open the settings window: {e}");
                    return None;
                }
            };
            let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_SMALL as usize), LPARAM(init.icon.0 as isize));
            let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_BIG as usize), LPARAM(init.icon.0 as isize));

            let mut ncm = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
            let _ = SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS.0, ncm.cbSize, Some(&mut ncm as *mut _ as *mut _), 0, dpi);
            let font = CreateFontIndirectW(&ncm.lfMessageFont);

            // Hover help for every option (destroyed together with its owner window).
            let tips = CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                PCWSTR::null(),
                WINDOW_STYLE(WS_POPUP.0 | TTS_ALWAYSTIP | TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                hwnd,
                HMENU::default(),
                hinst,
                None,
            )
            .unwrap_or_default();
            let _ = SendMessageW(tips, TTM_SETMAXTIPWIDTH, WPARAM(0), LPARAM((400.0 * scale) as isize));
            let _ = SendMessageW(tips, TTM_SETDELAYTIME, WPARAM(TTDT_AUTOPOP as usize), LPARAM(30_000));

            let cfg = init.cfg;
            let cameras: Vec<(String, String)> = init.cameras.iter().map(|c| (c.name.clone(), c.title.clone())).collect();
            let (cameras_all, labels, zones) = choices(init.cameras, cfg);
            let objects = Objects::from_config(cfg, &cameras_all);

            let mut s = Settings {
                hwnd,
                font,
                scale,
                tab: HWND::default(),
                tips,
                pages: Vec::new(),
                cameras: cameras_all,
                real_cameras: init.cameras.len(),
                labels,
                zones,
                monitors: init.monitors.len(),
                objects,
                scope: 0,
                scope_texts: Vec::new(),
            };
            s.build(cfg, init.autostart, &init.monitors, &cameras);
            if let Some(color) = sample_pane(s.tab) {
                PANE_COLOR.store(color.0, Ordering::Relaxed);
                PANE_BRUSH.store(CreateSolidBrush(color).0 as isize, Ordering::Relaxed);
            }
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            Some(s)
        }
    }

    fn px(&self, v: i32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    /// Width of `text` in the window font, in 96-DPI pixels.
    fn text_w(&self, text: &str) -> i32 {
        let buf: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        unsafe {
            let dc = GetDC(self.hwnd);
            let old = SelectObject(dc, self.font);
            let _ = GetTextExtentPoint32W(dc, &buf, &mut size);
            SelectObject(dc, old);
            ReleaseDC(self.hwnd, dc);
        }
        (size.cx as f32 / self.scale).ceil() as i32
    }

    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, page: Option<usize>, class: PCWSTR, text: &str, style: u32, ex: u32, id: i32, x: i32, y: i32, w: i32, h: i32) -> HWND {
        let visible = if page.unwrap_or(0) == 0 { WS_VISIBLE.0 } else { 0 };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(ex),
                class,
                &wide(text),
                WINDOW_STYLE(WS_CHILD.0 | visible | style),
                self.px(x),
                self.px(y),
                self.px(w),
                self.px(h),
                self.hwnd,
                HMENU(id as isize as *mut _),
                HINSTANCE::default(),
                None,
            )
            .unwrap_or_default()
        };
        unsafe {
            let _ = SendMessageW(hwnd, WM_SETFONT, WPARAM(self.font.0 as usize), LPARAM(1));
        }
        if let Some(p) = page {
            self.pages.push((hwnd, p));
        }
        hwnd
    }

    /// Hover help shown over the given controls.
    fn tip(&self, ctls: &[HWND], text: &str) {
        let mut buf: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        for ctl in ctls {
            let ti = TTTOOLINFOW {
                cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
                uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                hwnd: self.hwnd,
                uId: ctl.0 as usize,
                lpszText: PWSTR(buf.as_mut_ptr()),
                ..Default::default()
            };
            unsafe {
                let _ = SendMessageW(self.tips, TTM_ADDTOOLW, WPARAM(0), LPARAM(&ti as *const _ as isize));
            }
        }
    }

    /// Field caption (it takes mouse messages, so its option's help shows over it too).
    fn label(&mut self, page: usize, text: &str, x: i32, y: i32, w: i32) -> HWND {
        let h = self.add(Some(page), WC_STATICW, text, SS_NOPREFIX | SS_NOTIFY | SS_ENDELLIPSIS, 0, -1, x, y + 3, w, 20);
        on_pane(h)
    }

    /// Text that may wrap onto several lines.
    fn note(&mut self, page: usize, text: &str, x: i32, y: i32, w: i32, h: i32) {
        on_pane(self.add(Some(page), WC_STATICW, text, SS_NOPREFIX, 0, -1, x, y, w, h));
    }

    #[allow(clippy::too_many_arguments)]
    fn edit(&mut self, page: usize, id: i32, text: &str, x: i32, y: i32, w: i32, extra: u32) -> HWND {
        self.add(Some(page), WC_EDITW, text, WS_TABSTOP.0 | ES_AUTOHSCROLL as u32 | extra, WS_EX_CLIENTEDGE.0, id, x, y, w, 23)
    }

    #[allow(clippy::too_many_arguments)]
    fn combo(&mut self, page: Option<usize>, id: i32, items: &[String], sel: usize, x: i32, y: i32, w: i32) -> HWND {
        let h = self.add(page, WC_COMBOBOXW, "", WS_TABSTOP.0 | WS_VSCROLL.0 | CBS_DROPDOWNLIST as u32, 0, id, x, y, w, 260);
        unsafe {
            for it in items {
                let _ = SendMessageW(h, CB_ADDSTRING, WPARAM(0), LPARAM(wide(it).as_ptr() as isize));
            }
            let _ = SendMessageW(h, CB_SETCURSEL, WPARAM(sel.min(items.len().saturating_sub(1))), LPARAM(0));
        }
        h
    }

    #[allow(clippy::too_many_arguments)]
    fn check(&mut self, page: Option<usize>, id: i32, text: &str, on: bool, x: i32, y: i32, w: i32) -> HWND {
        let h = self.add(page, WC_BUTTONW, text, WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32, 0, id, x, y, w, 22);
        unsafe {
            let _ = SendMessageW(h, BM_SETCHECK, WPARAM(if on { 1 } else { 0 }), LPARAM(0));
        }
        if page.is_some() {
            on_pane(h);
        }
        h
    }

    #[allow(clippy::too_many_arguments)]
    fn track(&mut self, page: usize, id: i32, min: i32, max: i32, pos: i32, x: i32, y: i32, w: i32) -> HWND {
        let h = self.add(Some(page), TRACKBAR_CLASSW, "", WS_TABSTOP.0, 0, id, x, y, w, 28);
        unsafe {
            let _ = SendMessageW(h, TBM_SETRANGEMIN, WPARAM(0), LPARAM(min as isize));
            let _ = SendMessageW(h, TBM_SETRANGEMAX, WPARAM(0), LPARAM(max as isize));
            let _ = SendMessageW(h, TBM_SETPAGESIZE, WPARAM(0), LPARAM(((max - min) / 20).max(1) as isize));
            let _ = SendMessageW(h, TBM_SETPOS, WPARAM(1), LPARAM(pos as isize));
        }
        on_pane(h)
    }

    #[allow(clippy::too_many_arguments)]
    fn checklist(&mut self, page: usize, id: i32, items: &[(String, bool)], x: i32, y: i32, w: i32, h: i32) -> HWND {
        let style = WS_TABSTOP.0 | LVS_REPORT | LVS_NOCOLUMNHEADER | LVS_SINGLESEL | LVS_SHOWSELALWAYS;
        let lv = self.add(Some(page), WC_LISTVIEWW, "", style, WS_EX_CLIENTEDGE.0, id, x, y, w, h);
        unsafe {
            let ex = LVS_EX_CHECKBOXES | LVS_EX_FULLROWSELECT;
            let _ = SendMessageW(lv, LVM_SETEXTENDEDLISTVIEWSTYLE, WPARAM(ex as usize), LPARAM(ex as isize));
            let col = LVCOLUMNW { mask: LVCF_WIDTH, cx: self.px(w - 24), ..Default::default() };
            let _ = SendMessageW(lv, LVM_INSERTCOLUMNW, WPARAM(0), LPARAM(&col as *const _ as isize));
        }
        self.fill_list(id, items);
        lv
    }

    fn fill_list(&self, id: i32, items: &[(String, bool)]) {
        let lv = self.item(id);
        unsafe {
            let _ = SendMessageW(lv, LVM_DELETEALLITEMS, WPARAM(0), LPARAM(0));
            for (i, (text, on)) in items.iter().enumerate() {
                let mut buf: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let item = LVITEMW { mask: LVIF_TEXT, iItem: i as i32, pszText: PWSTR(buf.as_mut_ptr()), ..Default::default() };
                let _ = SendMessageW(lv, LVM_INSERTITEMW, WPARAM(0), LPARAM(&item as *const _ as isize));
                let state = LVITEMW {
                    stateMask: LVIS_STATEIMAGEMASK,
                    state: LIST_VIEW_ITEM_STATE_FLAGS(if *on { 2 << 12 } else { 1 << 12 }),
                    ..Default::default()
                };
                let _ = SendMessageW(lv, LVM_SETITEMSTATE, WPARAM(i), LPARAM(&state as *const _ as isize));
            }
        }
    }

    fn build(&mut self, c: &Config, autostart: bool, monitors: &[String], cameras: &[(String, String)]) {
        let t = i18n::t();
        unsafe {
            self.tab = self.add(None, WC_TABCONTROLW, "", WS_VISIBLE.0 | WS_CLIPSIBLINGS.0 | WS_TABSTOP.0, 0, ID_TAB, 12, 12, WIDTH - 24, 516);
            for (i, name) in [t.tab_connection, t.tab_detection, t.tab_window, t.tab_video].iter().enumerate() {
                // "&" would underline the next letter.
                let mut buf: Vec<u16> = name.replace('&', "&&").encode_utf16().chain(std::iter::once(0)).collect();
                let item = TCITEMW { mask: TCIF_TEXT, pszText: PWSTR(buf.as_mut_ptr()), ..Default::default() };
                let _ = SendMessageW(self.tab, TCM_INSERTITEMW, WPARAM(i), LPARAM(&item as *const _ as isize));
            }
        }
        // The caption column is as wide as its longest text in this language.
        let captions = [
            t.frigate_url,
            t.frigate_login,
            t.source,
            t.mqtt_server,
            t.mqtt_login,
            t.mqtt_prefix,
            t.rtsp,
            t.language,
            t.show_for,
            t.min_score,
            t.monitor,
            t.corner,
            t.width,
            t.margin,
            t.duration,
            t.max_windows,
            t.when_full,
            t.live,
            t.stream,
            t.auto_main_width,
        ];
        let lx = 28;
        let lw = captions.iter().map(|s| self.text_w(s)).max().unwrap_or(0).clamp(150, 250) + 4;
        let cx = lx + lw + 10;
        let cw = RIGHT - cx;
        let full = RIGHT - lx;

        // ---- Connection
        let p = 0;
        let l = self.label(p, t.frigate_url, lx, 52, lw);
        let e = self.edit(p, ID_URL, &c.url, cx, 52, cw, 0);
        self.tip(&[l, e], t.tip_url);
        let half = (cw - 10) / 2;
        let l = self.label(p, t.frigate_login, lx, 84, lw);
        let user = self.edit(p, ID_USER, &c.user, cx, 84, half, 0);
        let pass = self.edit(p, ID_PASS, &c.password, cx + half + 10, 84, cw - half - 10, ES_PASSWORD as u32);
        self.tip(&[l, user, pass], t.tip_frigate_login);
        let l = self.label(p, t.source, lx, 116, lw);
        let sources = [t.source_websocket.to_string(), t.source_mqtt.to_string()];
        let e = self.combo(Some(p), ID_SOURCE, &sources, (c.source == Source::Mqtt) as usize, cx, 116, cw);
        self.tip(&[l, e], t.tip_source);
        let port_w = 64;
        let port_lw = self.text_w(t.port) + 4;
        let port_lx = RIGHT - port_w - 6 - port_lw;
        let l = self.label(p, t.mqtt_server, lx, 148, lw);
        let e = self.edit(p, ID_MQTT_HOST, &c.mqtt_host, cx, 148, port_lx - 10 - cx, 0);
        self.tip(&[l, e], t.tip_mqtt_server);
        let l = self.label(p, t.port, port_lx, 148, port_lw);
        let e = self.edit(p, ID_MQTT_PORT, &c.mqtt_port.to_string(), RIGHT - port_w, 148, port_w, ES_NUMBER as u32);
        self.tip(&[l, e], t.tip_mqtt_port);
        let l = self.label(p, t.mqtt_login, lx, 180, lw);
        let user = self.edit(p, ID_MQTT_USER, &c.mqtt_user, cx, 180, half, 0);
        let pass = self.edit(p, ID_MQTT_PASS, &c.mqtt_password, cx + half + 10, 180, cw - half - 10, ES_PASSWORD as u32);
        self.tip(&[l, user, pass], t.tip_mqtt_login);
        let l = self.label(p, t.mqtt_prefix, lx, 212, lw);
        let e = self.edit(p, ID_MQTT_PREFIX, &c.mqtt_prefix, cx, 212, half, 0);
        self.tip(&[l, e], t.tip_mqtt_prefix);
        let l = self.label(p, t.rtsp, lx, 244, lw);
        let e = self.edit(p, ID_RTSP, &c.rtsp, cx, 244, cw, 0);
        self.tip(&[l, e], t.tip_rtsp);
        self.note(p, t.rtsp_hint, cx, 270, cw, 30);
        let bw = self.text_w(t.test_button).max(120) + 32;
        let b = self.add(Some(p), WC_BUTTONW, t.test_button, WS_TABSTOP.0 | BS_PUSHBUTTON as u32, 0, ID_TEST, cx, 302, bw, 28);
        self.tip(&[b], t.tip_test);
        let out_style = (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32 | WS_VSCROLL.0;
        self.add(Some(p), WC_EDITW, t.test_placeholder, out_style, WS_EX_CLIENTEDGE.0, ID_TEST_OUT, lx, 338, full, 96);
        let l = self.label(p, t.language, lx, 444, lw);
        let mut langs = vec![t.language_auto.to_string()];
        langs.extend(Lang::ALL.iter().map(|l| l.native_name().to_string()));
        let lang = Lang::from_code(&c.language).and_then(|l| Lang::ALL.iter().position(|x| *x == l)).map_or(0, |i| i + 1);
        let e = self.combo(Some(p), ID_LANGUAGE, &langs, lang, cx, 444, cw.min(240));
        self.tip(&[l, e], t.tip_language);
        let e = self.check(Some(p), ID_AUTOSTART, t.autostart, autostart, lx, 474, full);
        self.tip(&[e], t.tip_autostart);
        // Updates: check box, status text and a button ("Check now" / "Update to x").
        let check_w = self.text_w(t.check_updates) + 26;
        let e = self.check(Some(p), ID_CHECK_UPDATES, t.check_updates, c.check_updates, lx, 502, check_w);
        self.tip(&[e], t.tip_check_updates);
        let bw = self.text_w(t.update_check_now).max(self.text_w(&i18n::fmt(t.update_install, &[&"10.10.10"]))) + 28;
        let b = self.add(Some(p), WC_BUTTONW, t.update_check_now, WS_TABSTOP.0 | BS_PUSHBUTTON as u32, 0, ID_UPDATE, RIGHT - bw, 500, bw, 26);
        self.tip(&[b], t.tip_update);
        let x = lx + check_w + 8;
        self.label(p, "", x, 502, RIGHT - bw - 8 - x);
        self.set_id_of_last(ID_UPDATE_STATUS);

        // ---- Detection
        let p = 1;
        let l = self.label(p, t.show_for, lx, 52, lw);
        let modes: Vec<String> = TriggerMode::ALL.iter().map(|m| mode_name(*m).to_string()).collect();
        let mode = TriggerMode::ALL.iter().position(|m| *m == c.mode).unwrap_or(0);
        let e = self.combo(Some(p), ID_MODE, &modes, mode, cx, 52, cw);
        self.tip(&[l, e], t.tip_mode);
        let colw = (full - 24) / 3;
        let (cams, labels, zones) = self.list_items(c);
        let lists =
            [(t.cameras, ID_CAMS, cams, t.tip_cameras), (t.objects, ID_LABELS, labels, t.tip_objects), (t.zones, ID_ZONES, zones, t.tip_zones)];
        for (i, (title, id, items, tip)) in lists.into_iter().enumerate() {
            let x = lx + i as i32 * (colw + 12);
            let l = self.label(p, title, x, 88, colw);
            if id != ID_LABELS {
                let e = self.checklist(p, id, &items, x, 110, colw, 200);
                self.tip(&[l, e], tip);
                continue;
            }
            // Objects: whose list is shown (all cameras or one camera), the list, and a
            // button that gives the shown camera the list for all cameras again.
            let e = self.combo(Some(p), ID_LABELS_FOR, &[], 0, x, 112, colw);
            self.tip(&[e], t.tip_objects_for);
            let e = self.checklist(p, id, &items, x, 142, colw, 168);
            self.tip(&[l, e], tip);
            let b = self.add(Some(p), WC_BUTTONW, t.objects_same_as_all, WS_TABSTOP.0 | BS_PUSHBUTTON as u32, 0, ID_LABELS_SAME, x, 316, colw, 26);
            self.tip(&[b], t.tip_objects_same_as_all);
        }
        self.update_objects(c);
        self.note(p, t.detection_note, lx, 352, full, 36);
        let e = self.check(Some(p), ID_STATIONARY, t.ignore_stationary, c.ignore_stationary, lx, 394, full);
        self.tip(&[e], t.tip_ignore_stationary);
        let l = self.label(p, t.min_score, lx, 430, lw);
        let e = self.track(p, ID_MINSCORE, 0, 100, (c.min_score * 100.0).round() as i32, cx, 428, cw - 60);
        self.label(p, "", RIGHT - 55, 430, 55);
        self.set_id_of_last(ID_MINSCORE_LBL);
        self.tip(&[l, e], t.tip_min_score);

        // ---- Preview window
        let p = 2;
        let l = self.label(p, t.monitor, lx, 52, lw);
        let mon_sel = c.monitor.parse::<usize>().ok().filter(|n| *n >= 1 && *n <= monitors.len()).unwrap_or(0);
        let mut mons = vec![t.monitor_primary.to_string()];
        mons.extend(monitors.iter().cloned());
        let e = self.combo(Some(p), ID_MONITOR, &mons, mon_sel, cx, 52, cw);
        self.tip(&[l, e], t.tip_monitor);
        let l = self.label(p, t.corner, lx, 84, lw);
        let corners: Vec<String> = Corner::ALL.iter().map(|k| corner_name(*k).to_string()).collect();
        let e = self.combo(Some(p), ID_POSITION, &corners, Corner::ALL.iter().position(|k| *k == c.position).unwrap_or(3), cx, 84, cw);
        self.tip(&[l, e], t.tip_corner);
        let l = self.label(p, t.width, lx, 118, lw);
        let e = self.track(p, ID_WIDTH, 240, 1920, c.width, cx, 116, cw - 60);
        self.label(p, "", RIGHT - 55, 118, 55);
        self.set_id_of_last(ID_WIDTH_LBL);
        self.tip(&[l, e], t.tip_width);
        let l = self.label(p, t.margin, lx, 152, lw);
        let e = self.edit(p, ID_MARGIN, &c.margin.to_string(), cx, 152, 70, ES_NUMBER as u32);
        self.label(p, t.px, cx + 76, 152, 40);
        self.tip(&[l, e], t.tip_margin);
        let l = self.label(p, t.duration, lx, 184, lw);
        let e = self.edit(p, ID_DURATION, &c.duration.to_string(), cx, 184, 70, ES_NUMBER as u32);
        self.tip(&[l, e], t.tip_duration);
        let mw = self.text_w(t.duration_max) + 6;
        let l = self.label(p, t.duration_max, cx + 76, 184, mw);
        let e = self.edit(p, ID_MAXDUR, &c.max_duration.to_string(), cx + 80 + mw, 184, 70, ES_NUMBER as u32);
        self.label(p, t.seconds, cx + 156 + mw, 184, 30);
        self.tip(&[l, e], t.tip_max_duration);
        let l = self.label(p, t.max_windows, lx, 216, lw);
        let counts: Vec<String> = (1..=MAX_POPUPS).map(|n| n.to_string()).collect();
        let e = self.combo(Some(p), ID_MAXPOP, &counts, c.max_popups.clamp(1, MAX_POPUPS) - 1, cx, 216, 70);
        self.tip(&[l, e], t.tip_max_windows);
        let l = self.label(p, t.when_full, lx, 248, lw);
        let rep = [t.replace_oldest.to_string(), t.skip_new.to_string()];
        let e = self.combo(Some(p), ID_REPLACE, &rep, (!c.replace) as usize, cx, 248, cw);
        self.tip(&[l, e], t.tip_when_full);
        let checks = [
            (ID_TOPMOST, t.topmost, c.topmost, t.tip_topmost),
            (ID_OVERLAY, t.overlay, c.overlay, t.tip_overlay),
            (ID_SOUND, t.sound, c.sound, t.tip_sound),
            (ID_FULLSCREEN, t.skip_fullscreen, c.skip_fullscreen, t.tip_skip_fullscreen),
        ];
        for (i, (id, text, on, tip)) in checks.into_iter().enumerate() {
            let e = self.check(Some(p), id, text, on, lx, 286 + 26 * i as i32, full);
            self.tip(&[e], tip);
        }
        self.note(p, t.window_hint, lx, 398, full, 36);

        // ---- Video & audio
        let p = 3;
        let l = self.label(p, t.live, lx, 52, lw);
        let lives: Vec<String> = LiveMode::ALL.iter().map(|m| live_name(*m).to_string()).collect();
        let e = self.combo(Some(p), ID_LIVE, &lives, LiveMode::ALL.iter().position(|l| *l == c.live).unwrap_or(0), cx, 52, cw);
        self.tip(&[l, e], t.tip_live);
        let l = self.label(p, t.stream, lx, 84, lw);
        let streams: Vec<String> = StreamPref::ALL.iter().map(|s| stream_name(*s).to_string()).collect();
        let e = self.combo(Some(p), ID_STREAM, &streams, StreamPref::ALL.iter().position(|s| *s == c.stream).unwrap_or(0), cx, 84, cw);
        self.tip(&[l, e], t.tip_stream);
        let l = self.label(p, t.auto_main_width, lx, 116, lw);
        let e = self.edit(p, ID_AUTOW, &c.auto_main_width.to_string(), cx, 116, 70, ES_NUMBER as u32);
        self.label(p, t.auto_main_hint, cx + 76, 116, RIGHT - cx - 76);
        self.tip(&[l, e], t.tip_auto_main_width);
        let checks = [
            (ID_AUDIO, t.camera_audio, c.camera_audio, t.tip_camera_audio),
            (ID_PREBUF, t.prebuffer, c.prebuffer, t.tip_prebuffer),
            (ID_GPU, t.gpu_warm, c.gpu_warm, t.tip_gpu_warm),
        ];
        for (i, (id, text, on, tip)) in checks.into_iter().enumerate() {
            // The volume slider of the camera audio comes right under its check box.
            let e = self.check(Some(p), id, text, on, lx, 154 + 26 * i as i32 + if i > 0 { 38 } else { 0 }, full);
            self.tip(&[e], tip);
        }
        let l = self.label(p, t.camera_volume, lx, 184, lw);
        let e = self.track(p, ID_VOLUME, 0, 100, c.camera_volume as i32, cx, 182, cw - 60);
        self.label(p, "", RIGHT - 55, 184, 55);
        self.set_id_of_last(ID_VOLUME_LBL);
        self.tip(&[l, e], t.tip_camera_volume);

        // ---- bottom of the window: preview on the left, buttons on the right
        let buttons = [(IDOK.0, t.ok, true), (IDCANCEL.0, t.cancel, false), (ID_APPLY, t.apply, false), (ID_HELP, t.help, false)];
        let widths: Vec<i32> = buttons.iter().map(|(_, text, _)| (self.text_w(text) + 24).max(72)).collect();
        let first = WIDTH - 12 - widths.iter().sum::<i32>() - 6 * (buttons.len() as i32 - 1);
        let pw = self.text_w(t.preview) + 4;
        self.add(None, WC_STATICW, t.preview, WS_VISIBLE.0, 0, -1, 12, 544, pw, 20);
        let live_w = self.text_w(t.preview_live) + 26;
        let combo_x = 12 + pw + 4;
        let combo_w = (first - 12 - live_w - 8 - combo_x).clamp(90, 180);
        let cam_names: Vec<String> =
            if cameras.is_empty() { vec![t.menu_no_cameras.into()] } else { cameras.iter().map(|(_, t)| t.clone()).collect() };
        let e = self.combo(None, ID_PREV_CAM, &cam_names, 0, combo_x, 540, combo_w);
        self.tip(&[e], t.tip_preview_camera);
        let e = self.check(None, ID_PREVIEW, t.preview_live, false, combo_x + combo_w + 8, 541, live_w);
        self.tip(&[e], t.tip_preview_live);
        let mut x = first;
        for ((id, text, default), w) in buttons.into_iter().zip(widths) {
            let style = WS_VISIBLE.0 | WS_TABSTOP.0 | if default { BS_DEFPUSHBUTTON } else { BS_PUSHBUTTON } as u32;
            let b = self.add(None, WC_BUTTONW, text, style, 0, id, x, 538, w, 28);
            if id == ID_HELP {
                self.tip(&[b], t.tip_help);
            }
            x += w + 6;
        }
        unsafe {
            let _ = SetWindowPos(self.tab, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        self.update_labels();
        self.update_enabled();
    }

    /// List entries (text, checked) for cameras, objects and zones according to `c`.
    #[allow(clippy::type_complexity)]
    fn list_items(&self, c: &Config) -> (Vec<(String, bool)>, Vec<(String, bool)>, Vec<(String, bool)>) {
        let all = |list: &[String], name: &str| list.is_empty() || list.iter().any(|x| x.eq_ignore_ascii_case(name));
        let cams = self.cameras.iter().map(|(n, t)| (t.clone(), all(&c.cameras, n))).collect();
        let labels = self.labels.iter().map(|l| (label_text(l), all(&c.labels, l))).collect();
        let zones = self.zones.iter().map(|z| (z.clone(), c.zones.iter().any(|x| x.eq_ignore_ascii_case(z)))).collect();
        (cams, labels, zones)
    }

    /// A new camera list from Frigate (e.g. the window was opened before connecting):
    /// refreshes the lists and the preview, keeping the current selections.
    pub fn set_cameras(&mut self, cams: &[CameraInfo], base: &Config) {
        let (draft, _) = self.read(base);
        let (cameras, labels, zones) = choices(cams, &draft);
        if cameras == self.cameras && labels == self.labels && zones == self.zones && self.real_cameras == cams.len() {
            return;
        }
        self.cameras = cameras;
        self.labels = labels;
        self.zones = zones;
        self.real_cameras = cams.len();
        // The object list starts again with all cameras.
        self.objects = Objects::from_config(&draft, &self.cameras);
        self.scope = 0;
        self.scope_texts.clear();
        self.update_objects(&draft);
        let (c, l, z) = self.list_items(&draft);
        self.fill_list(ID_CAMS, &c);
        self.fill_list(ID_LABELS, &l);
        self.fill_list(ID_ZONES, &z);
        let combo = self.item(ID_PREV_CAM);
        unsafe {
            let _ = SendMessageW(combo, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
            for c in cams {
                let _ = SendMessageW(combo, CB_ADDSTRING, WPARAM(0), LPARAM(wide(&c.title).as_ptr() as isize));
            }
            let _ = SendMessageW(combo, CB_SETCURSEL, WPARAM(0), LPARAM(0));
        }
    }

    fn set_id_of_last(&mut self, id: i32) {
        if let Some((h, _)) = self.pages.last() {
            unsafe {
                SetWindowLongPtrW(*h, GWLP_ID, id as isize);
            }
        }
    }

    fn item(&self, id: i32) -> HWND {
        unsafe { GetDlgItem(self.hwnd, id).unwrap_or_default() }
    }

    fn text(&self, id: i32) -> String {
        let h = self.item(id);
        unsafe {
            let n = GetWindowTextLengthW(h).max(0) as usize;
            let mut buf = vec![0u16; n + 1];
            let got = GetWindowTextW(h, &mut buf).max(0) as usize;
            String::from_utf16_lossy(&buf[..got]).trim().to_string()
        }
    }

    fn sel(&self, id: i32) -> usize {
        unsafe { SendMessageW(self.item(id), CB_GETCURSEL, WPARAM(0), LPARAM(0)).0.max(0) as usize }
    }

    pub fn checked(&self, id: i32) -> bool {
        unsafe { SendMessageW(self.item(id), BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1 }
    }

    pub fn set_checked(&self, id: i32, on: bool) {
        unsafe {
            let _ = SendMessageW(self.item(id), BM_SETCHECK, WPARAM(on as usize), LPARAM(0));
        }
    }

    fn pos(&self, id: i32) -> i32 {
        unsafe { SendMessageW(self.item(id), TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as i32 }
    }

    fn list_checks(&self, id: i32, n: usize) -> Vec<bool> {
        let lv = self.item(id);
        (0..n).map(|i| unsafe { (SendMessageW(lv, LVM_GETITEMSTATE, WPARAM(i), LPARAM(LVIS_STATEIMAGEMASK.0 as isize)).0 >> 12) == 2 }).collect()
    }

    /// The entries of `names` checked in a list (in the same order).
    fn checked_names(&self, id: i32, names: &[String]) -> Vec<String> {
        names.iter().zip(self.list_checks(id, names.len())).filter(|(_, on)| *on).map(|(n, _)| n.clone()).collect()
    }

    /// The object list shows another scope (all cameras or one camera).
    pub fn switch_scope(&mut self) {
        let new = self.sel(ID_LABELS_FOR);
        if new == self.scope {
            return;
        }
        self.objects = self.objects.with_checks(self.scope, &self.checked_names(ID_LABELS, &self.labels), &self.labels);
        self.scope = new;
        self.show_objects();
    }

    /// "Same as all cameras": the shown camera loses its own object list.
    pub fn same_as_all(&mut self) {
        if self.scope > 0 {
            self.objects = self.objects.with_checks(self.scope, &[], &self.labels);
            self.show_objects();
        }
    }

    fn show_objects(&self) {
        let on = self.objects.checks(self.scope, &self.labels);
        let items: Vec<(String, bool)> = self.labels.iter().map(|l| (label_text(l), on.contains(l))).collect();
        self.fill_list(ID_LABELS, &items);
    }

    /// The scope combo (" *" after a camera with its own object list) and the "Same as all
    /// cameras" button, which works only for such a camera.
    pub fn update_objects(&mut self, draft: &Config) {
        let texts: Vec<String> = std::iter::once(i18n::t().objects_all_cameras.to_string())
            .chain(self.cameras.iter().map(|(n, title)| if draft.own_labels(n).is_some() { format!("{title} *") } else { title.clone() }))
            .collect();
        let combo = self.item(ID_LABELS_FOR);
        if texts != self.scope_texts {
            unsafe {
                let _ = SendMessageW(combo, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
                for text in &texts {
                    let _ = SendMessageW(combo, CB_ADDSTRING, WPARAM(0), LPARAM(wide(text).as_ptr() as isize));
                }
                let _ = SendMessageW(combo, CB_SETCURSEL, WPARAM(self.scope), LPARAM(0));
            }
            self.scope_texts = texts;
        }
        let own = self.scope > 0 && self.cameras.get(self.scope - 1).is_some_and(|(n, _)| draft.own_labels(n).is_some());
        unsafe {
            let _ = EnableWindow(self.item(ID_LABELS_SAME), own);
        }
    }

    pub fn preview_camera(&self) -> Option<usize> {
        (self.real_cameras > 0).then(|| self.sel(ID_PREV_CAM))
    }

    /// Reads every control into a draft of the settings (+ autostart).
    pub fn read(&self, base: &Config) -> (Config, bool) {
        let mut c = base.clone();
        let num = |id: i32, def: i64| self.text(id).parse::<i64>().unwrap_or(def);
        c.url = self.text(ID_URL).trim().trim_end_matches('/').to_string();
        c.user = self.text(ID_USER).trim().to_string();
        c.password = self.text(ID_PASS);
        c.source = if self.sel(ID_SOURCE) == 1 { Source::Mqtt } else { Source::WebSocket };
        c.mqtt_host = self.text(ID_MQTT_HOST);
        c.mqtt_port = num(ID_MQTT_PORT, 1883).clamp(1, 65535) as u16;
        c.mqtt_user = self.text(ID_MQTT_USER);
        c.mqtt_password = self.text(ID_MQTT_PASS);
        c.mqtt_prefix = Some(self.text(ID_MQTT_PREFIX)).filter(|s| !s.is_empty()).unwrap_or_else(|| "frigate".into());
        c.rtsp = Some(self.text(ID_RTSP)).filter(|s| !s.is_empty()).unwrap_or_else(|| "rtsp://{host}:8554/{stream}".into());
        c.check_updates = self.checked(ID_CHECK_UPDATES);
        c.language = match self.sel(ID_LANGUAGE) {
            0 => "auto".into(),
            n => Lang::ALL.get(n - 1).map_or("auto", |l| l.code()).into(),
        };

        c.mode = TriggerMode::ALL[self.sel(ID_MODE).min(2)];
        let cam_names: Vec<String> = self.cameras.iter().map(|(n, _)| n.clone()).collect();
        c.cameras = pick(&cam_names, &self.checked_names(ID_CAMS, &cam_names));
        // The object list shows one scope; the others are as they were when last shown.
        self.objects.with_checks(self.scope, &self.checked_names(ID_LABELS, &self.labels), &self.labels).apply(&mut c, &self.cameras);
        c.zones = self.zones.iter().zip(self.list_checks(ID_ZONES, self.zones.len())).filter(|(_, b)| *b).map(|(z, _)| z.clone()).collect();
        c.ignore_stationary = self.checked(ID_STATIONARY);
        c.min_score = self.pos(ID_MINSCORE) as f32 / 100.0;

        let mon = self.sel(ID_MONITOR);
        c.monitor = if mon == 0 || mon > self.monitors { "primary".into() } else { mon.to_string() };
        c.position = Corner::ALL[self.sel(ID_POSITION).min(3)];
        c.width = self.pos(ID_WIDTH).clamp(160, 3840);
        c.margin = num(ID_MARGIN, 16).clamp(0, 500) as i32;
        c.duration = num(ID_DURATION, 15).clamp(1, 3600) as u32;
        c.max_duration = num(ID_MAXDUR, 120).clamp(1, 36000) as u32;
        c.max_popups = self.sel(ID_MAXPOP) + 1;
        c.replace = self.sel(ID_REPLACE) == 0;
        c.topmost = self.checked(ID_TOPMOST);
        c.overlay = self.checked(ID_OVERLAY);
        c.sound = self.checked(ID_SOUND);
        c.skip_fullscreen = self.checked(ID_FULLSCREEN);

        c.live = LiveMode::ALL[self.sel(ID_LIVE).min(2)];
        c.stream = StreamPref::ALL[self.sel(ID_STREAM).min(2)];
        c.auto_main_width = num(ID_AUTOW, 800).clamp(160, 7680) as i32;
        c.camera_audio = self.checked(ID_AUDIO);
        c.camera_volume = self.pos(ID_VOLUME) as u8;
        c.prebuffer = self.checked(ID_PREBUF);
        c.gpu_warm = self.checked(ID_GPU);
        (c, self.checked(ID_AUTOSTART))
    }

    /// Index of the open tab.
    pub fn page(&self) -> usize {
        unsafe { SendMessageW(self.tab, TCM_GETCURSEL, WPARAM(0), LPARAM(0)).0.max(0) as usize }
    }

    pub fn set_page(&self, page: usize) {
        unsafe {
            let _ = SendMessageW(self.tab, TCM_SETCURSEL, WPARAM(page), LPARAM(0));
        }
        self.show_page();
    }

    pub fn show_page(&self) {
        let cur = self.page();
        for (h, p) in &self.pages {
            unsafe {
                let _ = ShowWindow(*h, if *p == cur { SW_SHOW } else { SW_HIDE });
            }
        }
    }

    pub fn update_labels(&self) {
        unsafe {
            let _ = SetWindowTextW(self.item(ID_WIDTH_LBL), &wide(&format!("{} px", self.pos(ID_WIDTH))));
            let _ = SetWindowTextW(self.item(ID_MINSCORE_LBL), &wide(&format!("{}%", self.pos(ID_MINSCORE))));
            let _ = SetWindowTextW(self.item(ID_VOLUME_LBL), &wide(&format!("{}%", self.pos(ID_VOLUME))));
        }
    }

    pub fn update_enabled(&self) {
        let mqtt = self.sel(ID_SOURCE) == 1;
        let auto = self.sel(ID_STREAM) == 0;
        unsafe {
            for id in [ID_MQTT_HOST, ID_MQTT_PORT, ID_MQTT_USER, ID_MQTT_PASS, ID_MQTT_PREFIX] {
                let _ = EnableWindow(self.item(id), mqtt);
            }
            let _ = EnableWindow(self.item(ID_AUTOW), auto);
        }
    }

    /// Update status text and the update button ("Check now" or "Update to x").
    pub fn set_update(&self, st: &crate::update::Status, available: Option<&str>) {
        use crate::update::Status;
        let t = i18n::t();
        let text = match st {
            Status::Idle => String::new(),
            Status::Checking => t.update_checking.to_string(),
            Status::Latest => i18n::fmt(t.update_latest, &[&crate::update::current_version()]),
            Status::Available(v) => i18n::fmt(t.update_available, &[v]),
            Status::Downloading => t.update_downloading.to_string(),
            Status::CheckFailed(e) => i18n::fmt(t.update_check_failed, &[e]),
            Status::InstallFailed(e) => i18n::fmt(t.update_failed, &[e]),
        };
        let button = available.map_or_else(|| t.update_check_now.to_string(), |v| i18n::fmt(t.update_install, &[&v]));
        let busy = matches!(st, Status::Checking | Status::Downloading);
        unsafe {
            let _ = SetWindowTextW(self.item(ID_UPDATE_STATUS), &wide(&text));
            let _ = SetWindowTextW(self.item(ID_UPDATE), &wide(&button));
            let _ = EnableWindow(self.item(ID_UPDATE), !busy);
        }
    }

    pub fn clear_test(&self) {
        unsafe {
            let _ = SetWindowTextW(self.item(ID_TEST_OUT), w!(""));
            let _ = EnableWindow(self.item(ID_TEST), false);
        }
    }

    pub fn append_test(&self, line: &str) {
        let h = self.item(ID_TEST_OUT);
        unsafe {
            let len = GetWindowTextLengthW(h).max(0) as usize;
            let _ = SendMessageW(h, EM_SETSEL, WPARAM(len), LPARAM(len as isize));
            let text = if len == 0 { line.to_string() } else { format!("\r\n{line}") };
            let _ = SendMessageW(h, EM_REPLACESEL, WPARAM(0), LPARAM(wide(&text).as_ptr() as isize));
            // The test ends with "Done." or with a hint after a failure.
            if line == i18n::t().test_done || line.starts_with("   ") {
                let _ = EnableWindow(self.item(ID_TEST), true);
            }
        }
    }
}

impl Drop for Settings {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            let _ = DeleteObject(self.font);
            let brush = PANE_BRUSH.swap(0, Ordering::Relaxed);
            if brush != 0 {
                let _ = DeleteObject(HBRUSH(brush as _));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IniDoc;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn cams() -> Vec<(String, String)> {
        vec![("gate".into(), "Gate".into()), ("yard".into(), "Yard".into())]
    }

    fn labels() -> Vec<String> {
        names(&["person", "car", "dog"])
    }

    #[test]
    fn checks_shown_for_every_scope() {
        let c = Config::from_ini(&IniDoc::parse("[trigger]\nlabels = person\n[camera.Yard]\nlabels = dog\n"));
        let o = Objects::from_config(&c, &cams());
        // 0 = all cameras, then the cameras: their own list, or the one for all cameras.
        assert_eq!(
            (o.checks(0, &labels()), o.checks(1, &labels()), o.checks(2, &labels())),
            (names(&["person"]), names(&["person"]), names(&["dog"]))
        );
        // Every object for all cameras: all checked.
        let o = Objects::from_config(&Config::from_ini(&IniDoc::parse("")), &cams());
        assert_eq!(o.checks(0, &labels()), labels());
    }

    #[test]
    fn cameras_without_a_list_follow_the_one_for_all_cameras() {
        let c = Config::from_ini(&IniDoc::parse("[trigger]\nlabels = person\n[camera.yard]\nlabels = dog\n"));
        let o = Objects::from_config(&c, &cams());
        // Gate shown, not changed, then the list for all cameras changed: gate follows it.
        let o = o.with_checks(1, &names(&["person"]), &labels()).with_checks(0, &names(&["person", "car"]), &labels());
        assert_eq!(o.checks(1, &labels()), names(&["person", "car"]));
        assert_eq!(o.own, [names(&[]), names(&["dog"])]);
        // A camera list made the same as for all cameras is none.
        assert_eq!(o.with_checks(2, &names(&["car", "person"]), &labels()).own[1], names(&[]));
        // Nothing checked for a camera: none either.
        assert_eq!(o.with_checks(2, &[], &labels()).own[1], names(&[]));
        assert_eq!(o.with_checks(1, &names(&["dog"]), &labels()).own[0], names(&["dog"]));
    }

    #[test]
    fn objects_back_into_the_settings() {
        let mut c = Config::from_ini(&IniDoc::parse("[camera.yard]\nlabels = dog\n[camera.old]\nlabels = car\n"));
        let o = Objects::from_config(&c, &cams()).with_checks(1, &names(&["person"]), &labels()).with_checks(2, &labels(), &labels());
        o.apply(&mut c, &cams());
        // Everything checked for yard is the same as for all cameras (every object).
        assert!(c.labels.is_empty());
        assert_eq!((c.own_labels("gate"), c.own_labels("yard")), (Some(&names(&["person"])[..]), None));
        // A camera that is not in the list keeps its own.
        assert_eq!(c.own_labels("old"), Some(&names(&["car"])[..]));
        let o = o.with_checks(0, &names(&["person", "car"]), &labels());
        o.apply(&mut c, &cams());
        assert_eq!(c.labels, names(&["person", "car"]));
    }
}
