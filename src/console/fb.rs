//! Framebuffer output (`/dev/fb0`): preview windows drawn straight onto the screen of a
//! Linux console without X or Wayland. What was under a window is saved and put back
//! when the window closes.

use super::canvas::{Font, Image};
use super::{Output, Rect};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;

#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
struct Bitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

/// `struct fb_var_screeninfo` from <linux/fb.h>.
#[repr(C)]
#[derive(Default)]
struct VarInfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: Bitfield,
    green: Bitfield,
    blue: Bitfield,
    transp: Bitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

/// `struct fb_fix_screeninfo` from <linux/fb.h>.
#[repr(C)]
#[derive(Default)]
struct FixInfo {
    id: [u8; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    type_: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: libc::c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

const FBIOGET_VSCREENINFO: u32 = 0x4600;
const FBIOGET_FSCREENINFO: u32 = 0x4602;

pub struct Framebuffer {
    _file: File,
    map: *mut u8,
    len: usize,
    width: u32,
    height: u32,
    stride: usize,
    bytes: usize,
    /// Top-left corner of the visible part (panning).
    offset: (u32, u32),
    red: Bitfield,
    green: Bitfield,
    blue: Bitfield,
    font: Option<Font>,
    /// What each window covers: (rectangle, saved screen bytes).
    saved: HashMap<u64, (Rect, Vec<u8>)>,
}

// The mapping is only used by the screen loop thread.
unsafe impl Send for Framebuffer {}

impl Framebuffer {
    /// Opens the framebuffer device. `fake_size` ("WxHxBPP") turns a regular file into
    /// a framebuffer, for tests without a screen.
    pub fn open(path: &str, fake_size: Option<&str>, font: Option<Font>) -> Result<Framebuffer, String> {
        let file = OpenOptions::new().read(true).write(true).open(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                format!("{path}: {e} - add the user to the \"video\" group (sudo usermod -aG video $USER, then log in again)")
            }
            _ => format!("{path}: {e}"),
        })?;
        let fd = file.as_raw_fd();
        let std32 = (
            Bitfield { offset: 16, length: 8, msb_right: 0 },
            Bitfield { offset: 8, length: 8, msb_right: 0 },
            Bitfield { offset: 0, length: 8, msb_right: 0 },
        );
        let (width, height, bpp, stride, len, offset, rgb) = match fake_size {
            Some(spec) => {
                let n: Vec<u32> = spec.split('x').filter_map(|v| v.trim().parse().ok()).collect();
                let [w, h, bpp] = n[..] else { return Err(format!("framebuffer_size \"{spec}\": expected WxHxBPP")) };
                if bpp != 16 && bpp != 32 {
                    return Err("framebuffer_size: 16 or 32 bits per pixel".into());
                }
                let stride = (w * bpp / 8) as usize;
                let len = stride * h as usize;
                file.set_len(len as u64).map_err(|e| e.to_string())?;
                let rgb = if bpp == 16 {
                    (
                        Bitfield { offset: 11, length: 5, msb_right: 0 },
                        Bitfield { offset: 5, length: 6, msb_right: 0 },
                        Bitfield { offset: 0, length: 5, msb_right: 0 },
                    )
                } else {
                    std32
                };
                (w, h, bpp, stride, len, (0, 0), rgb)
            }
            None => {
                let mut var = VarInfo::default();
                let mut fix = FixInfo::default();
                unsafe {
                    if libc::ioctl(fd, FBIOGET_VSCREENINFO as _, &mut var) != 0 || libc::ioctl(fd, FBIOGET_FSCREENINFO as _, &mut fix) != 0 {
                        return Err(format!("{path}: not a framebuffer ({})", std::io::Error::last_os_error()));
                    }
                }
                if var.bits_per_pixel != 16 && var.bits_per_pixel != 32 {
                    return Err(format!("{path}: {} bits per pixel are not supported (16 or 32)", var.bits_per_pixel));
                }
                let len = (fix.line_length as usize * var.yres_virtual.max(var.yres) as usize).max(fix.smem_len as usize);
                (var.xres, var.yres, var.bits_per_pixel, fix.line_length as usize, len, (var.xoffset, var.yoffset), (var.red, var.green, var.blue))
            }
        };
        let map = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, 0) };
        if map == libc::MAP_FAILED {
            return Err(format!("{path}: mmap: {}", std::io::Error::last_os_error()));
        }
        crate::log!("Framebuffer {path}: {width}x{height}, {bpp} bits, rgb {:?}/{:?}/{:?}", rgb.0, rgb.1, rgb.2);
        Ok(Framebuffer {
            _file: file,
            map: map as *mut u8,
            len,
            width,
            height,
            stride,
            bytes: (bpp / 8) as usize,
            offset,
            red: rgb.0,
            green: rgb.1,
            blue: rgb.2,
            font,
            saved: HashMap::new(),
        })
    }

    fn screen(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.map, self.len) }
    }

    /// Byte range of row `y` between columns `x0` and `x1` (already clipped).
    fn row_range(&self, x0: u32, x1: u32, y: u32) -> std::ops::Range<usize> {
        let start = (y + self.offset.1) as usize * self.stride + (x0 + self.offset.0) as usize * self.bytes;
        start..start + (x1 - x0) as usize * self.bytes
    }

    /// The part of `r` that is on the screen: (x0, y0, x1, y1).
    fn clip(&self, r: Rect) -> Option<(u32, u32, u32, u32)> {
        let x0 = r.x.max(0) as u32;
        let y0 = r.y.max(0) as u32;
        let x1 = ((r.x + r.w as i32).max(0) as u32).min(self.width);
        let y1 = ((r.y + r.h as i32).max(0) as u32).min(self.height);
        (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
    }

    fn pack(&self, r: u8, g: u8, b: u8) -> u32 {
        let c = |v: u8, f: Bitfield| ((v as u32) >> (8 - f.length.min(8))) << f.offset;
        c(r, self.red) | c(g, self.green) | c(b, self.blue)
    }

    fn restore(&mut self, id: u64) {
        if let Some((rect, saved)) = self.saved.remove(&id) {
            if let Some((x0, y0, x1, y1)) = self.clip(rect) {
                let row = (x1 - x0) as usize * self.bytes;
                for (i, y) in (y0..y1).enumerate() {
                    let range = self.row_range(x0, x1, y);
                    self.screen()[range].copy_from_slice(&saved[i * row..(i + 1) * row]);
                }
            }
        }
    }
}

