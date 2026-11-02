//! A minimal X11 client: the protocol spoken directly over the X server's socket, without
//! libraries. Enough for borderless always-on-top preview windows on a Linux desktop —
//! X11, or Wayland through XWayland (GNOME, KDE and others start it for X11 programs).
//!
//! Requests are written by the screen loop; a thread reads the answers and the pointer
//! events of our windows.

use super::canvas::Image;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// Pointer events of our windows (coordinates: `x`, `y` in the window, `root_*` on the screen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Press {
        window: u32,
        button: u8,
        x: i32,
        y: i32,
        root_x: i32,
        root_y: i32,
        time: u32,
    },
    Release {
        window: u32,
        button: u8,
        x: i32,
        y: i32,
        root_x: i32,
        root_y: i32,
    },
    Motion {
        window: u32,
        x: i32,
        y: i32,
        root_x: i32,
        root_y: i32,
        buttons: u16,
    },
    Enter {
        window: u32,
    },
    Leave {
        window: u32,
    },
    /// Part of the window must be drawn again.
    Expose {
        window: u32,
    },
    /// The connection to the X server ended (e.g. the user logged out).
    Closed,
}

/// Button 1 held during a motion (`Event::Motion::buttons`).
pub const BUTTON1_MASK: u16 = 0x100;

/// A monitor from RandR, in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub primary: bool,
}

enum Reply {
    Ok(u16, Vec<u8>),
    Err(u16),
}

// Predefined atoms.
const ATOM: u32 = 4;
const CARDINAL: u32 = 6;
const RESOURCE_MANAGER: u32 = 23;
const STRING: u32 = 31;
const WM_NAME: u32 = 39;
const WM_CLASS: u32 = 67;

pub struct X11 {
    out: UnixStream,
    seq: u16,
    id_base: u32,
    id_mask: u32,
    next_id: u32,
    pub root: u32,
    pub root_size: (u32, u32),
    /// Longest request in bytes.
    max_request: usize,
    gc: u32,
    replies: Receiver<Reply>,
    atoms: HashMap<&'static str, u32>,
    broken: bool,
}

/// A request being built: opcode, one data byte, the length (filled in by `done`), fields.
struct Req(Vec<u8>);

