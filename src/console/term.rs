//! Terminal output: preview windows inside a terminal — Kitty graphics, Sixel or
//! coloured half blocks (any terminal with 24-bit colour) — a status line at the
//! bottom and single-key commands.

use super::canvas::Image;
use super::{Output, Rect};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Write;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Graphics {
    Kitty,
    Sixel,
    Blocks,
}

impl Graphics {
    pub fn name(self) -> &'static str {
        match self {
            Graphics::Kitty => "kitty",
            Graphics::Sixel => "sixel",
            Graphics::Blocks => "blocks",
        }
    }
}

/// A window's place in character cells.
#[derive(Clone, Copy, PartialEq)]
struct Cells {
    col: u32,
    row: u32,
    cols: u32,
    rows: u32,
}

pub struct Terminal {
    graphics: Graphics,
    cols: u32,
    rows: u32,
    /// Size of one character cell in pixels.
    cell: (u32, u32),
    cell_known: bool,
    saved: Option<libc::termios>,
    shown: HashMap<u64, Cells>,
}

fn winsize() -> Option<libc::winsize> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ as _, &mut ws) } == 0;
    (ok && ws.ws_col > 0 && ws.ws_row > 0).then_some(ws)
}

/// Reads what the terminal answers within `ms` milliseconds (stdin in raw mode).
fn read_reply(ms: i32, done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut out = Vec::new();
    let start = std::time::Instant::now();
    while !done(&out) {
        let left = ms - start.elapsed().as_millis() as i32;
        if left <= 0 {
            break;
        }
        let mut pfd = libc::pollfd { fd: libc::STDIN_FILENO, events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut pfd, 1, left) } <= 0 {
            break;
        }
        let mut buf = [0u8; 256];
        let n = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    out
}

/// "ESC [ 6 ; h ; w t" → cell size (w, h).
fn parse_cell_size(reply: &[u8]) -> Option<(u32, u32)> {
    let s = String::from_utf8_lossy(reply);
    let i = s.find("\x1b[6;")?;
    let rest = &s[i + 4..];
    let end = rest.find('t')?;
    let mut parts = rest[..end].split(';').map(|v| v.parse::<u32>().ok());
    let (h, w) = (parts.next()??, parts.next()??);
    (w > 0 && h > 0).then_some((w, h))
}

/// Device attributes ("ESC [ ? 62 ; 4 ; … c"): 4 = Sixel.
fn has_sixel(reply: &[u8]) -> bool {
    let s = String::from_utf8_lossy(reply);
    let Some(i) = s.find("\x1b[?") else { return false };
    let Some(end) = s[i..].find('c') else { return false };
    s[i + 3..i + end].split(';').any(|p| p == "4")
}

