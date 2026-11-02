//! UI thread: tray icon, menu, PiP windows.

use crate::config::{self, Config, Corner, IniDoc, LiveMode};
use crate::filter::Detection;
use crate::frigate::{self, CameraInfo, Notice};
use crate::hub::Hub;
use crate::i18n::{self, t};
use crate::player::{self, Shared, WM_APP_ASPECT, WM_APP_PLAYER_DONE};
use crate::settings::{self, Settings};
use crate::update::{self, Release};
use crate::util::Cancel;
use crate::view::{choose_slot, choose_stream, Slot};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const MAIN_CLASS: PCWSTR = w!("FrigatePiP.Main");
const POPUP_CLASS: PCWSTR = w!("FrigatePiP.Popup");
const WM_APP_NOTICE: u32 = WM_APP + 1;
const WM_APP_TRAY: u32 = WM_APP + 2;
const WM_APP_RELOAD: u32 = WM_APP + 3;
pub const WM_APP_QUIT: u32 = WM_APP + 4;
pub const WM_APP_SETTINGS: u32 = WM_APP + 5;
const WM_APP_UPDATE: u32 = WM_APP + 6;
pub const COPYDATA_TEST: usize = 0x4650_5450;

const TIMER_DEADLINE: usize = 1;
const TIMER_FORCE_CLOSE: usize = 2;
const TIMER_RESUME: usize = 3;
const TIMER_TEST: usize = 4;
const TIMER_UPDATE: usize = 5;

const CMD_TEST: u32 = 1000;
const CMD_PAUSE_15: u32 = 1100;
const CMD_PAUSE_60: u32 = 1101;
const CMD_PAUSE_FOREVER: u32 = 1102;
const CMD_RESUME: u32 = 1103;
const CMD_SETTINGS: u32 = 1700;
const CMD_CLOSE_ALL: u32 = 1800;
const CMD_OPEN_FRIGATE: u32 = 1801;
const CMD_HELP: u32 = 1803;
const CMD_UPDATE: u32 = 1805;
const CMD_EDIT: u32 = 1802;
const CMD_LOG: u32 = 1804;
const CMD_EXIT: u32 = 1999;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// Settings window (for `IsDialogMessage` in the message loop: Tab, Enter, Esc).
static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Access to the app state from window procedures. On a nested call
/// (e.g. a message sent synchronously during `SetWindowPos`) returns `None`.
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| match a.try_borrow_mut() {
        Ok(mut g) => g.as_mut().map(f),
        Err(_) => None,
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Pause {
    Off,
    Until(Instant),
    Forever,
}

struct Popup {
    hwnd: HWND,
    camera: String,
    base_text: String,
    deadline: Instant,
    hard_deadline: Instant,
    hovered: bool,
    pinned: bool,
    closing: bool,
    user_moved: bool,
    width: Option<i32>,
    aspect: f32,
    shared: Arc<Shared>,
    /// Preview from the settings window: not counted toward the limit and never replaced.
    preview: bool,
}

struct Mon {
    work: RECT,
    rect: RECT,
    primary: bool,
    dpi: u32,
}

pub struct App {
    hwnd: HWND,
    hinst: HINSTANCE,
    cfg: Config,
    doc: IniDoc,
    path: PathBuf,
    cameras: Vec<CameraInfo>,
    connected: bool,
    status: String,
    pause: Pause,
    popups: Vec<Popup>,
    listener: Cancel,
    listener_key: String,
    hubs: HashMap<String, Arc<Hub>>,
    icons: [HICON; 3],
    pending_test: Option<String>,
    /// D3D11 device kept alive in the background so the GPU driver stays loaded
    /// and a window shows a picture without the ~150 ms cold start.
    gpu_warm: Option<windows::Win32::Graphics::Direct3D11::ID3D11Device>,
    settings: Option<Settings>,
    /// Unsaved draft from the settings window — display uses it live.
    view: Option<Config>,
    /// A newer release found on GitHub.
    update: Option<Release>,
    update_status: update::Status,
    /// Version already announced with a notification (once per run).
    update_notified: Option<String>,
    /// The last notification offers the update (a click installs it).
    update_balloon: bool,
}

// ---------------------------------------------------------------- helpers

fn wide(s: &str) -> HSTRING {
    HSTRING::from(s)
}

fn open_url(url: &str) {
    unsafe {
        ShellExecuteW(HWND::default(), w!("open"), &wide(url), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Result of the background update work, sent to the UI thread.
enum UpdateEvent {
    Checked { manual: bool, result: Result<Release, String> },
    InstallFailed(String),
}

fn post_update_event(hwnd: isize, ev: UpdateEvent) {
    let b = Box::into_raw(Box::new(ev));
    unsafe {
        if PostMessageW(HWND(hwnd as _), WM_APP_UPDATE, WPARAM(0), LPARAM(b as isize)).is_err() {
            drop(Box::from_raw(b));
        }
    }
}

fn open_in_notepad(path: &std::path::Path) {
    unsafe {
        ShellExecuteW(HWND::default(), w!("open"), w!("notepad.exe"), &HSTRING::from(path.as_os_str()), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

fn monitors() -> Vec<Mon> {
    unsafe extern "system" fn cb(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let v = &mut *(data.0 as *mut Vec<Mon>);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(h, &mut mi).as_bool() {
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            v.push(Mon { work: mi.rcWork, rect: mi.rcMonitor, primary: mi.dwFlags & 1 != 0, dpi: dx.max(96) });
        }
        TRUE
    }
    let mut v: Vec<Mon> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(HDC::default(), None, Some(cb), LPARAM(&mut v as *mut _ as isize));
    }
    v.sort_by_key(|m| (m.rect.left, m.rect.top));
    v
}

fn fullscreen_active() -> bool {
    matches!(unsafe { SHQueryUserNotificationState() }, Ok(QUNS_BUSY) | Ok(QUNS_RUNNING_D3D_FULL_SCREEN) | Ok(QUNS_PRESENTATION_MODE))
}

fn is_autostart() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, w!("FrigatePiP"), RRF_RT_REG_SZ, None, None, None) == ERROR_SUCCESS }
}

fn set_autostart(on: bool) {
    unsafe {
        if on {
            let exe = std::env::current_exe().unwrap_or_default();
            let val: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain(std::iter::once(0)).collect();
            let r = RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, w!("FrigatePiP"), REG_SZ.0, Some(val.as_ptr() as *const _), (val.len() * 2) as u32);
            crate::log!("Autostart enabled ({:?})", r);
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, w!("FrigatePiP"));
            crate::log!("Autostart disabled");
        }
    }
}

/// Placement of a new window when replacing: index in the queue, position (if
/// the old one was moved manually) and width.
struct Placement {
    index: usize,
    moved_to: Option<RECT>,
    width: Option<i32>,
}

fn listener_key(c: &Config) -> String {
    format!(
        "{}|{:?}|{}|{}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{}|{}",
        c.url,
        c.source,
        c.mqtt_host,
        c.mqtt_port,
        c.mqtt_user,
        c.mqtt_password,
        c.mqtt_prefix,
        c.mode,
        c.cameras,
        c.labels,
        c.zones,
        c.ignore_stationary,
        c.min_score
    )
}

fn watch_config(path: PathBuf, hwnd: isize) {
    let _ = std::thread::Builder::new().name("config-watch".into()).spawn(move || unsafe {
        let Some(dir) = path.parent() else { return };
        let Ok(h) =
            FindFirstChangeNotificationW(&HSTRING::from(dir.as_os_str()), false, FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME)
        else {
            return;
        };
        let mtime = || std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let mut last = mtime();
        loop {
            if WaitForSingleObject(h, INFINITE) != WAIT_OBJECT_0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
            let m = mtime();
            if m != last {
                last = m;
                let _ = PostMessageW(HWND(hwnd as _), WM_APP_RELOAD, WPARAM(0), LPARAM(0));
            }
            if FindNextChangeNotification(h).is_err() {
                break;
            }
        }
    });
}

// ---------------------------------------------------------------- start

pub fn run(test: Option<String>, open_settings: bool) {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap_or_default().into();
        let main_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(main_proc),
            hInstance: hinst,
            lpszClassName: MAIN_CLASS,
            ..Default::default()
        };
        RegisterClassExW(&main_class);
        let popup_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(popup_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(HINSTANCE::default(), IDC_HAND).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            lpszClassName: POPUP_CLASS,
            ..Default::default()
        };
        RegisterClassExW(&popup_class);
        settings::register(hinst, Some(settings_proc));
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            MAIN_CLASS,
            w!("FrigatePiP"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            HWND::default(),
            HMENU::default(),
            hinst,
            None,
        ) else {
            crate::log!("Cannot create the main window");
            return;
        };

        let path = config::config_path();
        // First start: no settings file yet — open the settings window to set up Frigate.
        let first_start = !path.exists();
        let mut loaded = config::load(&path);
        for _ in 0..20 {
            if loaded.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            loaded = config::load(&path);
        }
        let (cfg, doc) = loaded.unwrap_or_else(|e| {
            crate::log!("Cannot read {}: {e} - using defaults (file left untouched)", path.display());
            let doc = IniDoc::parse(config::DEFAULT_INI);
            (Config::from_ini(&doc), doc)
        });
        i18n::set(i18n::resolve(&cfg.language));
        crate::log!("Settings: {} (Frigate {}, language {})", path.display(), cfg.url, i18n::current().code());
        let icon_size = GetSystemMetrics(SM_CXSMICON).max(16);
        let icons = [
            crate::icon::make(icon_size, (0x3B, 0x82, 0xF6)),
            crate::icon::make(icon_size, (0x94, 0x94, 0x94)),
            crate::icon::make(icon_size, (0xF5, 0x9E, 0x0B)),
        ];
        let app = App {
            hwnd,
            hinst,
            cfg,
            doc,
            path: path.clone(),
            cameras: Vec::new(),
            connected: false,
            status: "…".into(),
            pause: Pause::Off,
            popups: Vec::new(),
            listener: Cancel::new(),
            listener_key: String::new(),
            hubs: HashMap::new(),
            icons,
            pending_test: None,
            gpu_warm: None,
            settings: None,
            view: None,
            update: None,
            update_status: update::Status::Idle,
            update_notified: None,
            update_balloon: false,
        };
        APP.with(|a| *a.borrow_mut() = Some(app));
        with_app(|a| {
            a.tray(NIM_ADD);
            a.start_listener();
            a.update_gpu_warm();
            // The first update check a minute after the start, then once a day.
            SetTimer(a.hwnd, TIMER_UPDATE, 60_000, None);
            if let Some(t) = test {
                a.test(&t);
            }
            if open_settings || first_start {
                a.open_settings();
            }
        });
        watch_config(path, hwnd.0 as isize);
        crate::util::trim_memory();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let dlg = SETTINGS_HWND.load(Ordering::Relaxed);
            // F1 anywhere in the settings window opens the user guide.
            if dlg != 0
                && msg.message == WM_KEYDOWN
                && msg.wParam.0 == VK_F1.0 as usize
                && (msg.hwnd.0 as isize == dlg || IsChild(HWND(dlg as _), msg.hwnd).as_bool())
            {
                open_url(&settings::help_url());
                continue;
            }
            if dlg != 0 && IsDialogMessageW(HWND(dlg as _), &msg).as_bool() {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let app = APP.with(|a| a.borrow_mut().take());
        if let Some(mut app) = app {
            app.shutdown();
        }
    }
}

