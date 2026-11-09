//! Pictures in memory (BGRA) and the caption drawn on them.

use crate::view::Zoom;

/// Picture in BGRA, 4 bytes per pixel, rows without padding.
#[derive(Clone)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl Image {
    /// The part `want` of the camera picture from a picture that shows the part `have`, at
    /// the same size (bilinear, 8-bit fixed point). What `have` does not cover comes from
    /// `whole` (an older picture of the whole camera view), or stays black: zooming out shows
    /// at once, with the live part at its place.
    pub fn zoomed(&self, have: Zoom, want: Zoom, whole: Option<&Image>) -> Image {
        if have == want || self.w == 0 || self.h == 0 {
            return self.clone();
        }
        let (w, h) = (self.w, self.h);
        // For every output column (row): the two source columns and the weight of the second (0–256).
        let axis = |n: u32, want_from: f32, have_from: f32| -> Vec<Option<(u32, u32, u32)>> {
            (0..n)
                .map(|i| {
                    let f = (want_from + (i as f32 + 0.5) / n as f32 * want.size - have_from) / have.size;
                    if !(0.0..=1.0).contains(&f) {
                        return None;
                    }
                    let s = (f * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
                    let a = s as u32;
                    Some((a, (a + 1).min(n - 1), ((s - a as f32) * 256.0) as u32))
                })
                .collect()
        };
        let (cols, rows) = (axis(w, want.x, have.x), axis(h, want.y, have.y));
        let gaps = cols.iter().chain(rows.iter()).any(Option::is_none);
        let mut out = match whole {
            Some(whole) if gaps => whole.resized(w, h).zoomed(Zoom::FULL, want, None),
            _ => Image::new(w, h),
        };
        for (y, row) in rows.iter().enumerate() {
            let Some((ya, yb, ty)) = *row else { continue };
            for (x, col) in cols.iter().enumerate() {
                let Some((xa, xb, tx)) = *col else { continue };
                let at = |px: u32, py: u32| ((py * w + px) * 4) as usize;
                let (a, b, c, d) = (at(xa, ya), at(xb, ya), at(xa, yb), at(xb, yb));
                let o = (y * w as usize + x) * 4;
                for k in 0..4 {
                    let top = self.data[a + k] as u32 * (256 - tx) + self.data[b + k] as u32 * tx;
                    let bottom = self.data[c + k] as u32 * (256 - tx) + self.data[d + k] as u32 * tx;
                    out.data[o + k] = ((top * (256 - ty) + bottom * ty + (1 << 15)) >> 16) as u8;
                }
            }
        }
        out
    }

    pub fn new(w: u32, h: u32) -> Image {
        Image { w, h, data: vec![0; (w * h * 4) as usize] }
    }

    /// (r, g, b) of a pixel.
    pub fn rgb(&self, x: u32, y: u32) -> (u8, u8, u8) {
        let i = ((y * self.w + x) * 4) as usize;
        (self.data[i + 2], self.data[i + 1], self.data[i])
    }

    /// Scaled copy (area average when shrinking, nearest pixel when enlarging).
    pub fn resized(&self, w: u32, h: u32) -> Image {
        if (w, h) == (self.w, self.h) || self.w == 0 || self.h == 0 {
            return self.clone();
        }
        let mut out = Image::new(w.max(1), h.max(1));
        for y in 0..out.h {
            let y0 = y * self.h / out.h;
            let y1 = ((y + 1) * self.h / out.h).max(y0 + 1).min(self.h);
            for x in 0..out.w {
                let x0 = x * self.w / out.w;
                let x1 = ((x + 1) * self.w / out.w).max(x0 + 1).min(self.w);
                let mut sum = [0u32; 4];
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let i = ((sy * self.w + sx) * 4) as usize;
                        for (k, s) in sum.iter_mut().enumerate() {
                            *s += self.data[i + k] as u32;
                        }
                    }
                }
                let n = (y1 - y0) * (x1 - x0);
                let o = ((y * out.w + x) * 4) as usize;
                for (k, s) in sum.iter().enumerate() {
                    out.data[o + k] = (s / n) as u8;
                }
            }
        }
        out
    }

    /// Darkens a rectangle (background of the caption).
    fn shade(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, keep: f32) {
        for y in y0.max(0)..y1.min(self.h as i32) {
            for x in x0.max(0)..x1.min(self.w as i32) {
                let i = ((y as u32 * self.w + x as u32) * 4) as usize;
                for k in 0..3 {
                    self.data[i + k] = (self.data[i + k] as f32 * keep) as u8;
                }
            }
        }
    }

    /// Blends `color` (r, g, b) into a pixel with coverage `alpha` (0–255).
    fn blend(&mut self, x: i32, y: i32, color: (u8, u8, u8), alpha: u8) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || alpha == 0 {
            return;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        let a = alpha as u32;
        for (k, c) in [color.2, color.1, color.0].into_iter().enumerate() {
            self.data[i + k] = ((c as u32 * a + self.data[i + k] as u32 * (255 - a)) / 255) as u8;
        }
    }

    fn disc(&mut self, cx: f32, cy: f32, r: f32, color: (u8, u8, u8)) {
        self.disc_alpha(cx, cy, r, color, 1.0);
    }

    /// A filled circle with opacity `opacity` (0–1), smooth edges.
    fn disc_alpha(&mut self, cx: f32, cy: f32, r: f32, color: (u8, u8, u8), opacity: f32) {
        for y in (cy - r - 1.0) as i32..=(cy + r + 1.0) as i32 {
            for x in (cx - r - 1.0) as i32..=(cx + r + 1.0) as i32 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cover = (r - d + 0.5).clamp(0.0, 1.0) * opacity;
                self.blend(x, y, color, (cover * 255.0) as u8);
            }
        }
    }

    /// A line `width` pixels wide with smooth edges.
    fn line(&mut self, (x0, y0): (f32, f32), (x1, y1): (f32, f32), width: f32, color: (u8, u8, u8)) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = (dx * dx + dy * dy).max(1e-6);
        let h = width / 2.0;
        for y in (y0.min(y1) - h - 1.0) as i32..=(y0.max(y1) + h + 1.0) as i32 {
            for x in (x0.min(x1) - h - 1.0) as i32..=(x0.max(x1) + h + 1.0) as i32 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = (((px - x0) * dx + (py - y0) * dy) / len2).clamp(0.0, 1.0);
                let d = ((px - x0 - t * dx).powi(2) + (py - y0 - t * dy).powi(2)).sqrt();
                self.blend(x, y, color, ((h - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
    }

    /// A filled polygon (4×4 samples per pixel for smooth edges).
    fn polygon(&mut self, points: &[(f32, f32)], color: (u8, u8, u8)) {
        let inside = |x: f32, y: f32| {
            let mut c = false;
            let mut j = points.len() - 1;
            for i in 0..points.len() {
                let ((xi, yi), (xj, yj)) = (points[i], points[j]);
                if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                    c = !c;
                }
                j = i;
            }
            c
        };
        let (x0, x1) = points.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (y0, y1) = points.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        for y in y0.floor() as i32..=y1.ceil() as i32 {
            for x in x0.floor() as i32..=x1.ceil() as i32 {
                let hits = (0..16).filter(|i| inside(x as f32 + (i % 4) as f32 / 4.0 + 0.125, y as f32 + (i / 4) as f32 / 4.0 + 0.125)).count();
                self.blend(x, y, color, (hits * 255 / 16) as u8);
            }
        }
    }

    /// The right part of a ring (angles within ±`span` radians), `width` pixels thick.
    fn arc(&mut self, (cx, cy): (f32, f32), r: f32, span: f32, width: f32, color: (u8, u8, u8)) {
        let h = width / 2.0;
        for y in (cy - r - h - 1.0) as i32..=(cy + r + h + 1.0) as i32 {
            for x in cx as i32..=(cx + r + h + 1.0) as i32 {
                let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if py.atan2(px).abs() > span {
                    continue;
                }
                let d = ((px * px + py * py).sqrt() - r).abs();
                self.blend(x, y, color, ((h - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
    }

    /// A ring `width` pixels thick, drawn clockwise (y down) from angle `from` to `to` (radians).
    fn ring(&mut self, (cx, cy): (f32, f32), r: f32, from: f32, to: f32, width: f32, color: (u8, u8, u8)) {
        let h = width / 2.0;
        let sweep = (to - from).rem_euclid(std::f32::consts::TAU);
        for y in (cy - r - h - 1.0) as i32..=(cy + r + h + 1.0) as i32 {
            for x in (cx - r - h - 1.0) as i32..=(cx + r + h + 1.0) as i32 {
                let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if (py.atan2(px) - from).rem_euclid(std::f32::consts::TAU) > sweep {
                    continue;
                }
                let d = ((px * px + py * py).sqrt() - r).abs();
                self.blend(x, y, color, ((h - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
    }

    /// Round replay button: an anticlockwise arrow, lit while replaying or under the mouse.
    pub fn replay_button(&mut self, (x0, y0, x1, _): (i32, i32, i32, i32), running: bool, hot: bool) {
        let (cx, cy, rad) = ((x0 + x1) as f32 / 2.0, y0 as f32 + (x1 - x0) as f32 / 2.0, (x1 - x0) as f32 / 2.0);
        self.disc_alpha(cx, cy, rad, (0, 0, 0), 0.55);
        if hot || running {
            self.disc_alpha(cx, cy, rad, (255, 255, 255), 0.3);
        }
        let icon = crate::view::replay_icon((cx, cy), rad);
        let white = (255, 255, 255);
        self.ring((cx, cy), icon.radius, icon.from, icon.to, (rad / 7.0).max(1.5), white);
        self.polygon(&icon.head, white);
    }

    /// Round close button (×) in `rect` (x0, y0, x1, y1); red under the mouse — like on Windows.
    pub fn close_button(&mut self, (x0, y0, x1, _): (i32, i32, i32, i32), hot: bool) {
        let (cx, cy, rad) = ((x0 + x1) as f32 / 2.0, y0 as f32 + (x1 - x0) as f32 / 2.0, (x1 - x0) as f32 / 2.0);
        if hot {
            self.disc(cx, cy, rad, (239, 68, 68));
        } else {
            self.disc_alpha(cx, cy, rad, (0, 0, 0), 0.55);
        }
        let (arm, width) = (rad * 0.38, (rad / 7.0).max(1.5));
        self.line((cx - arm, cy - arm), (cx + arm, cy + arm), width, (255, 255, 255));
        self.line((cx - arm, cy + arm), (cx + arm, cy - arm), width, (255, 255, 255));
    }

    /// Volume slider: a dark bar with round ends, the track and the knob at `volume` percent.
    pub fn volume_slider(&mut self, slider: (i32, i32, i32, i32), volume: u8) {
        let (l, t, r, b) = slider;
        let rad = (r - l) as f32 / 2.0;
        for y in t..b {
            let off = (rad - (y - t).min(b - 1 - y) as f32 - 0.5).max(0.0);
            let inset = (rad - (rad * rad - off * off).sqrt()) as i32;
            self.shade(l + inset, y, r - inset, y + 1, 0.45);
        }
        let (top, bottom) = crate::view::volume_track(slider);
        let (x, level) = (l as f32 + rad, crate::view::volume_y(slider, volume));
        let (width, white) = ((rad / 7.0).max(1.5), (255, 255, 255));
        self.line((x, top as f32), (x, bottom as f32), width, (190, 190, 190));
        self.line((x, level), (x, bottom as f32), width, white);
        self.disc(x, level, rad * 0.45, white);
    }

    /// Round sound button: a speaker with waves, or crossed out when muted.
    pub fn sound_button(&mut self, (x0, y0, x1, _): (i32, i32, i32, i32), muted: bool, hot: bool) {
        let (cx, cy, rad) = ((x0 + x1) as f32 / 2.0, y0 as f32 + (x1 - x0) as f32 / 2.0, (x1 - x0) as f32 / 2.0);
        self.disc_alpha(cx, cy, rad, (0, 0, 0), 0.55);
        if hot {
            self.disc_alpha(cx, cy, rad, (255, 255, 255), 0.3);
        }
        let s = rad * 0.46;
        let o = cx - 0.2 * s;
        let width = (rad / 7.0).max(1.5);
        let white = (255, 255, 255);
        self.polygon(
            &[
                (o - 0.75 * s, cy - 0.32 * s),
                (o - 0.25 * s, cy - 0.32 * s),
                (o + 0.3 * s, cy - 0.8 * s),
                (o + 0.3 * s, cy + 0.8 * s),
                (o - 0.25 * s, cy + 0.32 * s),
                (o - 0.75 * s, cy + 0.32 * s),
            ],
            white,
        );
        if muted {
            let (x, arm) = (o + 0.85 * s, 0.3 * s);
            self.line((x - arm, cy - arm), (x + arm, cy + arm), width, white);
            self.line((x - arm, cy + arm), (x + arm, cy - arm), width, white);
        } else {
            self.arc((o + 0.3 * s, cy), 0.42 * s, 0.9, width, white);
            self.arc((o + 0.3 * s, cy), 0.8 * s, 0.9, width, white);
        }
    }
}

/// Caption font, loaded from a TrueType file installed on the system.
pub struct Font {
    font: fontdue::Font,
}

/// Bold sans-serif fonts found on common distributions (DejaVu comes with ffmpeg's
/// dependencies on Debian, Ubuntu and Raspberry Pi OS).
const FONTS: [&str; 8] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Bold.ttf",
];

impl Font {
    /// `custom` (from the settings) or the first font found.
    pub fn load(custom: Option<&str>) -> Option<Font> {
        let paths = custom.into_iter().chain(FONTS);
        for p in paths {
            if let Ok(bytes) = std::fs::read(p) {
                match fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                    Ok(font) => {
                        crate::log!("Caption font: {p}");
                        return Some(Font { font });
                    }
                    Err(e) => crate::log!("Cannot read font {p}: {e}"),
                }
            }
        }
        crate::log!("No caption font found - install fonts-dejavu-core or set [console] font");
        None
    }

    /// Caption in the top-left corner: a dark rounded box, a dot (red while the
    /// picture is live) and the text — like the Windows version. `reserve` = pixels
    /// kept free on the right (buttons).
    pub fn caption(&self, img: &mut Image, text: &str, live: bool, reserve: f32) {
        let px = (img.h as f32 / 20.0).clamp(12.0, 36.0);
        let pad = (px * 0.6).round();
        let dot = px * 0.62;
        let max_w = img.w as f32 - pad * 4.0 - dot * 2.0 - reserve;
        let line = self.font.horizontal_line_metrics(px).map(|m| m.ascent).unwrap_or(px * 0.8);
        // Glyphs that still fit into the width.
        let mut glyphs = Vec::new();
        let mut width = 0.0;
        for ch in text.chars() {
            let (m, bitmap) = self.font.rasterize(ch, px);
            if width + m.advance_width > max_w {
                break;
            }
            glyphs.push((width, m, bitmap));
            width += m.advance_width;
        }
        let x0 = pad;
        let y0 = pad;
        let box_h = px * 1.3 + pad * 0.7;
        let box_w = pad * 0.8 + dot + pad * 0.6 + width + pad * 0.9;
        img.shade(x0 as i32, y0 as i32, (x0 + box_w) as i32, (y0 + box_h) as i32, 0.45);
        let cy = y0 + box_h / 2.0;
        img.disc(x0 + pad * 0.8 + dot / 2.0, cy, dot / 2.0, if live { (239, 68, 68) } else { (191, 191, 191) });
        let tx = x0 + pad * 0.8 + dot + pad * 0.6;
        let baseline = (cy + line / 2.0 - px * 0.1).round();
        for (gx, m, bitmap) in glyphs {
            let left = (tx + gx).round() as i32 + m.xmin;
            let top = baseline as i32 - m.height as i32 - m.ymin;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bitmap[row * m.width + col];
                    img.blend(left + col as i32, top + row as i32, (255, 255, 255), a);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoomed_picture() {
        // 8×2: left half black, right half white.
        let mut img = Image::new(8, 2);
        for y in 0..2 {
            for x in 4..8 {
                img.data[((y * 8 + x) * 4) as usize..][..4].copy_from_slice(&[255; 4]);
            }
        }
        let row = |im: &Image, y: u32| im.data[(y * 8 * 4) as usize..((y + 1) * 8 * 4) as usize].to_vec();
        // Zoomed in on the right half: white (the first column blends a little of the black edge).
        let right = Zoom { x: 0.5, y: 0.0, size: 0.5 };
        let z = img.zoomed(Zoom::FULL, right, None);
        for y in 0..2 {
            assert!(row(&z, y)[4..].iter().all(|&b| b == 255) && row(&z, y)[0] > 128);
        }
        // Back out from that picture: its content goes to the right half, the left half is
        // black for now — or comes from an older picture of the whole view.
        let back = z.zoomed(right, Zoom::FULL, None);
        assert!(row(&back, 0)[..16].iter().all(|&b| b == 0));
        assert!(row(&back, 0)[20..].iter().all(|&b| b == 255));
        let mut grey = Image::new(8, 2);
        grey.data.fill(128);
        let filled = z.zoomed(right, Zoom::FULL, Some(&grey));
        assert!(row(&filled, 0)[..12].iter().all(|&b| b == 128));
        assert!(row(&filled, 0)[20..].iter().all(|&b| b == 255));
        assert_eq!(img.zoomed(Zoom::FULL, Zoom::FULL, None).data, img.data);
    }

    #[test]
    fn resize_averages_when_shrinking() {
        let mut img = Image::new(2, 2);
        img.data = vec![0, 0, 0, 255, 200, 200, 200, 255, 0, 0, 0, 255, 200, 200, 200, 255];
        let small = img.resized(1, 1);
        assert_eq!(small.data, vec![100, 100, 100, 255]);
        assert_eq!(img.resized(4, 4).rgb(3, 3), (200, 200, 200));
    }
}