impl Terminal {
    /// Takes over the terminal: raw keys, alternate screen, hidden cursor.
    /// `pref` = auto, kitty, sixel or blocks.
    pub fn open(pref: &str) -> Result<Terminal, String> {
        if unsafe { libc::isatty(libc::STDOUT_FILENO) } != 1 {
            return Err("standard output is not a terminal".into());
        }
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        let have_tty = unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut saved) } == 0;
        if have_tty {
            let mut raw = saved;
            // Keys arrive at once and are not echoed; Ctrl+C still sends SIGINT.
            raw.c_lflag &= !(libc::ICANON | libc::ECHO);
            raw.c_cc[libc::VMIN] = 1;
            raw.c_cc[libc::VTIME] = 0;
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) };
        }
        let ws = winsize().ok_or("cannot read the terminal size")?;
        let mut cell = (ws.ws_xpixel as u32 / ws.ws_col as u32, ws.ws_ypixel as u32 / ws.ws_row as u32);
        let mut cell_known = cell.0 > 0 && cell.1 > 0;
        let mut sixel = false;
        if have_tty {
            // Cell size (if the kernel does not know it) and device attributes.
            let mut out = std::io::stdout();
            let _ = out.write_all(b"\x1b[16t\x1b[c");
            let _ = out.flush();
            let reply = read_reply(400, |r| r.contains(&b'c') && r.windows(3).any(|w| w == b"\x1b[?"));
            if !cell_known {
                if let Some(c) = parse_cell_size(&reply) {
                    cell = c;
                    cell_known = true;
                }
            }
            sixel = has_sixel(&reply);
        }
        if !cell_known {
            cell = (9, 18);
        }
        let kitty = std::env::var_os("KITTY_WINDOW_ID").is_some()
            || std::env::var("TERM").is_ok_and(|t| t.contains("kitty") || t.contains("ghostty"))
            || std::env::var("TERM_PROGRAM").is_ok_and(|t| t == "WezTerm" || t == "ghostty");
        let graphics = match pref {
            "kitty" => Graphics::Kitty,
            "sixel" => Graphics::Sixel,
            "blocks" => Graphics::Blocks,
            _ if kitty => Graphics::Kitty,
            _ if sixel => Graphics::Sixel,
            _ => Graphics::Blocks,
        };
        crate::log!(
            "Terminal {}x{} cells, cell {}x{} px{}, graphics {}",
            ws.ws_col,
            ws.ws_row,
            cell.0,
            cell.1,
            if cell_known { "" } else { " (assumed)" },
            graphics.name()
        );
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J");
        let _ = out.flush();
        Ok(Terminal {
            graphics,
            cols: ws.ws_col as u32,
            rows: ws.ws_row as u32,
            cell,
            cell_known,
            saved: have_tty.then_some(saved),
            shown: HashMap::new(),
        })
    }

    /// Window rectangle (pixels) → character cells; the bottom row is the status line.
    fn cells(&self, r: Rect) -> Cells {
        let (cw, ch) = self.cell;
        let col = (r.x.max(0) as u32 / cw).min(self.cols.saturating_sub(4));
        let row = (r.y.max(0) as u32 / ch).min(self.rows.saturating_sub(4));
        let cols = r.w.div_ceil(cw).clamp(4, self.cols - col);
        let rows = r.h.div_ceil(ch).clamp(3, self.rows.saturating_sub(1) - row);
        Cells { col, row, cols, rows }
    }

    fn erase(&self, c: Cells, buf: &mut String) {
        buf.push_str("\x1b[0m");
        for r in 0..c.rows {
            let _ = write!(buf, "\x1b[{};{}H{}", c.row + r + 1, c.col + 1, " ".repeat(c.cols as usize));
        }
    }

    fn caption_row(&self, c: Cells, caption: &str, live: bool, buf: &mut String) {
        let _ = write!(buf, "\x1b[{};{}H\x1b[0;97;40m", c.row + 1, c.col + 1);
        buf.push_str(if live { "\x1b[91m●\x1b[97m " } else { "\x1b[37m●\x1b[97m " });
        let text: String = caption.chars().take(c.cols.saturating_sub(2) as usize).collect();
        let pad = (c.cols as usize).saturating_sub(2 + text.chars().count());
        buf.push_str(&text);
        buf.push_str(&" ".repeat(pad));
        buf.push_str("\x1b[0m");
    }

    fn kitty(&self, id: u64, img: &Image, buf: &mut String) {
        let mut rgb = Vec::with_capacity((img.w * img.h * 3) as usize);
        for px in img.data.chunks_exact(4) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        let data = crate::util::base64_encode(&rgb);
        let chunks: Vec<&[u8]> = data.as_bytes().chunks(4096).collect();
        for (i, chunk) in chunks.iter().enumerate() {
            let more = if i + 1 < chunks.len() { 1 } else { 0 };
            if i == 0 {
                let _ = write!(buf, "\x1b_Ga=T,f=24,s={},v={},i={},p=1,q=2,C=1,m={more};", img.w, img.h, id as u32);
            } else {
                let _ = write!(buf, "\x1b_Gm={more};");
            }
            buf.push_str(std::str::from_utf8(chunk).unwrap_or_default());
            buf.push_str("\x1b\\");
        }
    }

    fn blocks(&self, img: &Image, c: Cells, buf: &mut String) {
        // Each cell shows two pixels: the upper half (▀ in the foreground colour) and the lower one.
        let mut last = (None, None);
        for r in 0..img.h / 2 {
            let _ = write!(buf, "\x1b[{};{}H", c.row + 2 + r, c.col + 1);
            for x in 0..img.w {
                let top = img.rgb(x, r * 2);
                let bottom = img.rgb(x, r * 2 + 1);
                if last.0 != Some(top) {
                    let _ = write!(buf, "\x1b[38;2;{};{};{}m", top.0, top.1, top.2);
                }
                if last.1 != Some(bottom) {
                    let _ = write!(buf, "\x1b[48;2;{};{};{}m", bottom.0, bottom.1, bottom.2);
                }
                last = (Some(top), Some(bottom));
                buf.push('▀');
            }
            last = (None, None);
            buf.push_str("\x1b[0m");
        }
    }
}