// ---------------------------------------------------------------- window procedures

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_APP_NOTICE => {
            let n = Box::from_raw(lp.0 as *mut Notice);
            with_app(|a| a.on_notice(*n));
            LRESULT(0)
        }
        WM_APP_TRAY => {
            match (lp.0 as u32) & 0xFFFF {
                WM_LBUTTONUP => {
                    with_app(|a| a.open_settings());
                }
                WM_RBUTTONUP => show_menu(hwnd),
                NIN_BALLOONUSERCLICK => {
                    with_app(|a| a.balloon_clicked());
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_APP_RELOAD => {
            with_app(|a| a.reload());
            LRESULT(0)
        }
        WM_APP_QUIT => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_APP_SETTINGS => {
            with_app(|a| a.open_settings());
            LRESULT(0)
        }
        WM_APP_UPDATE => {
            let ev = Box::from_raw(lp.0 as *mut UpdateEvent);
            with_app(|a| a.on_update_event(*ev));
            LRESULT(0)
        }
        WM_COPYDATA => {
            let cds = &*(lp.0 as *const COPYDATASTRUCT);
            if cds.dwData == COPYDATA_TEST && !cds.lpData.is_null() {
                let bytes = std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize);
                let cam = String::from_utf8_lossy(bytes).into_owned();
                with_app(|a| a.test(&cam));
            }
            LRESULT(1)
        }
        WM_TIMER => {
            let _ = KillTimer(hwnd, wp.0);
            match wp.0 {
                TIMER_RESUME => {
                    with_app(|a| a.set_pause(Pause::Off));
                }
                TIMER_TEST => {
                    with_app(|a| {
                        if let Some(t) = a.pending_test.take() {
                            a.test_now(&t);
                        }
                    });
                }
                TIMER_UPDATE => {
                    with_app(|a| a.on_update_timer());
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            with_app(|a| a.layout());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            with_app(|a| a.tray(NIM_ADD));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let Some(menu) = with_app(|a| a.build_menu()) else { return };
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, None);
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
    if cmd.0 > 0 {
        with_app(|a| a.on_command(cmd.0 as u32));
    }
}

fn window_rect(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut r);
    }
    r
}

/// Hit-test code of the sound button: one without default behaviour (HTHELP, for example,
/// would start the "?" context help mode and swallow the next click).
const HT_SOUND: u32 = HTOBJECT;

unsafe extern "system" fn popup_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => {
            // The close button in the top-right corner, the sound button left of it (only with
            // camera audio); the rest of the window drags it.
            let mut pt = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
            let _ = ScreenToClient(hwnd, &mut pt);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let dpi = GetDpiForWindow(hwnd);
            let inside = |(l, t, r, b): (i32, i32, i32, i32)| pt.x >= l && pt.x < r && pt.y >= t && pt.y < b;
            let hit = if inside(crate::render::close_button_rect(rc.right, dpi)) {
                HTCLOSE
            } else if inside(crate::render::sound_button_rect(rc.right, dpi)) && with_app(|a| a.has_sound(hwnd)) == Some(true) {
                HT_SOUND
            } else {
                HTCAPTION
            };
            LRESULT(hit as isize)
        }
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wp.0 as u32 == HTCLOSE => {
            // Closes at once, on the press (an emergency must not wait for the button release).
            with_app(|a| a.close_hwnd(hwnd));
            LRESULT(0)
        }
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK if wp.0 as u32 == HT_SOUND => {
            with_app(|a| a.toggle_sound(hwnd));
            LRESULT(0)
        }
        // The release on a button is ours too (the default handling has nothing to do there).
        WM_NCLBUTTONUP if matches!(wp.0 as u32, HTCLOSE | HT_SOUND) => LRESULT(0),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCLBUTTONDOWN => {
            // Dragging moves the window; a plain click pins / unpins it.
            let before = window_rect(hwnd);
            let r = DefWindowProcW(hwnd, msg, wp, lp);
            let after = window_rect(hwnd);
            with_app(|a| a.after_click(hwnd, before != after));
            r
        }
        WM_NCLBUTTONDBLCLK => {
            if let Some(url) = with_app(|a| a.camera_url(hwnd)).flatten() {
                open_url(&url);
            }
            LRESULT(0)
        }
        WM_NCRBUTTONDOWN | WM_NCMBUTTONDOWN | WM_CONTEXTMENU => LRESULT(0),
        WM_NCRBUTTONUP | WM_NCMBUTTONUP => {
            with_app(|a| a.close_hwnd(hwnd));
            LRESULT(0)
        }
        WM_NCMOUSEMOVE => {
            with_app(|a| a.hover(hwnd, true, wp.0 as u32));
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_NCMOUSELEAVE => {
            with_app(|a| a.hover(hwnd, false, HTNOWHERE));
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as u16 as i16;
            // With Ctrl the picture zooms around the mouse; without, the window grows or shrinks.
            // MK_CONTROL = 0x0008 in the low word (key state).
            if wp.0 & 0x0008 != 0 {
                let at = POINT { x: (lp.0 & 0xFFFF) as u16 as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32 };
                with_app(|a| a.zoom(hwnd, delta, at));
            } else {
                with_app(|a| a.wheel(hwnd, delta));
            }
            LRESULT(0)
        }
        WM_TIMER => {
            with_app(|a| a.popup_timer(hwnd, wp.0));
            LRESULT(0)
        }
        WM_PAINT => {
            let _ = ValidateRect(hwnd, None);
            with_app(|a| a.request_redraw(hwnd));
            LRESULT(0)
        }
        WM_APP_ASPECT => {
            with_app(|a| a.set_aspect(hwnd, wp.0 as u32, lp.0 as u32));
            LRESULT(0)
        }
        WM_APP_PLAYER_DONE => {
            with_app(|a| a.player_done(hwnd));
            LRESULT(0)
        }
        WM_SYSCOMMAND if (wp.0 & 0xFFF0) as u32 == SC_MAXIMIZE || (wp.0 & 0xFFF0) as u32 == SC_KEYMENU => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CTLCOLORSTATIC => {
            if let Some(brush) = settings::pane_brush(HWND(lp.0 as _), HDC(wp.0 as _)) {
                return LRESULT(brush.0 as isize);
            }
            let _ = SetBkColor(HDC(wp.0 as _), COLORREF(GetSysColor(COLOR_WINDOW)));
            LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
        }
        WM_COMMAND => {
            let (id, code) = ((wp.0 & 0xFFFF) as i32, ((wp.0 >> 16) & 0xFFFF) as u32);
            with_app(|a| a.on_settings_command(id, code));
            LRESULT(0)
        }
        WM_NOTIFY => {
            let hdr = &*(lp.0 as *const windows::Win32::UI::Controls::NMHDR);
            if hdr.code == windows::Win32::UI::Controls::TCN_SELCHANGE {
                with_app(|a| a.settings.as_ref().map(|s| s.show_page()));
            } else if hdr.code == windows::Win32::UI::Controls::LVN_ITEMCHANGED {
                let lv = &*(lp.0 as *const windows::Win32::UI::Controls::NMLISTVIEW);
                if (lv.uNewState ^ lv.uOldState) & windows::Win32::UI::Controls::LVIS_STATEIMAGEMASK.0 != 0 {
                    with_app(|a| a.settings_changed());
                }
            }
            LRESULT(0)
        }
        WM_HSCROLL => {
            with_app(|a| {
                if let Some(s) = &a.settings {
                    s.update_labels();
                }
                a.settings_changed();
            });
            LRESULT(0)
        }
        settings::WM_APP_TEST_LINE => {
            let line = Box::from_raw(lp.0 as *mut String);
            with_app(|a| a.settings.as_ref().map(|s| s.append_test(&line)));
            LRESULT(0)
        }
        WM_CLOSE => {
            with_app(|a| a.close_settings());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------------------------------------------------------- logic

impl App {
    fn paused(&self) -> bool {
        match self.pause {
            Pause::Off => false,
            Pause::Forever => true,
            Pause::Until(t) => Instant::now() < t,
        }
    }

    fn tray(&self, action: NOTIFY_ICON_MESSAGE) {
        let icon = if self.paused() {
            self.icons[2]
        } else if self.connected {
            self.icons[0]
        } else {
            self.icons[1]
        };
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_APP_TRAY,
            hIcon: icon,
            ..Default::default()
        };
        let tip = format!("FrigatePiP\n{}", self.status_line());
        for (i, c) in tip.encode_utf16().take(nid.szTip.len() - 1).enumerate() {
            nid.szTip[i] = c;
        }
        unsafe {
            let _ = Shell_NotifyIconW(action, &nid);
        }
    }

    fn status_line(&self) -> String {
        match self.pause {
            Pause::Forever => return t().status_paused_forever.into(),
            Pause::Until(until) if Instant::now() < until => {
                let mins = (until - Instant::now()).as_secs() / 60 + 1;
                return i18n::fmt(t().status_paused_minutes, &[&mins]);
            }
            _ => {}
        }
        if self.connected {
            i18n::fmt(t().status_connected, &[&self.status])
        } else {
            i18n::fmt(t().status_disconnected, &[&self.status])
        }
    }

    fn start_listener(&mut self) {
        self.listener.cancel();
        self.listener = Cancel::new();
        self.listener_key = listener_key(&self.cfg);
        self.connected = false;
        self.status = t().status_connecting.into();
        let hwnd = self.hwnd.0 as isize;
        frigate::start(self.cfg.clone(), self.listener.clone(), move |n| unsafe {
            let b = Box::into_raw(Box::new(n));
            if PostMessageW(HWND(hwnd as _), WM_APP_NOTICE, WPARAM(0), LPARAM(b as isize)).is_err() {
                drop(Box::from_raw(b));
            }
        });
        self.tray(NIM_MODIFY);
    }

    fn on_notice(&mut self, n: Notice) {
        match n {
            Notice::Connected(url) => {
                self.connected = true;
                self.status = url;
                self.tray(NIM_MODIFY);
            }
            Notice::Disconnected(e) => {
                self.connected = false;
                self.status = e;
                self.tray(NIM_MODIFY);
            }
            Notice::Cameras(c) => {
                if c != self.cameras {
                    crate::log!(
                        "Cameras: {}",
                        c.iter()
                            .map(|c| format!("{} [{} / {}]", c.name, c.main.as_deref().unwrap_or("-"), c.sub.as_deref().unwrap_or("-")))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                self.cameras = c;
                self.update_hubs();
                if let Some(s) = &mut self.settings {
                    s.set_cameras(&self.cameras, &self.cfg);
                }
                if let Some(t) = self.pending_test.take() {
                    self.test_now(&t);
                }
            }
            Notice::Show(d, t0) => self.on_detection(d, t0, false),
            Notice::Activity(cam) => self.extend(&cam),
        }
    }

    fn monitor(&self) -> Mon {
        let mut mons = monitors();
        let pick = match self.vcfg().monitor.parse::<usize>() {
            Ok(n) if n >= 1 && n <= mons.len() => Some(n - 1),
            _ => mons.iter().position(|m| m.primary),
        };
        match pick {
            Some(i) => mons.swap_remove(i),
            None => mons.pop().unwrap_or(Mon {
                work: RECT { left: 0, top: 0, right: 1920, bottom: 1080 },
                rect: RECT { left: 0, top: 0, right: 1920, bottom: 1080 },
                primary: true,
                dpi: 96,
            }),
        }
    }

    fn update_hubs(&mut self) {
        if !self.cfg.prebuffer || self.cfg.live != LiveMode::Rtsp {
            if !self.hubs.is_empty() {
                crate::log!("Prebuffer disabled");
            }
            self.hubs.clear();
            return;
        }
        // The prebuffer always holds the lightweight sub stream; a window that wants main
        // shows it at once and switches to main after main's first frame.
        let mut want: HashMap<String, String> = HashMap::new();
        for c in &self.cameras {
            if !self.cfg.cameras.is_empty() && !self.cfg.cameras.iter().any(|x| x.eq_ignore_ascii_case(&c.name)) {
                continue;
            }
            if let Some(s) = c.sub.clone().or_else(|| c.main.clone()) {
                want.insert(c.name.clone(), self.cfg.rtsp_url(&s));
            }
        }
        let audio = self.cfg.camera_audio;
        self.hubs.retain(|k, h| want.get(k).is_some_and(|u| u == h.url()) && h.audio() == audio);
        for (cam, url) in want {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.hubs.entry(cam) {
                crate::log!("Prebuffer {}: {url}", slot.key());
                let hub = Arc::new(Hub::start(url, slot.key(), audio));
                slot.insert(hub);
            }
        }
    }

    fn update_gpu_warm(&mut self) {
        use windows::Win32::Graphics::Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT};
        if !self.cfg.gpu_warm {
            self.gpu_warm = None;
        } else if self.gpu_warm.is_none() {
            self.gpu_warm = crate::render::create_device(D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT).ok();
        }
    }

    fn test(&mut self, name: &str) {
        if self.cameras.is_empty() {
            self.pending_test = Some(name.to_string());
            unsafe {
                SetTimer(self.hwnd, TIMER_TEST, 3000, None);
            }
            return;
        }
        self.test_now(name);
    }

    fn test_now(&mut self, name: &str) {
        let cam = self
            .cameras
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name) || c.title.eq_ignore_ascii_case(name))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| name.to_string());
        let d = Detection { camera: cam, label: "test".into(), sub_label: None, score: 0.0, id: String::new() };
        self.on_detection(d, Instant::now(), true);
    }

    fn on_detection(&mut self, d: Detection, t0: Instant, test: bool) {
        if !test {
            if self.paused() {
                crate::log!("{}: skipped (paused)", d.camera);
                return;
            }
            if self.cfg.skip_fullscreen && fullscreen_active() {
                crate::log!("{}: skipped (full screen)", d.camera);
                return;
            }
        }
        let info = self.cameras.iter().find(|c| c.name == d.camera).cloned();
        let title = info.as_ref().map(|c| c.title.clone()).unwrap_or_else(|| d.camera.clone());
        let text = crate::view::caption(&title, &d);
        let now = Instant::now();
        let dur = Duration::from_secs(self.vcfg().duration as u64);
        let max = Duration::from_secs(self.vcfg().max_duration.max(self.vcfg().duration) as u64);
        if let Some(p) = self.popups.iter_mut().find(|p| p.camera == d.camera && !p.closing && !p.preview) {
            p.base_text = text;
            p.shared.set_overlay(Self::popup_text(p));
            p.hard_deadline = now + max;
            p.deadline = p.deadline.max(now + dur).min(p.hard_deadline);
            Self::arm(p);
            return;
        }
        let (max_popups, replace) = (self.vcfg().max_popups, self.vcfg().replace);
        let slots: Vec<usize> = (0..self.popups.len()).filter(|&i| !self.popups[i].preview).collect();
        let states: Vec<(bool, bool)> = slots.iter().map(|&i| (self.popups[i].closing, self.popups[i].pinned)).collect();
        let place = match choose_slot(&states, max_popups, replace) {
            Slot::New => None,
            Slot::Skip => {
                crate::log!("{}: skipped (all windows in use)", d.camera);
                return;
            }
            Slot::Replace(k) => {
                let i = slots[k];
                let old = &self.popups[i];
                crate::log!("{}: replaces window {}", d.camera, old.camera);
                let place = Placement { index: i, moved_to: old.user_moved.then(|| window_rect(old.hwnd)), width: old.width };
                self.close_quiet(i);
                Some(place)
            }
        };
        self.create_popup(d.camera, info, title, text, t0, place);
        if self.cfg.sound {
            unsafe {
                let _ = MessageBeep(MB_ICONASTERISK);
            }
        }
    }

    fn popup_text(p: &Popup) -> String {
        let mut text = p.base_text.clone();
        if p.pinned {
            text = format!("{text} · {}", t().overlay_pinned);
        }
        if let Some(zoom) = p.shared.zoom().label() {
            text = format!("{text} · {zoom}");
        }
        text
    }

    fn create_popup(
        &mut self,
        camera: String,
        info: Option<CameraInfo>,
        title: String,
        text: String,
        t0: Instant,
        place: Option<Placement>,
    ) -> Option<HWND> {
        let v = self.vcfg().clone();
        let scale = self.monitor().dpi as f32 / 96.0;
        let width = place.as_ref().and_then(|p| p.width).unwrap_or((v.width as f32 * scale).round() as i32);
        let aspect = info.as_ref().map(|c| c.aspect).filter(|a| *a > 0.2).unwrap_or(16.0 / 9.0);
        let stream = choose_stream(&v, info.as_ref(), &camera, width);
        let rtsp_url = stream.as_ref().map(|s| self.cfg.rtsp_url(s));
        let hub = self.hubs.get(&camera).cloned();
        let ex = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | if v.topmost { WS_EX_TOPMOST } else { WINDOW_EX_STYLE(0) };
        let hwnd = unsafe {
            match CreateWindowExW(
                ex,
                POPUP_CLASS,
                &wide(&title),
                WS_POPUP,
                0,
                0,
                width,
                (width as f32 / aspect) as i32,
                HWND::default(),
                HMENU::default(),
                self.hinst,
                None,
            ) {
                Ok(h) => h,
                Err(e) => {
                    crate::log!("{camera}: cannot create window: {e}");
                    return None;
                }
            }
        };
        unsafe {
            let pref = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &pref as *const _ as *const _, 4);
        }
        let shared = Shared::new(text.clone(), v.overlay);
        let now = Instant::now();
        let dur = Duration::from_secs(v.duration as u64);
        let max = Duration::from_secs(v.max_duration.max(v.duration) as u64);
        let popup = Popup {
            hwnd,
            camera: camera.clone(),
            base_text: text,
            deadline: now + dur,
            hard_deadline: now + max,
            hovered: false,
            pinned: false,
            closing: false,
            user_moved: place.as_ref().is_some_and(|p| p.moved_to.is_some()),
            width: place.as_ref().and_then(|p| p.width),
            aspect,
            shared: shared.clone(),
            preview: false,
        };
        match &place {
            Some(p) => {
                if let Some(r) = p.moved_to {
                    unsafe {
                        let _ = SetWindowPos(hwnd, HWND::default(), r.left, r.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
                    }
                }
                self.popups.insert(p.index.min(self.popups.len()), popup);
            }
            None => self.popups.push(popup),
        }
        self.layout();
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetTimer(hwnd, TIMER_DEADLINE, dur.as_millis() as u32, None);
        }
        let prebuf_note = match &hub {
            Some(h) if Some(h.url()) == rtsp_url.as_deref() => ", from prebuffer",
            Some(_) => ", starting from sub prebuffer",
            None => "",
        };
        crate::log!("{camera}: window after {} ms (stream {}{prebuf_note})", t0.elapsed().as_millis(), stream.as_deref().unwrap_or("-"));
        let params = player::Params {
            hwnd: hwnd.0 as isize,
            camera: camera.clone(),
            frigate_url: self.cfg.url.clone(),
            rtsp_url,
            hub,
            live: v.live,
            audio: v.camera_audio,
            t0,
        };
        let _ = std::thread::Builder::new().name(format!("pip-{camera}")).spawn(move || player::run(params, shared));
        Some(hwnd)
    }

    /// Settings used for display: the draft from an open settings window, or the saved ones.
    fn vcfg(&self) -> &Config {
        self.view.as_ref().unwrap_or(&self.cfg)
    }

    // ------------------------------------------------ settings window

    fn open_settings(&mut self) {
        if let Some(s) = &self.settings {
            unsafe {
                let _ = ShowWindow(s.hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(s.hwnd);
            }
            return;
        }
        let monitors: Vec<String> = monitors()
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let main = if m.primary { t().monitor_primary_suffix } else { "" };
                let (w, h) = (m.rect.right - m.rect.left, m.rect.bottom - m.rect.top);
                i18n::fmt(t().monitor_item, &[&(i + 1), &w, &h]) + main
            })
            .collect();
        let init = settings::Init { cfg: &self.cfg, autostart: is_autostart(), cameras: &self.cameras, monitors, icon: self.icons[0] };
        self.settings = Settings::create(self.hinst, init);
        if let Some(s) = &self.settings {
            SETTINGS_HWND.store(s.hwnd.0 as isize, Ordering::Relaxed);
            s.set_update(&self.update_status, self.update.as_ref().map(|r| r.version.as_str()));
        }
    }

    fn on_settings_command(&mut self, id: i32, code: u32) {
        let Some(s) = &self.settings else { return };
        match id {
            x if x == IDOK.0 => {
                self.apply_settings();
                self.close_settings();
            }
            x if x == IDCANCEL.0 => self.close_settings(),
            settings::ID_APPLY => self.apply_settings(),
            settings::ID_HELP => open_url(&settings::help_url()),
            settings::ID_UPDATE => {
                if self.update.is_some() {
                    self.run_update();
                } else {
                    self.check_for_update(true);
                }
            }
            settings::ID_TEST => self.run_connection_test(),
            settings::ID_TEST_OUT => {}
            settings::ID_PREVIEW => {
                if s.checked(settings::ID_PREVIEW) {
                    self.preview_on();
                } else {
                    self.preview_off();
                }
            }
            settings::ID_PREV_CAM => {
                if code == CBN_SELCHANGE && s.checked(settings::ID_PREVIEW) {
                    self.preview_on();
                }
            }
            _ if code == EN_CHANGE || code == CBN_SELCHANGE || code == BN_CLICKED => self.settings_changed(),
            _ => {}
        }
    }

    /// A control changed: the draft applies to open windows at once (without saving).
    fn settings_changed(&mut self) {
        let Some(s) = &self.settings else { return };
        s.update_enabled();
        let (draft, _) = s.read(&self.cfg);
        if &draft == self.vcfg() {
            return;
        }
        let old = self.vcfg().clone();
        self.view = Some(draft);
        self.refresh_view(&old);
    }

    /// Updates open windows to match the current display settings.
    fn refresh_view(&mut self, old: &Config) {
        let v = self.vcfg().clone();
        if v.topmost != old.topmost {
            let after = if v.topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
            for p in &self.popups {
                unsafe {
                    let _ = SetWindowPos(p.hwnd, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
        for p in &mut self.popups {
            p.shared.set_overlay_on(v.overlay);
            if v.width != old.width {
                p.width = None;
            }
            if p.preview && (v.position != old.position || v.monitor != old.monitor || v.margin != old.margin) {
                p.user_moved = false;
            }
        }
        self.layout();
        let video = |c: &Config| (c.live, c.stream, c.auto_main_width, c.camera_audio);
        if video(&v) != video(old) && self.popups.iter().any(|p| p.preview && !p.closing) {
            self.preview_on();
        }
    }

    fn apply_settings(&mut self) {
        let Some(s) = &self.settings else { return };
        let (draft, autostart) = s.read(&self.cfg);
        if autostart != is_autostart() {
            set_autostart(autostart);
        }
        config::write_all(&draft, &mut self.doc);
        config::save(&self.path, &self.doc);
        crate::log!("Settings saved from the settings window");
        let old = self.vcfg().clone();
        self.view = None;
        self.apply(draft);
        self.refresh_view(&old);
    }

    /// Closes the settings window; the unsaved draft no longer applies.
    fn close_settings(&mut self) {
        let Some(s) = self.settings.take() else { return };
        SETTINGS_HWND.store(0, Ordering::Relaxed);
        self.preview_off();
        drop(s);
        if self.view.is_some() {
            let old = self.vcfg().clone();
            self.view = None;
            self.refresh_view(&old);
        }
    }

    /// Preview of the camera selected in the settings window (pinned, outside the window limit).
    fn preview_on(&mut self) {
        self.preview_off();
        let Some(idx) = self.settings.as_ref().and_then(|s| s.preview_camera()) else { return };
        let Some(info) = self.cameras.get(idx).cloned() else { return };
        let text = format!("{} · {}", info.title, t().overlay_settings_preview);
        if let Some(h) = self.create_popup(info.name.clone(), Some(info.clone()), info.title.clone(), text, Instant::now(), None) {
            if let Some(p) = self.find(h) {
                p.preview = true;
                p.pinned = true;
            }
        }
    }

    fn preview_off(&mut self) {
        let open: Vec<usize> = (0..self.popups.len()).filter(|&i| self.popups[i].preview && !self.popups[i].closing).collect();
        if open.is_empty() {
            return;
        }
        for i in open {
            self.close_quiet(i);
        }
        self.layout();
    }

    fn run_connection_test(&mut self) {
        let Some(s) = &self.settings else { return };
        let (draft, _) = s.read(&self.cfg);
        s.clear_test();
        let hwnd = s.hwnd.0 as isize;
        let _ = std::thread::Builder::new().name("conn-test".into()).spawn(move || {
            let mut post = |line: String| unsafe {
                let b = Box::into_raw(Box::new(line));
                if PostMessageW(HWND(hwnd as _), settings::WM_APP_TEST_LINE, WPARAM(0), LPARAM(b as isize)).is_err() {
                    drop(Box::from_raw(b));
                }
            };
            frigate::connection_test(&draft, &Cancel::new(), &mut post);
        });
    }

    fn arm(p: &Popup) {
        let left = p.deadline.saturating_duration_since(Instant::now()).as_millis().max(50) as u32;
        unsafe {
            SetTimer(p.hwnd, TIMER_DEADLINE, left, None);
        }
    }

    fn extend(&mut self, cam: &str) {
        let dur = Duration::from_secs(self.vcfg().duration as u64);
        if let Some(p) = self.popups.iter_mut().find(|p| p.camera == cam && !p.closing) {
            let target = (Instant::now() + dur).min(p.hard_deadline);
            if target > p.deadline {
                p.deadline = target;
                Self::arm(p);
            }
        }
    }

    fn find(&mut self, hwnd: HWND) -> Option<&mut Popup> {
        self.popups.iter_mut().find(|p| p.hwnd == hwnd)
    }

    fn layout(&mut self) {
        let mon = self.monitor();
        let (margin_px, width_px, position) = (self.vcfg().margin, self.vcfg().width, self.vcfg().position);
        let scale = mon.dpi as f32 / 96.0;
        let margin = (margin_px as f32 * scale).round() as i32;
        let gap = (8.0 * scale).round() as i32;
        let work = mon.work;
        let max_w = (work.right - work.left - 2 * margin).max(160);
        let default_w = (width_px as f32 * scale).round() as i32;
        let mut offset = 0;
        for p in self.popups.iter().filter(|p| !p.closing) {
            let w = p.width.unwrap_or(default_w).min(max_w);
            let h = (w as f32 / p.aspect).round() as i32;
            unsafe {
                if p.user_moved {
                    let _ = SetWindowPos(p.hwnd, HWND::default(), 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
                    continue;
                }
                let x = match position {
                    Corner::TopLeft | Corner::BottomLeft => work.left + margin,
                    _ => work.right - margin - w,
                };
                let y = match position {
                    Corner::TopLeft | Corner::TopRight => work.top + margin + offset,
                    _ => work.bottom - margin - h - offset,
                };
                offset += h + gap;
                let _ = SetWindowPos(p.hwnd, HWND::default(), x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
    }

    fn close(&mut self, i: usize) {
        self.close_quiet(i);
        self.layout();
    }

    /// Closes a window without recomputing the layout (e.g. when replacing it in place).
    fn close_quiet(&mut self, i: usize) {
        let p = &mut self.popups[i];
        if p.closing {
            return;
        }
        p.closing = true;
        p.shared.cancel.cancel();
        unsafe {
            let _ = KillTimer(p.hwnd, TIMER_DEADLINE);
            let _ = ShowWindow(p.hwnd, SW_HIDE);
            SetTimer(p.hwnd, TIMER_FORCE_CLOSE, 3000, None);
        }
    }

    fn close_hwnd(&mut self, hwnd: HWND) {
        if let Some(i) = self.popups.iter().position(|p| p.hwnd == hwnd) {
            if self.popups[i].preview {
                if let Some(s) = &self.settings {
                    s.set_checked(settings::ID_PREVIEW, false);
                }
            }
            self.close(i);
        }
    }

    fn close_all(&mut self) {
        let all: Vec<HWND> = self.popups.iter().map(|p| p.hwnd).collect();
        for h in all {
            self.close_hwnd(h);
        }
    }

    fn player_done(&mut self, hwnd: HWND) {
        if let Some(i) = self.popups.iter().position(|p| p.hwnd == hwnd) {
            if !self.popups[i].closing {
                // The player exited on its own (e.g. a renderer error) — close the window.
                self.popups[i].closing = true;
            }
            self.popups.remove(i);
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            self.layout();
            if self.popups.is_empty() {
                crate::util::trim_memory();
            }
        }
    }

    fn popup_timer(&mut self, hwnd: HWND, id: usize) {
        match id {
            TIMER_FORCE_CLOSE => self.player_done(hwnd),
            TIMER_DEADLINE => {
                let Some(i) = self.popups.iter().position(|p| p.hwnd == hwnd) else {
                    unsafe {
                        let _ = KillTimer(hwnd, id);
                    }
                    return;
                };
                let p = &self.popups[i];
                if p.hovered || p.pinned || p.preview {
                    unsafe {
                        SetTimer(hwnd, TIMER_DEADLINE, 1000, None);
                    }
                } else if Instant::now() >= p.deadline {
                    self.close(i);
                } else {
                    Self::arm(p);
                }
            }
            _ => {}
        }
    }

    /// Mouse over the window (`on`); `hit` = the part under it (`HTCLOSE`, `HT_SOUND`).
    fn hover(&mut self, hwnd: HWND, on: bool, hit: u32) {
        let Some(p) = self.find(hwnd) else { return };
        p.shared.set_close_button(match (on, hit) {
            (false, _) => 0,
            (true, HTCLOSE) => 2,
            (true, HT_SOUND) => 3,
            (true, _) => 1,
        });
        if on && !p.hovered {
            p.hovered = true;
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE | TME_NONCLIENT,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut tme);
            }
        } else if !on {
            p.hovered = false;
            p.deadline = p.deadline.max(Instant::now() + Duration::from_secs(3));
            Self::arm(p);
        }
    }

    /// The window plays camera audio (then it has a sound button).
    fn has_sound(&mut self, hwnd: HWND) -> bool {
        self.find(hwnd).is_some_and(|p| p.shared.sound.playing.load(Ordering::Relaxed))
    }

    /// Sound button: mutes / unmutes this window.
    fn toggle_sound(&mut self, hwnd: HWND) {
        let Some(p) = self.find(hwnd) else { return };
        let muted = !p.shared.sound.muted.load(Ordering::Relaxed);
        p.shared.sound.muted.store(muted, Ordering::Relaxed);
        p.shared.request_redraw();
        crate::log!("{}: sound {}", p.camera, if muted { "muted" } else { "on" });
    }

    fn after_click(&mut self, hwnd: HWND, moved: bool) {
        let Some(p) = self.find(hwnd) else { return };
        if moved {
            p.user_moved = true;
        } else if !p.preview {
            p.pinned = !p.pinned;
            p.shared.set_overlay(Self::popup_text(p));
            if !p.pinned {
                p.deadline = p.deadline.max(Instant::now() + Duration::from_secs(5));
                Self::arm(p);
            }
        }
        self.layout();
    }

    fn wheel(&mut self, hwnd: HWND, delta: i16) {
        let mon = self.monitor();
        let scale = mon.dpi as f32 / 96.0;
        let default_w = (self.vcfg().width as f32 * scale).round() as i32;
        let max_w = mon.work.right - mon.work.left;
        let Some(p) = self.find(hwnd) else { return };
        let cur = p.width.unwrap_or(default_w) as f32;
        let factor = if delta > 0 { 1.12 } else { 1.0 / 1.12 };
        p.width = Some(((cur * factor) as i32).clamp((160.0 * scale) as i32, max_w));
        self.layout();
    }

    /// Ctrl + wheel: zooms the picture in or out around the mouse (`at`, screen pixels).
    fn zoom(&mut self, hwnd: HWND, delta: i16, at: POINT) {
        let Some(p) = self.find(hwnd) else { return };
        let mut pt = at;
        let mut rc = RECT::default();
        unsafe {
            let _ = ScreenToClient(hwnd, &mut pt);
            let _ = GetClientRect(hwnd, &mut rc);
        }
        // Where the picture is in the window (it may have bars when the aspect differs).
        let (l, t, r, b) = crate::render::fit(((p.aspect * 1000.0).round() as u32, 1000), (rc.right.max(1) as u32, rc.bottom.max(1) as u32));
        let (u, v) = ((pt.x as f32 - l) / (r - l).max(1.0), (pt.y as f32 - t) / (b - t).max(1.0));
        let zoom = p.shared.zoom().at(delta as f32 / WHEEL_DELTA as f32, u, v);
        p.shared.set_zoom(zoom);
        p.shared.set_overlay(Self::popup_text(p));
    }

    fn set_aspect(&mut self, hwnd: HWND, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        let Some(p) = self.find(hwnd) else { return };
        let a = w as f32 / h as f32;
        if (a - p.aspect).abs() / p.aspect > 0.02 {
            p.aspect = a;
            self.layout();
        }
    }

    fn request_redraw(&mut self, hwnd: HWND) {
        if let Some(p) = self.find(hwnd) {
            p.shared.request_redraw();
        }
    }

    fn camera_url(&mut self, hwnd: HWND) -> Option<String> {
        let base = self.cfg.url.clone();
        let p = self.find(hwnd)?;
        Some(format!("{base}/#{}", p.camera))
    }

    fn set_pause(&mut self, pause: Pause) {
        self.pause = pause;
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_RESUME);
            if let Pause::Until(t) = pause {
                let ms = t.saturating_duration_since(Instant::now()).as_millis().min(u32::MAX as u128) as u32;
                SetTimer(self.hwnd, TIMER_RESUME, ms.max(1000), None);
            }
        }
        crate::log!("{}", self.status_line());
        self.tray(NIM_MODIFY);
    }

    fn reload(&mut self) {
        let (cfg, doc) = match config::load(&self.path) {
            Ok(v) => v,
            Err(e) => {
                crate::log!("Cannot read settings ({e}) - keeping previous ones");
                return;
            }
        };
        self.doc = doc;
        crate::log!("Settings reloaded");
        self.apply(cfg);
    }

    fn apply(&mut self, cfg: Config) {
        let topmost_changed = cfg.topmost != self.cfg.topmost;
        let language_changed = cfg.language != self.cfg.language;
        self.cfg = cfg;
        if language_changed {
            i18n::set(i18n::resolve(&self.cfg.language));
            crate::log!("Language: {}", i18n::current().code());
            // Rebuild an open settings window in the new language, on the same tab.
            if let Some(page) = self.settings.as_ref().map(|s| s.page()) {
                self.close_settings();
                self.open_settings();
                if let Some(s) = &self.settings {
                    s.set_page(page);
                }
            }
        }
        if listener_key(&self.cfg) != self.listener_key {
            self.start_listener();
        }
        self.update_hubs();
        self.update_gpu_warm();
        if topmost_changed {
            let after = if self.cfg.topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
            for p in &self.popups {
                unsafe {
                    let _ = SetWindowPos(p.hwnd, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
        self.layout();
        self.tray(NIM_MODIFY);
    }

    // ------------------------------------------------ updates

    fn on_update_timer(&mut self) {
        unsafe {
            SetTimer(self.hwnd, TIMER_UPDATE, 24 * 60 * 60 * 1000, None);
        }
        if self.cfg.check_updates {
            self.check_for_update(false);
        }
    }

    /// Asks GitHub for the latest release in the background.
    fn check_for_update(&mut self, manual: bool) {
        if matches!(self.update_status, update::Status::Checking | update::Status::Downloading) {
            return;
        }
        self.set_update_status(update::Status::Checking);
        let hwnd = self.hwnd.0 as isize;
        let _ = std::thread::Builder::new().name("update-check".into()).spawn(move || {
            post_update_event(hwnd, UpdateEvent::Checked { manual, result: update::check() });
        });
    }

    fn on_update_event(&mut self, ev: UpdateEvent) {
        match ev {
            UpdateEvent::Checked { result: Ok(r), .. } => {
                let current = update::current_version();
                if update::is_newer(&r.version, &current) {
                    crate::log!("Update available: {} (running {current})", r.version);
                    let v = r.version.clone();
                    self.update = Some(r);
                    self.set_update_status(update::Status::Available(v.clone()));
                    if self.update_notified.as_deref() != Some(v.as_str()) {
                        self.update_notified = Some(v.clone());
                        self.notify(&i18n::fmt(t().update_balloon_title, &[&v]), t().update_balloon_text, true);
                    }
                } else {
                    crate::log!("No update: {current} is current (latest {})", r.version);
                    self.update = None;
                    self.set_update_status(update::Status::Latest);
                }
            }
            UpdateEvent::Checked { result: Err(e), manual } => {
                crate::log!("Update check failed: {e}");
                let st = match (&self.update, manual) {
                    (_, true) => update::Status::CheckFailed(e),
                    (Some(r), false) => update::Status::Available(r.version.clone()),
                    (None, false) => update::Status::Idle,
                };
                self.set_update_status(st);
            }
            UpdateEvent::InstallFailed(e) => {
                crate::log!("Update failed: {e}");
                self.set_update_status(update::Status::InstallFailed(e.clone()));
                self.notify("FrigatePiP", &i18n::fmt(t().update_failed, &[&e]), false);
            }
        }
    }

    /// Installs the update found: the installed copy downloads and runs the installer,
    /// a portable copy opens the release page.
    fn run_update(&mut self) {
        let Some(r) = self.update.clone() else { return };
        if !update::installed() {
            open_url(&r.page);
            return;
        }
        if self.update_status == update::Status::Downloading {
            return;
        }
        crate::log!("Downloading update {}", r.version);
        self.set_update_status(update::Status::Downloading);
        let hwnd = self.hwnd.0 as isize;
        let _ = std::thread::Builder::new().name("update".into()).spawn(move || {
            if let Err(e) = update::install(&r) {
                post_update_event(hwnd, UpdateEvent::InstallFailed(e));
            }
        });
    }

    fn balloon_clicked(&mut self) {
        if self.update_balloon {
            self.run_update();
        }
    }

    fn set_update_status(&mut self, st: update::Status) {
        self.update_status = st;
        if let Some(s) = &self.settings {
            s.set_update(&self.update_status, self.update.as_ref().map(|r| r.version.as_str()));
        }
    }

    /// Windows notification from the tray icon; `offer` = a click installs the update.
    fn notify(&mut self, title: &str, text: &str, offer: bool) {
        self.update_balloon = offer;
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_INFO,
            dwInfoFlags: NIIF_INFO,
            ..Default::default()
        };
        for (i, c) in title.encode_utf16().take(nid.szInfoTitle.len() - 1).enumerate() {
            nid.szInfoTitle[i] = c;
        }
        for (i, c) in text.encode_utf16().take(nid.szInfo.len() - 1).enumerate() {
            nid.szInfo[i] = c;
        }
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    fn build_menu(&self) -> HMENU {
        unsafe {
            let m = CreatePopupMenu().unwrap_or_default();
            let item = |menu: HMENU, id: u32, text: &str, checked: bool| {
                let flags = MF_STRING | if checked { MF_CHECKED } else { MF_UNCHECKED };
                let _ = AppendMenuW(menu, flags, id as usize, &wide(&text.replace('&', "&&")));
            };
            let sep = |menu: HMENU| {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            };
            let sub = |menu: HMENU, child: HMENU, text: &str| {
                let _ = AppendMenuW(menu, MF_POPUP, child.0 as usize, &wide(&text.replace('&', "&&")));
            };
            let _ = AppendMenuW(m, MF_STRING | MF_GRAYED, 0, &wide(&self.status_line()));
            sep(m);
            let t = t();
            if let Some(r) = &self.update {
                item(m, CMD_UPDATE, &i18n::fmt(t.menu_update, &[&r.version]), false);
                sep(m);
            }
            item(m, CMD_SETTINGS, t.menu_settings, false);
            let _ = SetMenuDefaultItem(m, CMD_SETTINGS, 0);

            let cams = CreatePopupMenu().unwrap_or_default();
            if self.cameras.is_empty() {
                let _ = AppendMenuW(cams, MF_STRING | MF_GRAYED, 0, &wide(t.menu_no_cameras));
            }
            for (i, c) in self.cameras.iter().enumerate().take(100) {
                item(cams, CMD_TEST + i as u32, &c.title, false);
            }
            sub(m, cams, t.menu_show_camera);

            let pause = CreatePopupMenu().unwrap_or_default();
            item(pause, CMD_PAUSE_15, t.menu_pause_15, false);
            item(pause, CMD_PAUSE_60, t.menu_pause_60, false);
            item(pause, CMD_PAUSE_FOREVER, t.menu_pause_forever, self.pause == Pause::Forever);
            if self.paused() {
                sep(pause);
                item(pause, CMD_RESUME, t.menu_resume, false);
            }
            sub(m, pause, if self.paused() { t.menu_paused } else { t.menu_pause });
            item(m, CMD_CLOSE_ALL, t.menu_close_all, false);
            sep(m);
            item(m, CMD_OPEN_FRIGATE, t.menu_open_frigate, false);
            item(m, CMD_HELP, t.menu_help, false);
            item(m, CMD_LOG, t.menu_log, false);
            item(m, CMD_EDIT, t.menu_config_file, false);
            sep(m);
            item(m, CMD_EXIT, t.menu_exit, false);
            m
        }
    }

    fn on_command(&mut self, cmd: u32) {
        match cmd {
            c if (CMD_TEST..CMD_TEST + 100).contains(&c) => {
                if let Some(cam) = self.cameras.get((c - CMD_TEST) as usize) {
                    let name = cam.name.clone();
                    self.test_now(&name);
                }
            }
            CMD_SETTINGS => self.open_settings(),
            CMD_PAUSE_15 => self.set_pause(Pause::Until(Instant::now() + Duration::from_secs(15 * 60))),
            CMD_PAUSE_60 => self.set_pause(Pause::Until(Instant::now() + Duration::from_secs(60 * 60))),
            CMD_PAUSE_FOREVER => self.set_pause(Pause::Forever),
            CMD_RESUME => self.set_pause(Pause::Off),
            CMD_CLOSE_ALL => self.close_all(),
            CMD_OPEN_FRIGATE => open_url(&self.cfg.url),
            CMD_EDIT => open_in_notepad(&self.path),
            CMD_LOG => open_in_notepad(&crate::util::log_path()),
            CMD_HELP => open_url(&settings::help_url()),
            CMD_UPDATE => self.run_update(),
            CMD_EXIT => unsafe { PostQuitMessage(0) },
            _ => {}
        }
    }

    fn shutdown(&mut self) {
        crate::log!("Shutting down");
        self.settings = None;
        SETTINGS_HWND.store(0, Ordering::Relaxed);
        self.listener.cancel();
        self.hubs.clear();
        for p in self.popups.drain(..) {
            p.shared.cancel.cancel();
            unsafe {
                let _ = DestroyWindow(p.hwnd);
            }
        }
        self.tray(NIM_DELETE);
        for i in self.icons {
            unsafe {
                let _ = DestroyIcon(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_texts(m: HMENU, depth: usize, out: &mut Vec<String>) {
        unsafe {
            for i in 0..GetMenuItemCount(m) {
                let mut buf = [0u16; 128];
                let n = GetMenuStringW(m, i as u32, Some(&mut buf), MF_BYPOSITION);
                let checked = GetMenuState(m, i as u32, MF_BYPOSITION) & MF_CHECKED.0 != 0;
                let text = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
                out.push(format!("{}{}{}", "  ".repeat(depth), if checked { "✓ " } else { "" }, if text.is_empty() { "—" } else { &text }));
                let sub = GetSubMenu(m, i);
                if !sub.is_invalid() {
                    menu_texts(sub, depth + 1, out);
                }
            }
        }
    }

    #[test]
    fn tray_menu_contents() {
        let cfg = Config::from_ini(&IniDoc::parse(config::DEFAULT_INI));
        let app = App {
            hwnd: HWND::default(),
            hinst: HINSTANCE::default(),
            cfg,
            doc: IniDoc::default(),
            path: PathBuf::new(),
            cameras: vec![CameraInfo {
                name: "brama".into(),
                title: "Brama".into(),
                main: None,
                sub: None,
                aspect: 1.7,
                labels: vec![],
                zones: vec![],
            }],
            connected: true,
            status: "http://frigate:5000".into(),
            pause: Pause::Off,
            popups: Vec::new(),
            listener: Cancel::new(),
            listener_key: String::new(),
            hubs: HashMap::new(),
            icons: [HICON::default(); 3],
            pending_test: None,
            gpu_warm: None,
            settings: None,
            view: None,
            update: None,
            update_status: update::Status::Idle,
            update_notified: None,
            update_balloon: false,
        };
        i18n::set(i18n::Lang::En);
        let m = app.build_menu();
        let mut items = Vec::new();
        menu_texts(m, 0, &mut items);
        unsafe {
            let _ = DestroyMenu(m);
        }
        let all = items.join("\n");
        println!("{all}");
        for expected in ["Connected: http://frigate:5000", "Settings…", "Show camera", "  Brama", "Pause notifications", "Help", "Exit"] {
            assert!(items.iter().any(|i| i.ends_with(expected) || i == expected), "missing item: {expected}\n{all}");
        }
    }
}
