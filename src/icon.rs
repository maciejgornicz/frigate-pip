//! Tray icon drawn in code (a screen with a small PiP window).

use windows::Win32::Foundation::{BOOL, HANDLE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// Icon pixel (ARGB, premultiplied) for coordinates in the range 0..size.
fn pixel(x: f32, y: f32, s: f32, rgb: (u8, u8, u8)) -> u32 {
    let t = (s / 14.0).max(1.0);
    let (x0, x1, y0, y1) = (s * 0.04, s * 0.96, s * 0.14, s * 0.86);
    let r = s * 0.14;
    let inside_round = |x: f32, y: f32, l: f32, tp: f32, rt: f32, b: f32, rad: f32| -> bool {
        if x < l || x > rt || y < tp || y > b {
            return false;
        }
        let cx = x.clamp(l + rad, rt - rad);
        let cy = y.clamp(tp + rad, b - rad);
        (x - cx).powi(2) + (y - cy).powi(2) <= rad * rad
    };
    let outer = inside_round(x, y, x0, y0, x1, y1, r);
    let inner = inside_round(x, y, x0 + t, y0 + t, x1 - t, y1 - t, (r - t).max(0.0));
    let pip = inside_round(x, y, s * 0.46, s * 0.47, x1 - t * 1.8, y1 - t * 1.8, s * 0.06);
    if (outer && !inner) || pip {
        0xFF00_0000 | (rgb.0 as u32) << 16 | (rgb.1 as u32) << 8 | rgb.2 as u32
    } else {
        0
    }
}

pub fn make(size: i32, rgb: (u8, u8, u8)) -> HICON {
    unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let Ok(color) = CreateDIBSection(HDC::default(), &bmi, DIB_RGB_COLORS, &mut bits, HANDLE::default(), 0) else {
            return HICON::default();
        };
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (size * size) as usize);
        let s = size as f32;
        for y in 0..size {
            for x in 0..size {
                // 2x2 supersampling for smooth edges
                let mut a = 0u32;
                let mut c = 0u32;
                for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let v = pixel(x as f32 + dx, y as f32 + dy, s, rgb);
                    if v != 0 {
                        a += 1;
                        c = v;
                    }
                }
                px[(y * size + x) as usize] = if a == 0 {
                    0
                } else {
                    let alpha = a * 255 / 4;
                    let ch = |shift: u32| ((c >> shift) & 0xFF) * alpha / 255;
                    alpha << 24 | ch(16) << 16 | ch(8) << 8 | ch(0)
                };
            }
        }
        let mask = CreateBitmap(size, size, 1, 1, None);
        let info = ICONINFO { fIcon: BOOL(1), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        let _ = DeleteObject(mask);
        let _ = DeleteObject(color);
        icon
    }
}