/// Single keys from the terminal, read on a thread. With `raw` it also switches the
/// terminal to raw mode and hides the cursor (the framebuffer output on a console;
/// the terminal output does that itself).
pub struct Keys {
    saved: Option<libc::termios>,
}

impl Keys {
    pub fn start(raw: bool, on_key: impl Fn(u8) + Send + 'static) -> Keys {
        let mut saved = None;
        if raw {
            let mut t: libc::termios = unsafe { std::mem::zeroed() };
            if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut t) } == 0 {
                let mut r = t;
                r.c_lflag &= !(libc::ICANON | libc::ECHO);
                r.c_cc[libc::VMIN] = 1;
                r.c_cc[libc::VTIME] = 0;
                unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &r) };
                saved = Some(t);
                let mut out = std::io::stdout();
                let _ = out.write_all(b"\x1b[?25l");
                let _ = out.flush();
            }
        }
        let _ = std::thread::Builder::new().name("keys".into()).spawn(move || loop {
            let mut b = [0u8; 1];
            if unsafe { libc::read(libc::STDIN_FILENO, b.as_mut_ptr() as *mut _, 1) } <= 0 {
                return;
            }
            on_key(b[0]);
        });
        Keys { saved }
    }
}

impl Drop for Keys {
    fn drop(&mut self) {
        if let Some(t) = &self.saved {
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, t) };
            let mut out = std::io::stdout();
            let _ = out.write_all(b"\x1b[?25h");
            let _ = out.flush();
        }
    }
}

