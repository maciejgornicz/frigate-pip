//! Small utilities: base64, MD5, cancelling network operations, logging.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------- base64

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- MD5 (RFC 1321)

pub fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16,
        23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    for block in msg.chunks(64) {
        let m: Vec<u32> = block.chunks(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let (mut a, mut b, mut c, mut d) = (h[0], h[1], h[2], h[3]);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

pub fn md5_hex(data: &[u8]) -> String {
    md5(data).iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------- cancellation

/// Cancellation flag shared between threads. Registered sockets are
/// closed on `cancel()`, which immediately interrupts blocking reads.
#[derive(Clone, Default)]
pub struct Cancel(Arc<CancelInner>);

#[derive(Default)]
struct CancelInner {
    flag: AtomicBool,
    socks: Mutex<Vec<TcpStream>>,
}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_set(&self) -> bool {
        self.0.flag.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.0.flag.store(true, Ordering::Relaxed);
        for s in self.0.socks.lock().unwrap().drain(..) {
            let _ = s.shutdown(Shutdown::Both);
        }
    }

    pub fn register(&self, s: &TcpStream) {
        let Ok(c) = s.try_clone() else { return };
        let mut socks = self.0.socks.lock().unwrap();
        if self.is_set() {
            let _ = c.shutdown(Shutdown::Both);
            return;
        }
        if socks.len() >= 8 {
            socks.remove(0);
        }
        socks.push(c);
    }

    /// Waits `ms` milliseconds or until cancelled. Returns `true` if cancelled.
    pub fn sleep(&self, ms: u64) -> bool {
        let mut left = ms;
        while left > 0 && !self.is_set() {
            let step = left.min(100);
            std::thread::sleep(std::time::Duration::from_millis(step));
            left -= step;
        }
        self.is_set()
    }
}

// ---------------------------------------------------------------- log

static LOG: Mutex<Option<File>> = Mutex::new(None);
static LOG_STDERR: AtomicBool = AtomicBool::new(false);

/// Folder with the settings and the log: `%APPDATA%\\FrigatePiP` on Windows,
/// `$XDG_CONFIG_HOME/frigate-pip` (usually `~/.config/frigate-pip`) on Linux.
pub fn app_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("FrigatePiP")
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("frigate-pip")
    }
}

pub fn log_path() -> PathBuf {
    app_dir().join("frigate-pip.log")
}

pub fn init_log(to_stderr: bool) {
    let path = log_path();
    let _ = std::fs::create_dir_all(app_dir());
    if std::fs::metadata(&path).map(|m| m.len() > 512 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("old.log"));
    }
    *LOG.lock().unwrap() = OpenOptions::new().create(true).append(true).open(path).ok();
    LOG_STDERR.store(to_stderr, Ordering::Relaxed);
}

pub fn log_line(msg: &str) {
    let line = format!("{} {}\r\n", local_time(true), msg);
    if LOG_STDERR.load(Ordering::Relaxed) {
        eprint!("{line}");
    }
    if let Some(f) = LOG.lock().unwrap().as_mut() {
        let _ = f.write_all(line.as_bytes());
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::util::log_line(&format!($($t)*)) };
}

/// Local time as `HH:MM:SS` (with milliseconds if `ms`).
pub fn local_time(ms: bool) -> String {
    #[cfg(windows)]
    let (h, m, s, millis) = {
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        (t.wHour as i32, t.wMinute as i32, t.wSecond as i32, t.wMilliseconds as u32)
    };
    #[cfg(not(windows))]
    let (h, m, s, millis) = {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        #[allow(deprecated)] // musl: libc::time_t is going to become 64-bit
        let secs = now.as_secs() as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&secs, &mut tm) };
        (tm.tm_hour, tm.tm_min, tm.tm_sec, now.subsec_millis())
    };
    if ms {
        format!("{h:02}:{m:02}:{s:02}.{millis:03}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

/// Returns unused working-set memory to the OS (after preview windows close).
#[cfg(windows)]
pub fn trim_memory() {
    use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// Simple pseudo-random generator (WebSocket masks, cnonce, client id).
pub fn random_u64() -> u64 {
    use std::sync::atomic::AtomicU64;
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut x = STATE.load(Ordering::Relaxed);
    if x == 0 {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        x = t.as_nanos() as u64 ^ (std::process::id() as u64) << 32 | 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    STATE.store(x, Ordering::Relaxed);
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_rfc4648_vectors() {
        let cases = [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")];
        for (plain, enc) in cases {
            assert_eq!(base64_encode(plain.as_bytes()), enc);
            assert_eq!(base64_decode(enc).unwrap(), plain.as_bytes());
        }
        assert!(base64_decode("Zm9v!").is_none());
    }

    #[test]
    fn md5_rfc1321_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5_hex(b"message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
        assert_eq!(md5_hex(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"), "57edf4a22be3c955ac49da2e2107b67a");
    }

    #[test]
    fn cancel_sleep_returns_early() {
        let c = Cancel::new();
        let c2 = c.clone();
        let t = std::thread::spawn(move || c2.sleep(5000));
        std::thread::sleep(std::time::Duration::from_millis(50));
        c.cancel();
        assert!(t.join().unwrap());
    }
}
