#![windows_subsystem = "windows"]

#[macro_use]
mod util;
mod app;
mod audio;
mod config;
mod decoder;
mod filter;
mod frigate;
mod h264;
mod hub;
mod i18n;
mod icon;
mod json;
mod mqtt;
mod net;
mod player;
mod render;
mod rtp;
mod rtsp;
mod settings;
mod ws;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, SendMessageW, WM_COPYDATA};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let test = args.iter().position(|a| a == "--test").and_then(|i| args.get(i + 1)).cloned();
    let quit = args.iter().any(|a| a == "--quit");
    let open_settings = args.iter().any(|a| a == "--settings");
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _mutex = CreateMutexW(None, true, w!("Local\\FrigatePiP.SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // The app is already running: forward the command to it and exit.
            if let Ok(hwnd) = FindWindowW(app::MAIN_CLASS, PCWSTR::null()) {
                if quit {
                    let _ = PostMessageW(hwnd, app::WM_APP_QUIT, WPARAM(0), LPARAM(0));
                } else if open_settings {
                    let _ = PostMessageW(hwnd, app::WM_APP_SETTINGS, WPARAM(0), LPARAM(0));
                } else if let Some(cam) = &test {
                    let cds = COPYDATASTRUCT { dwData: app::COPYDATA_TEST, cbData: cam.len() as u32, lpData: cam.as_ptr() as *mut _ };
                    SendMessageW(hwnd, WM_COPYDATA, WPARAM(0), LPARAM(&cds as *const _ as isize));
                }
            }
            return;
        }
        if quit {
            return;
        }
        util::init_log(false);
        log!("FrigatePiP {} - start", env!("CARGO_PKG_VERSION"));
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        settings::enable_visual_styles();
        app::run(test, open_settings);
    }
}