/// 4×4 ordered dithering thresholds (Bayer matrix).
const BAYER: [[i32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Sixel picture with a fixed palette of 6×6×6 colours (ordered dithering hides the steps).
fn sixel(img: &Image) -> String {
    let w = img.w as usize;
    let level = |v: u8, t: i32| ((v as i32 + (t * 2 - 15) * 51 / 32).clamp(0, 255) as u32 * 5 + 127) as usize / 255;
    let index: Vec<u8> = img
        .data
        .chunks_exact(4)
        .enumerate()
        .map(|(i, p)| {
            let t = BAYER[(i / w) % 4][(i % w) % 4];
            (level(p[2], t) * 36 + level(p[1], t) * 6 + level(p[0], t)) as u8
        })
        .collect();
    let mut used = [false; 216];
    for &i in &index {
        used[i as usize] = true;
    }
    let mut s = String::with_capacity(index.len());
    let _ = write!(s, "\x1bPq\"1;1;{};{}", img.w, img.h);
    for (i, _) in used.iter().enumerate().filter(|(_, u)| **u) {
        let pct = |l: usize| l * 100 / 5;
        let _ = write!(s, "#{i};2;{};{};{}", pct(i / 36), pct(i / 6 % 6), pct(i % 6));
    }
    let h = img.h as usize;
    let mut bits = vec![0u8; w];
    for band in (0..h).step_by(6) {
        let mut colors: Vec<usize> = (band..(band + 6).min(h)).flat_map(|y| index[y * w..(y + 1) * w].iter().map(|&c| c as usize)).collect();
        colors.sort_unstable();
        colors.dedup();
        for (n, &color) in colors.iter().enumerate() {
            bits.iter_mut().for_each(|b| *b = 0);
            for dy in 0..6.min(h - band) {
                let row = &index[(band + dy) * w..(band + dy + 1) * w];
                for (x, &c) in row.iter().enumerate() {
                    if c as usize == color {
                        bits[x] |= 1 << dy;
                    }
                }
            }
            let _ = write!(s, "#{color}");
            // Run-length encoding: "!count char".
            let mut x = 0;
            while x < w {
                let ch = bits[x];
                let mut run = 1;
                while x + run < w && bits[x + run] == ch {
                    run += 1;
                }
                let c = (63 + ch) as char;
                if run > 3 {
                    let _ = write!(s, "!{run}{c}");
                } else {
                    (0..run).for_each(|_| s.push(c));
                }
                x += run;
            }
            s.push(if n + 1 < colors.len() { '$' } else { '-' });
        }
    }
    s.push_str("\x1b\\");
    s
}

impl Output for Terminal {
    fn size(&self) -> (u32, u32) {
        (self.cols * self.cell.0, self.rows.saturating_sub(1) * self.cell.1)
    }

    fn caption_height(&self) -> u32 {
        self.cell.1
    }

    fn min_interval(&self) -> Duration {
        // A terminal is no video player: about 10 pictures a second (15 for blocks).
        Duration::from_millis(if self.graphics == Graphics::Blocks { 66 } else { 100 })
    }

    fn draw(&mut self, id: u64, rect: Rect, img: &Image, caption: &str, live: bool) {
        let c = self.cells(rect);
        let mut buf = String::new();
        if let Some(old) = self.shown.get(&id).copied() {
            if old != c {
                self.erase(old, &mut buf);
                if self.graphics == Graphics::Kitty {
                    let _ = write!(buf, "\x1b_Ga=d,d=I,i={},q=2\x1b\\", id as u32);
                }
            }
        }
        self.caption_row(c, caption, live, &mut buf);
        let image_rows = c.rows - 1;
        let _ = write!(buf, "\x1b[{};{}H", c.row + 2, c.col + 1);
        match self.graphics {
            Graphics::Kitty => self.kitty(id, &img.resized(c.cols * self.cell.0, image_rows * self.cell.1), &mut buf),
            Graphics::Sixel => buf.push_str(&sixel(&img.resized(c.cols * self.cell.0, image_rows * self.cell.1))),
            Graphics::Blocks => self.blocks(&img.resized(c.cols, image_rows * 2), c, &mut buf),
        }
        self.shown.insert(id, c);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(buf.as_bytes());
        let _ = out.flush();
    }

    fn clear(&mut self, id: u64) {
        let Some(c) = self.shown.remove(&id) else { return };
        let mut buf = String::new();
        if self.graphics == Graphics::Kitty {
            let _ = write!(buf, "\x1b_Ga=d,d=I,i={},q=2\x1b\\", id as u32);
        }
        self.erase(c, &mut buf);
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(buf.as_bytes());
        let _ = out.flush();
    }

    fn status(&mut self, text: &str) {
        let text: String = text.chars().take(self.cols as usize).collect();
        let pad = (self.cols as usize).saturating_sub(text.chars().count());
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "\x1b[{};1H\x1b[0;7m{text}{}\x1b[0m", self.rows, " ".repeat(pad));
        let _ = out.flush();
    }

    fn refresh(&mut self) -> bool {
        let Some(ws) = winsize() else { return false };
        let (cols, rows) = (ws.ws_col as u32, ws.ws_row as u32);
        if (cols, rows) == (self.cols, self.rows) {
            return false;
        }
        self.cols = cols;
        self.rows = rows;
        if ws.ws_xpixel > 0 && ws.ws_ypixel > 0 {
            self.cell = (ws.ws_xpixel as u32 / cols, ws.ws_ypixel as u32 / rows);
            self.cell_known = true;
        }
        self.shown.clear();
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(b"\x1b_Ga=d,d=A,q=2\x1b\\\x1b[0m\x1b[2J");
        let _ = out.flush();
        true
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let mut out = std::io::stdout().lock();
        if self.graphics == Graphics::Kitty {
            let _ = out.write_all(b"\x1b_Ga=d,d=A,q=2\x1b\\");
        }
        let _ = out.write_all(b"\x1b[0m\x1b[2J\x1b[?25h\x1b[?1049l");
        let _ = out.flush();
        if let Some(t) = &self.saved {
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, t) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_replies() {
        assert_eq!(parse_cell_size(b"\x1b[6;20;10t\x1b[?62;4;22c"), Some((10, 20)));
        assert!(has_sixel(b"\x1b[6;20;10t\x1b[?62;4;22c"));
        assert!(!has_sixel(b"\x1b[?62;22c"));
        assert_eq!(parse_cell_size(b"\x1b[?1;2c"), None);
    }

    #[test]
    fn kitty_transmits_rgb_with_the_window_id() {
        let t = Terminal { graphics: Graphics::Kitty, cols: 80, rows: 24, cell: (10, 20), cell_known: true, saved: None, shown: HashMap::new() };
        let mut img = Image::new(1, 1);
        img.data = vec![30, 20, 10, 255];
        let mut buf = String::new();
        t.kitty(7, &img, &mut buf);
        // RGB 10,20,30 = "ChQe" in base64.
        assert_eq!(buf, "\x1b_Ga=T,f=24,s=1,v=1,i=7,p=1,q=2,C=1,m=0;ChQe\x1b\\");
    }

    #[test]
    fn sixel_encodes_a_tiny_picture() {
        let mut img = Image::new(2, 2);
        img.data = vec![0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255];
        // Pure red = palette entry 5*36 = 180, two columns with the lower two sixel bits set.
        assert_eq!(sixel(&img), "\x1bPq\"1;1;2;2#180;2;100;0;0#180BB-\x1b\\");
    }
}