impl Req {
    fn new(opcode: u8, data: u8) -> Req {
        Req(vec![opcode, data, 0, 0])
    }
    fn u8(mut self, v: u8) -> Req {
        self.0.push(v);
        self
    }
    fn u16(mut self, v: u16) -> Req {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn i16(mut self, v: i16) -> Req {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn u32(mut self, v: u32) -> Req {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn pad(mut self, n: usize) -> Req {
        self.0.resize(self.0.len() + n, 0);
        self
    }
    fn bytes(mut self, b: &[u8]) -> Req {
        self.0.extend_from_slice(b);
        let n = (4 - self.0.len() % 4) % 4;
        self.pad(n)
    }
    fn done(mut self) -> Vec<u8> {
        let n = (4 - self.0.len() % 4) % 4;
        self.0.resize(self.0.len() + n, 0);
        let len = (self.0.len() / 4) as u16;
        self.0[2..4].copy_from_slice(&len.to_le_bytes());
        self.0
    }
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn i16le(b: &[u8], at: usize) -> i32 {
    i16::from_le_bytes([b[at], b[at + 1]]) as i32
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Display number from DISPLAY (":0", ":0.0", "unix:1"); remote displays (host:0) are not supported.
pub fn display_number(display: &str) -> Option<u32> {
    let rest = display.strip_prefix("unix").unwrap_or(display).strip_prefix(':')?;
    rest.split('.').next()?.parse().ok()
}

/// MIT-MAGIC-COOKIE-1 for display `number` from an .Xauthority file: (name, data).
/// An entry for this host (or any host) is preferred; otherwise any entry for the number.
fn auth_cookie(file: &[u8], number: u32, host: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    fn field<'a>(file: &'a [u8], i: &mut usize) -> Option<&'a [u8]> {
        let n = u16::from_be_bytes([*file.get(*i)?, *file.get(*i + 1)?]) as usize;
        let v = file.get(*i + 2..*i + 2 + n)?;
        *i += 2 + n;
        Some(v)
    }
    let (mut i, mut fallback) = (0, None);
    while i + 2 <= file.len() {
        let family = u16::from_be_bytes([file[i], file[i + 1]]);
        i += 2;
        let (addr, num, name, data) = (field(file, &mut i)?, field(file, &mut i)?, field(file, &mut i)?, field(file, &mut i)?);
        if name != b"MIT-MAGIC-COOKIE-1" || !(num.is_empty() || num == number.to_string().as_bytes()) {
            continue;
        }
        // 256 = local host, 65535 = any host.
        if family == 65535 || (family == 256 && addr == host.as_bytes()) {
            return Some((name.to_vec(), data.to_vec()));
        }
        fallback.get_or_insert((name.to_vec(), data.to_vec()));
    }
    fallback
}

fn hostname() -> String {
    let mut buf = [0u8; 256];
    let n = unsafe {
        if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) != 0 {
            return String::new();
        }
        buf.iter().position(|&c| c == 0).unwrap_or(buf.len())
    };
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

fn socket(number: u32) -> std::io::Result<UnixStream> {
    let path = format!("/tmp/.X11-unix/X{number}");
    UnixStream::connect(&path).or_else(|e| {
        // Some servers listen only in the abstract namespace.
        use std::os::linux::net::SocketAddrExt;
        let addr = std::os::unix::net::SocketAddr::from_abstract_name(path.as_bytes()).map_err(|_| e)?;
        UnixStream::connect_addr(&addr)
    })
}

/// What the server tells us at the start.
#[derive(Debug, PartialEq)]
struct Setup {
    id_base: u32,
    id_mask: u32,
    max_request: usize,
    root: u32,
    root_size: (u32, u32),
}

/// Parses the connection setup reply (after its 8-byte header). Only the usual
/// little-endian 24-bit colour with 32 bits per pixel is supported (that is what our
/// BGRA pictures are).
fn parse_setup(b: &[u8]) -> Result<Setup, String> {
    let short = || "short X11 setup reply".to_string();
    if b.len() < 32 {
        return Err(short());
    }
    let vendor_len = u16le(b, 16) as usize;
    let max_request = u16le(b, 18) as usize * 4;
    let (screens, formats) = (b[20], b[21] as usize);
    if b[22] != 0 {
        return Err("X server with big-endian images is not supported".into());
    }
    let formats_at = 32 + vendor_len.div_ceil(4) * 4;
    let screen = formats_at + formats * 8;
    if screens == 0 || b.len() < screen + 40 {
        return Err(short());
    }
    let bpp24 = (0..formats).map(|i| &b[formats_at + i * 8..]).find(|f| f[0] == 24).map(|f| f[1]);
    let (root, w, h, visual, depth) = (u32le(b, screen), u16le(b, screen + 20), u16le(b, screen + 22), u32le(b, screen + 32), b[screen + 38]);
    if depth != 24 || bpp24 != Some(32) {
        return Err(format!("X display with {depth}-bit colour ({:?} bits per pixel) is not supported", bpp24));
    }
    // Colour masks of the root visual: blue in the low byte, like BGRA.
    let mut at = screen + 40;
    for _ in 0..b[screen + 39] {
        if at + 8 > b.len() {
            break;
        }
        let visuals = u16le(b, at + 2) as usize;
        for v in 0..visuals {
            let p = at + 8 + v * 24;
            if p + 24 <= b.len() && u32le(b, p) == visual {
                let masks = (u32le(b, p + 8), u32le(b, p + 12), u32le(b, p + 16));
                if masks != (0xFF0000, 0xFF00, 0xFF) {
                    return Err(format!("X display colour layout {masks:x?} is not supported"));
                }
            }
        }
        at += 8 + visuals * 24;
    }
    Ok(Setup { id_base: u32le(b, 4), id_mask: u32le(b, 8), max_request, root, root_size: (w as u32, h as u32) })
}

fn parse_event(code: u8, b: &[u8; 32]) -> Option<Event> {
    let (window, x, y, rx, ry) = (u32le(b, 12), i16le(b, 24), i16le(b, 26), i16le(b, 20), i16le(b, 22));
    // Crossing events caused by grabs (e.g. while a button is held) are not real.
    let normal = b[30] == 0;
    Some(match code {
        4 => Event::Press { window, button: b[1], x, y, root_x: rx, root_y: ry, time: u32le(b, 4) },
        5 => Event::Release { window, button: b[1], x, y, root_x: rx, root_y: ry },
        6 => Event::Motion { window, x, y, root_x: rx, root_y: ry, buttons: u16le(b, 28) },
        7 if normal => Event::Enter { window },
        8 if normal => Event::Leave { window },
        // The last of a series of exposed areas.
        12 if u16le(b, 16) == 0 => Event::Expose { window: u32le(b, 4) },
        _ => return None,
    })
}

/// Reads answers and events until the connection ends.
fn read_loop(mut r: UnixStream, replies: Sender<Reply>, on_event: impl Fn(Event)) {
    let mut errors = 0;
    loop {
        let mut b = [0u8; 32];
        if r.read_exact(&mut b).is_err() {
            on_event(Event::Closed);
            return;
        }
        let seq = u16le(&b, 2);
        match b[0] {
            0 => {
                errors += 1;
                if errors <= 20 {
                    crate::log!("X11 error {} (request {}.{}, value {:#x})", b[1], b[10], u16le(&b, 8), u32le(&b, 4));
                }
                let _ = replies.send(Reply::Err(seq));
            }
            1 => {
                let mut v = b.to_vec();
                v.resize(32 + u32le(&b, 4) as usize * 4, 0);
                if r.read_exact(&mut v[32..]).is_err() {
                    on_event(Event::Closed);
                    return;
                }
                let _ = replies.send(Reply::Ok(seq, v));
            }
            code => {
                let code = code & 0x7F;
                if code == 35 {
                    // Generic event: more data follows.
                    let mut extra = vec![0; u32le(&b, 4) as usize * 4];
                    if r.read_exact(&mut extra).is_err() {
                        on_event(Event::Closed);
                        return;
                    }
                } else if let Some(e) = parse_event(code, &b) {
                    on_event(e);
                }
            }
        }
    }
}

impl X11 {
    /// Connects to the display in DISPLAY; `on_event` is called on the reader thread.
    pub fn connect(on_event: impl Fn(Event) + Send + 'static) -> Result<X11, String> {
        let display = std::env::var("DISPLAY").map_err(|_| "no desktop (DISPLAY is not set)".to_string())?;
        let number = display_number(&display).ok_or_else(|| format!("DISPLAY={display}: only local displays are supported"))?;
        let mut s = socket(number).map_err(|e| format!("X display {display}: {e}"))?;
        let auth_file = std::env::var_os("XAUTHORITY")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".Xauthority")));
        let (name, data) = auth_file.and_then(|f| std::fs::read(f).ok()).and_then(|f| auth_cookie(&f, number, &hostname())).unwrap_or_default();
        let hello = Req(vec![b'l', 0]).u16(11).u16(0).u16(name.len() as u16).u16(data.len() as u16).pad(2).bytes(&name).bytes(&data).0;
        s.write_all(&hello).map_err(|e| e.to_string())?;
        let mut head = [0u8; 8];
        s.read_exact(&mut head).map_err(|e| format!("X display {display}: {e}"))?;
        let mut body = vec![0; u16le(&head, 6) as usize * 4];
        s.read_exact(&mut body).map_err(|e| e.to_string())?;
        let setup = match head[0] {
            1 => parse_setup(&body)?,
            0 => {
                let why = String::from_utf8_lossy(&body[..(head[1] as usize).min(body.len())]).into_owned();
                return Err(format!("X display {display} refused the connection: {}", why.trim()));
            }
            _ => return Err(format!("X display {display} wants another kind of authentication")),
        };
        let (tx, replies) = mpsc::channel();
        let reader = s.try_clone().map_err(|e| e.to_string())?;
        std::thread::Builder::new().name("x11".into()).spawn(move || read_loop(reader, tx, on_event)).map_err(|e| e.to_string())?;
        let mut x = X11 {
            out: s,
            seq: 0,
            id_base: setup.id_base,
            id_mask: setup.id_mask,
            next_id: 0,
            root: setup.root,
            root_size: setup.root_size,
            max_request: setup.max_request,
            gc: 0,
            replies,
            atoms: HashMap::new(),
            broken: false,
        };
        x.gc = x.new_id();
        let gc_req = Req::new(55, 0).u32(x.gc).u32(x.root).u32(0).done();
        x.send(&gc_req);
        crate::log!("X display {display}: {}x{} pixels", setup.root_size.0, setup.root_size.1);
        Ok(x)
    }