impl Output for Framebuffer {
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn draw(&mut self, id: u64, rect: Rect, img: &Image, caption: &str, live: bool) {
        // A window that moved: put back what was under its old place first.
        if self.saved.get(&id).is_some_and(|(r, _)| *r != rect) {
            self.restore(id);
        }
        let Some((x0, y0, x1, y1)) = self.clip(rect) else { return };
        if !self.saved.contains_key(&id) {
            let mut saved = Vec::new();
            for y in y0..y1 {
                let range = self.row_range(x0, x1, y);
                saved.extend_from_slice(&self.screen()[range]);
            }
            self.saved.insert(id, (rect, saved));
        }
        let mut pic = if (img.w, img.h) == (rect.w, rect.h) { img.clone() } else { img.resized(rect.w, rect.h) };
        if let (Some(font), false) = (&self.font, caption.is_empty()) {
            font.caption(&mut pic, caption, live, 0.0);
        }
        let standard32 = self.bytes == 4 && (self.red.offset, self.green.offset, self.blue.offset) == (16, 8, 0);
        for y in y0..y1 {
            let sy = (y as i32 - rect.y) as u32;
            let range = self.row_range(x0, x1, y);
            let src_start = ((sy * pic.w + (x0 as i32 - rect.x) as u32) * 4) as usize;
            let src = &pic.data[src_start..src_start + (x1 - x0) as usize * 4];
            if standard32 {
                // BGRA in memory is exactly XRGB8888 (little endian).
                self.screen()[range].copy_from_slice(src);
            } else {
                let mut row = Vec::with_capacity(range.len());
                for px in src.chunks_exact(4) {
                    let v = self.pack(px[2], px[1], px[0]);
                    row.extend_from_slice(&v.to_le_bytes()[..self.bytes]);
                }
                self.screen()[range].copy_from_slice(&row);
            }
        }
    }

    fn clear(&mut self, id: u64) {
        self.restore(id);
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        let ids: Vec<u64> = self.saved.keys().copied().collect();
        for id in ids {
            self.restore(id);
        }
        unsafe {
            libc::munmap(self.map as *mut libc::c_void, self.len);
        }
    }
}
