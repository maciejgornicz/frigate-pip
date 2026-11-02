//! Pictures in memory (BGRA) and the caption drawn on them.

/// Picture in BGRA, 4 bytes per pixel, rows without padding.
#[derive(Clone)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl Image {
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
        for y in (cy - r - 1.0) as i32..=(cy + r + 1.0) as i32 {
            for x in (cx - r - 1.0) as i32..=(cx + r + 1.0) as i32 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cover = (r - d + 0.5).clamp(0.0, 1.0);
                self.blend(x, y, color, (cover * 255.0) as u8);
            }
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
    /// picture is live) and the text — like the Windows version.
    pub fn caption(&self, img: &mut Image, text: &str, live: bool) {
        let px = (img.h as f32 / 20.0).clamp(12.0, 36.0);
        let pad = (px * 0.6).round();
        let dot = px * 0.62;
        let max_w = img.w as f32 - pad * 4.0 - dot * 2.0;
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
    fn resize_averages_when_shrinking() {
        let mut img = Image::new(2, 2);
        img.data = vec![0, 0, 0, 255, 200, 200, 200, 255, 0, 0, 0, 255, 200, 200, 200, 255];
        let small = img.resized(1, 1);
        assert_eq!(small.data, vec![100, 100, 100, 255]);
        assert_eq!(img.resized(4, 4).rgb(3, 3), (200, 200, 200));
    }
}
