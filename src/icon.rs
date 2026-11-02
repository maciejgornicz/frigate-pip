//! Tray icon drawn in code (a screen with a small PiP window): an HICON on Windows, the
//! pixels of the StatusNotifier icon on Linux.

#[cfg(windows)]
use windows::Win32::Foundation::{BOOL, HANDLE};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::*;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// Whether the icon covers the point (x, y), coordinates in the range 0..size.
fn covers(x: f32, y: f32, s: f32) -> bool {
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
    (outer && !inner) || pip
}

/// Coverage of every pixel (0–255), 2×2 supersampled for smooth edges; rows top to bottom.
pub fn coverage(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut out = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let hits =
                [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)].iter().filter(|(dx, dy)| covers(x as f32 + dx, y as f32 + dy, s)).count()
                    as u32;
            out.push((hits * 255 / 4) as u8);
        }
    }
    out
}

#[cfg(windows)]
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
        for (p, a) in px.iter_mut().zip(coverage(size as u32)) {
            // ARGB, premultiplied
            let alpha = a as u32;
            let ch = |c: u8| c as u32 * alpha / 255;
            *p = if alpha == 0 { 0 } else { alpha << 24 | ch(rgb.0) << 16 | ch(rgb.1) << 8 | ch(rgb.2) };
        }
        let mask = CreateBitmap(size, size, 1, 1, None);
        let info = ICONINFO { fIcon: BOOL(1), xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        let _ = DeleteObject(mask);
        let _ = DeleteObject(color);
        icon
    }
}