    fn send(&mut self, req: &[u8]) -> u16 {
        self.seq = self.seq.wrapping_add(1);
        if let Err(e) = self.out.write_all(req) {
            if !self.broken {
                self.broken = true;
                crate::log!("X11: {e}");
            }
        }
        self.seq
    }

    /// The answer to request `seq` (`None` after an error or 2 s without an answer).
    fn reply(&mut self, seq: u16) -> Option<Vec<u8>> {
        loop {
            match self.replies.recv_timeout(Duration::from_secs(2)) {
                Ok(Reply::Ok(s, v)) if s == seq => return Some(v),
                Ok(Reply::Err(s)) if s == seq => return None,
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    fn new_id(&mut self) -> u32 {
        let shift = self.id_mask.trailing_zeros();
        self.next_id += 1;
        self.id_base | ((self.next_id << shift) & self.id_mask)
    }

    fn atom(&mut self, name: &'static str) -> u32 {
        if let Some(&a) = self.atoms.get(name) {
            return a;
        }
        let s = self.send(&Req::new(16, 0).u16(name.len() as u16).pad(2).bytes(name.as_bytes()).done());
        let a = self.reply(s).map_or(0, |r| u32le(&r, 8));
        self.atoms.insert(name, a);
        a
    }

    /// A property of the root window: (format, value).
    fn root_property(&mut self, property: u32, kind: u32) -> Option<(u8, Vec<u8>)> {
        let s = self.send(&Req::new(20, 0).u32(self.root).u32(property).u32(kind).u32(0).u32(4096).done());
        let r = self.reply(s)?;
        let (format, len) = (r[1], u32le(&r, 16) as usize);
        let bytes = len * (format as usize / 8).max(1);
        (format != 0 && r.len() >= 32 + bytes).then(|| (format, r[32..32 + bytes].to_vec()))
    }

    /// Monitors (RandR 1.5); empty when the server cannot tell.
    pub fn monitors(&mut self) -> Vec<Monitor> {
        let name = b"RANDR";
        let s = self.send(&Req::new(98, 0).u16(name.len() as u16).pad(2).bytes(name).done());
        let Some(r) = self.reply(s) else { return Vec::new() };
        if r[8] == 0 {
            return Vec::new();
        }
        let op = r[9];
        let s = self.send(&Req::new(op, 0).u32(1).u32(5).done());
        let _ = self.reply(s);
        let s = self.send(&Req::new(op, 42).u32(self.root).u8(1).pad(3).done());
        self.reply(s).map(|r| parse_monitors(&r)).unwrap_or_default()
    }

    /// The desktop without panels and docks (`_NET_WORKAREA` of the first desktop).
    pub fn work_area(&mut self) -> Option<(i32, i32, u32, u32)> {
        let a = self.atom("_NET_WORKAREA");
        let (format, v) = self.root_property(a, CARDINAL)?;
        (format == 32 && v.len() >= 16).then(|| (u32le(&v, 0) as i32, u32le(&v, 4) as i32, u32le(&v, 8), u32le(&v, 12)))
    }

    /// Desktop scaling from `Xft.dpi` (192 = 2×); 1.0 when not set.
    pub fn scale(&mut self) -> f32 {
        let dpi = self.root_property(RESOURCE_MANAGER, STRING).and_then(|(_, v)| xft_dpi(&String::from_utf8_lossy(&v)));
        dpi.map_or(1.0, |d| (d / 96.0).clamp(1.0, 4.0))
    }

    /// A borderless window above the others that never takes the keyboard focus
    /// (override-redirect: the window manager leaves it alone).
    pub fn create_window(&mut self, x: i32, y: i32, w: u32, h: u32) -> u32 {
        let id = self.new_id();
        // Exposure, ButtonPress, ButtonRelease, EnterWindow, LeaveWindow, PointerMotion.
        let events: u32 = (1 << 15) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 6);
        let req = Req::new(1, 0)
            .u32(id)
            .u32(self.root)
            .i16(x as i16)
            .i16(y as i16)
            .u16(w.max(1) as u16)
            .u16(h.max(1) as u16)
            .u16(0)
            .u16(1)
            .u32(0)
            // back-pixel, override-redirect, event-mask
            .u32((1 << 1) | (1 << 9) | (1 << 11))
            .u32(0)
            .u32(1)
            .u32(events)
            .done();
        self.send(&req);
        let utf8 = self.atom("UTF8_STRING");
        let (net_name, kind, notification) =
            (self.atom("_NET_WM_NAME"), self.atom("_NET_WM_WINDOW_TYPE"), self.atom("_NET_WM_WINDOW_TYPE_NOTIFICATION"));
        self.set_property(id, WM_NAME, STRING, 8, b"FrigatePiP");
        self.set_property(id, net_name, utf8, 8, b"FrigatePiP");
        self.set_property(id, WM_CLASS, STRING, 8, b"frigate-pip\0FrigatePiP\0");
        self.set_property(id, kind, ATOM, 32, &notification.to_le_bytes());
        self.send(&Req::new(8, 0).u32(id).done());
        id
    }

    fn set_property(&mut self, window: u32, property: u32, kind: u32, format: u8, data: &[u8]) {
        let units = data.len() / (format as usize / 8);
        let req = Req::new(18, 0).u32(window).u32(property).u32(kind).u8(format).pad(3).u32(units as u32).bytes(data).done();
        self.send(&req);
    }

    /// Moves and resizes a window and puts it on top.
    pub fn configure(&mut self, window: u32, x: i32, y: i32, w: u32, h: u32) {
        let req = Req::new(12, 0).u32(window).u16(1 | 2 | 4 | 8 | 64).pad(2).u32(x as u32).u32(y as u32).u32(w.max(1)).u32(h.max(1)).u32(0).done();
        self.send(&req);
    }

    /// Puts a window above the others again.
    pub fn raise(&mut self, window: u32) {
        let req = Req::new(12, 0).u32(window).u16(64).pad(2).u32(0).done();
        self.send(&req);
    }

    /// Moves a window (dragging).
    pub fn move_to(&mut self, window: u32, x: i32, y: i32) {
        let req = Req::new(12, 0).u32(window).u16(1 | 2).pad(2).u32(x as u32).u32(y as u32).done();
        self.send(&req);
    }

    pub fn destroy(&mut self, window: u32) {
        self.send(&Req::new(4, 0).u32(window).done());
    }

    /// Draws a BGRA picture into a window, in strips that fit into one request.
    pub fn put_image(&mut self, window: u32, img: &Image) {
        let row = img.w as usize * 4;
        if row == 0 {
            return;
        }
        let rows_per = ((self.max_request.saturating_sub(24)) / row).max(1);
        let mut y = 0;
        while y < img.h as usize {
            let n = rows_per.min(img.h as usize - y);
            let data = &img.data[y * row..(y + n) * row];
            let mut req = Req::new(72, 2).u32(window).u32(self.gc).u16(img.w as u16).u16(n as u16).i16(0).i16(y as i16).u8(0).u8(24).pad(2);
            req.0.extend_from_slice(data);
            self.send(&req.done());
            y += n;
        }
    }
}

fn parse_monitors(r: &[u8]) -> Vec<Monitor> {
    let mut out = Vec::new();
    if r.len() < 32 {
        return out;
    }
    let mut at = 32;
    for _ in 0..u32le(r, 12) {
        if at + 24 > r.len() {
            break;
        }
        out.push(Monitor {
            x: i16le(r, at + 8),
            y: i16le(r, at + 10),
            w: u16le(r, at + 12) as u32,
            h: u16le(r, at + 14) as u32,
            primary: r[at + 4] != 0,
        });
        at += 24 + 4 * u16le(r, at + 6) as usize;
    }
    out
}

/// `Xft.dpi` from the X resources text.
fn xft_dpi(resources: &str) -> Option<f32> {
    resources.lines().find_map(|l| l.strip_prefix("Xft.dpi:")).and_then(|v| v.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names() {
        assert_eq!(display_number(":0"), Some(0));
        assert_eq!(display_number(":1.0"), Some(1));
        assert_eq!(display_number("unix:2"), Some(2));
        assert_eq!(display_number("localhost:10.0"), None);
        assert_eq!(display_number(""), None);
    }

    #[test]
    fn requests_are_padded_and_sized() {
        // InternAtom "ABC": 8 bytes of header + 3 bytes of name padded to 4.
        let r = Req::new(16, 0).u16(3).pad(2).bytes(b"ABC").done();
        assert_eq!(r, vec![16, 0, 3, 0, 3, 0, 0, 0, b'A', b'B', b'C', 0]);
    }

    #[test]
    fn xauthority_cookie_for_this_display() {
        let entry = |family: u16, addr: &[u8], num: &[u8], data: &[u8]| {
            let mut v = family.to_be_bytes().to_vec();
            for f in [addr, num, b"MIT-MAGIC-COOKIE-1".as_slice(), data] {
                v.extend((f.len() as u16).to_be_bytes());
                v.extend_from_slice(f);
            }
            v
        };
        let mut file = entry(256, b"other", b"0", b"wrong-host");
        file.extend(entry(256, b"box", b"1", b"other-display"));
        file.extend(entry(256, b"box", b"0", b"right"));
        assert_eq!(auth_cookie(&file, 0, "box").map(|c| c.1), Some(b"right".to_vec()));
        // Unknown host: any entry for the display number.
        assert_eq!(auth_cookie(&file, 0, "elsewhere").map(|c| c.1), Some(b"wrong-host".to_vec()));
        assert_eq!(auth_cookie(&file, 7, "box"), None);
    }

    #[test]
    fn setup_reply() {
        let mut b = vec![0u8; 32];
        b[4..8].copy_from_slice(&0x0040_0000u32.to_le_bytes());
        b[8..12].copy_from_slice(&0x001F_FFFFu32.to_le_bytes());
        b[16..18].copy_from_slice(&4u16.to_le_bytes()); // vendor "Test"
        b[18..20].copy_from_slice(&65535u16.to_le_bytes());
        b[20] = 1; // screens
        b[21] = 1; // formats
        b.extend(b"Test");
        b.extend([24, 32, 32, 0, 0, 0, 0, 0]);
        let mut screen = vec![0u8; 40];
        screen[0..4].copy_from_slice(&0x1234u32.to_le_bytes());
        screen[20..22].copy_from_slice(&1920u16.to_le_bytes());
        screen[22..24].copy_from_slice(&1080u16.to_le_bytes());
        screen[32..36].copy_from_slice(&0x21u32.to_le_bytes());
        screen[38] = 24;
        screen[39] = 1;
        b.extend(screen);
        b.extend([24, 0, 1, 0, 0, 0, 0, 0]);
        let mut visual = vec![0u8; 24];
        visual[0..4].copy_from_slice(&0x21u32.to_le_bytes());
        visual[8..12].copy_from_slice(&0xFF0000u32.to_le_bytes());
        visual[12..16].copy_from_slice(&0xFF00u32.to_le_bytes());
        visual[16..20].copy_from_slice(&0xFFu32.to_le_bytes());
        b.extend(visual);
        let s = parse_setup(&b).unwrap();
        assert_eq!((s.root, s.root_size, s.max_request, s.id_base), (0x1234, (1920, 1080), 262140, 0x0040_0000));
        b[22] = 1;
        assert!(parse_setup(&b).is_err());
    }

    #[test]
    fn randr_monitors_and_dpi() {
        let mut r = vec![0u8; 32];
        r[12..16].copy_from_slice(&2u32.to_le_bytes());
        let mon = |primary: u8, outputs: u16, x: i16, w: u16| {
            let mut m = vec![0u8; 24];
            m[4] = primary;
            m[6..8].copy_from_slice(&outputs.to_le_bytes());
            m[8..10].copy_from_slice(&x.to_le_bytes());
            m[12..14].copy_from_slice(&w.to_le_bytes());
            m[14..16].copy_from_slice(&1080u16.to_le_bytes());
            m.extend(vec![0u8; 4 * outputs as usize]);
            m
        };
        r.extend(mon(0, 1, -1920, 1920));
        r.extend(mon(1, 2, 0, 2560));
        let m = parse_monitors(&r);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].x, m[0].w, m[0].primary), (-1920, 1920, false));
        assert_eq!((m[1].x, m[1].w, m[1].primary), (0, 2560, true));
        assert_eq!(xft_dpi("Xft.antialias:\t1\nXft.dpi:\t192\n"), Some(192.0));
        assert_eq!(xft_dpi("Xcursor.size: 24"), None);
    }
}
