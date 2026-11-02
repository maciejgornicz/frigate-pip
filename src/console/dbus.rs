//! A minimal D-Bus client for the session bus, without libraries: enough for the tray icon
//! (StatusNotifierItem with its menu) and desktop notifications. Little-endian messages only
//! (every Linux machine FrigatePiP runs on).
//!
//! A thread reads the bus: answers go to the waiting `call`, method calls and signals to the
//! handler given to `Bus::session`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A D-Bus value. Arrays carry their element signature, so empty ones can be sent too.
#[derive(Clone, Debug, PartialEq)]
pub enum V {
    Y(u8),
    B(bool),
    N(i16),
    Q(u16),
    I(i32),
    U(u32),
    X(i64),
    T(u64),
    D(f64),
    S(String),
    O(String),
    G(String),
    A(String, Vec<V>),
    R(Vec<V>),
    E(Box<V>, Box<V>),
    Var(Box<V>),
}

impl V {
    pub fn s(v: &str) -> V {
        V::S(v.to_string())
    }

    pub fn var(v: V) -> V {
        V::Var(Box::new(v))
    }

    /// `a{sv}` from (key, value) pairs.
    pub fn dict(pairs: Vec<(&str, V)>) -> V {
        V::A("{sv}".into(), pairs.into_iter().map(|(k, v)| V::E(Box::new(V::s(k)), Box::new(V::var(v)))).collect())
    }

    pub fn signature(&self) -> String {
        match self {
            V::Y(_) => "y".into(),
            V::B(_) => "b".into(),
            V::N(_) => "n".into(),
            V::Q(_) => "q".into(),
            V::I(_) => "i".into(),
            V::U(_) => "u".into(),
            V::X(_) => "x".into(),
            V::T(_) => "t".into(),
            V::D(_) => "d".into(),
            V::S(_) => "s".into(),
            V::O(_) => "o".into(),
            V::G(_) => "g".into(),
            V::A(e, _) => format!("a{e}"),
            V::R(items) => format!("({})", items.iter().map(V::signature).collect::<String>()),
            V::E(k, v) => format!("{{{}{}}}", k.signature(), v.signature()),
            V::Var(_) => "v".into(),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            V::S(s) | V::O(s) | V::G(s) => Some(s),
            V::Var(v) => v.as_str(),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            V::Y(v) => Some(v as i64),
            V::N(v) => Some(v as i64),
            V::Q(v) => Some(v as i64),
            V::I(v) => Some(v as i64),
            V::U(v) => Some(v as i64),
            V::X(v) => Some(v),
            V::Var(ref v) => v.as_i64(),
            _ => None,
        }
    }
}

/// Length of the first complete type in a signature.
fn type_len(s: &[u8]) -> Option<usize> {
    match *s.first()? {
        b'a' => Some(1 + type_len(&s[1..])?),
        open @ (b'(' | b'{') => {
            let close = if open == b'(' { b')' } else { b'}' };
            let mut i = 1;
            while *s.get(i)? != close {
                i += type_len(&s[i..])?;
            }
            Some(i + 1)
        }
        _ => Some(1),
    }
}

fn align_of(c: u8) -> usize {
    match c {
        b'y' | b'g' | b'v' => 1,
        b'n' | b'q' => 2,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 4,
    }
}

/// Writes values (offsets count from the start of the buffer, which starts 8-aligned).
#[derive(Default)]
struct Enc {
    b: Vec<u8>,
}

impl Enc {
    fn align(&mut self, n: usize) {
        while !self.b.len().is_multiple_of(n) {
            self.b.push(0);
        }
    }

    fn u32(&mut self, v: u32) {
        self.align(4);
        self.b.extend(v.to_le_bytes());
    }

    fn put(&mut self, v: &V) {
        match v {
            V::Y(x) => self.b.push(*x),
            V::B(x) => self.u32(*x as u32),
            V::N(x) => {
                self.align(2);
                self.b.extend(x.to_le_bytes());
            }
            V::Q(x) => {
                self.align(2);
                self.b.extend(x.to_le_bytes());
            }
            V::I(x) => self.u32(*x as u32),
            V::U(x) => self.u32(*x),
            V::X(x) => {
                self.align(8);
                self.b.extend(x.to_le_bytes());
            }
            V::T(x) => {
                self.align(8);
                self.b.extend(x.to_le_bytes());
            }
            V::D(x) => {
                self.align(8);
                self.b.extend(x.to_le_bytes());
            }
            V::S(s) | V::O(s) => {
                self.u32(s.len() as u32);
                self.b.extend(s.as_bytes());
                self.b.push(0);
            }
            V::G(s) => {
                self.b.push(s.len() as u8);
                self.b.extend(s.as_bytes());
                self.b.push(0);
            }
            V::A(elem, items) => {
                self.u32(0);
                let len_at = self.b.len() - 4;
                self.align(align_of(elem.as_bytes().first().copied().unwrap_or(b'y')));
                let start = self.b.len();
                for item in items {
                    self.put(item);
                }
                let len = (self.b.len() - start) as u32;
                self.b[len_at..len_at + 4].copy_from_slice(&len.to_le_bytes());
            }
            V::R(items) => {
                self.align(8);
                for item in items {
                    self.put(item);
                }
            }
            V::E(k, v) => {
                self.align(8);
                self.put(k);
                self.put(v);
            }
            V::Var(inner) => {
                self.put(&V::G(inner.signature()));
                self.put(inner);
            }
        }
    }
}

/// Reads values of a known signature.
struct Dec<'a> {
    b: &'a [u8],
    p: usize,
}

impl Dec<'_> {
    fn align(&mut self, n: usize) {
        self.p = self.p.div_ceil(n) * n;
    }

    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let v = self.b.get(self.p..self.p + n)?;
        self.p += n;
        Some(v)
    }

    fn u32(&mut self) -> Option<u32> {
        self.align(4);
        self.take(4).map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }

    fn u64(&mut self) -> Option<u64> {
        self.align(8);
        let v = self.take(8)?;
        Some(u64::from_le_bytes(v.try_into().ok()?))
    }

    fn u16(&mut self) -> Option<u16> {
        self.align(2);
        self.take(2).map(|v| u16::from_le_bytes([v[0], v[1]]))
    }

    fn string(&mut self, len: usize) -> Option<String> {
        let s = String::from_utf8_lossy(self.take(len)?).into_owned();
        self.take(1)?;
        Some(s)
    }

    /// One value of the complete type `sig`.
    fn read(&mut self, sig: &[u8]) -> Option<V> {
        Some(match sig[0] {
            b'y' => V::Y(self.take(1)?[0]),
            b'b' => V::B(self.u32()? != 0),
            b'n' => V::N(self.u16()? as i16),
            b'q' => V::Q(self.u16()?),
            b'i' => V::I(self.u32()? as i32),
            b'u' | b'h' => V::U(self.u32()?),
            b'x' => V::X(self.u64()? as i64),
            b't' => V::T(self.u64()?),
            b'd' => V::D(f64::from_bits(self.u64()?)),
            b's' | b'o' => {
                let len = self.u32()? as usize;
                let s = self.string(len)?;
                if sig[0] == b's' {
                    V::S(s)
                } else {
                    V::O(s)
                }
            }
            b'g' => {
                let len = self.take(1)?[0] as usize;
                V::G(self.string(len)?)
            }
            b'v' => {
                let len = self.take(1)?[0] as usize;
                let inner = self.string(len)?;
                let s = inner.as_bytes();
                if s.is_empty() || type_len(s)? != s.len() {
                    return None;
                }
                V::Var(Box::new(self.read(s)?))
            }
            b'a' => {
                let len = self.u32()? as usize;
                let elem = &sig[1..1 + type_len(&sig[1..])?];
                self.align(align_of(elem[0]));
                let end = self.p + len;
                let mut items = Vec::new();
                while self.p < end {
                    items.push(self.read(elem)?);
                }
                V::A(String::from_utf8_lossy(elem).into_owned(), items)
            }
            b'(' => {
                self.align(8);
                let mut items = Vec::new();
                let mut i = 1;
                while sig[i] != b')' {
                    let n = type_len(&sig[i..])?;
                    items.push(self.read(&sig[i..i + n])?);
                    i += n;
                }
                V::R(items)
            }
            b'{' => {
                self.align(8);
                let k = type_len(&sig[1..])?;
                let key = self.read(&sig[1..1 + k])?;
                let n = type_len(&sig[1 + k..])?;
                let value = self.read(&sig[1 + k..1 + k + n])?;
                V::E(Box::new(key), Box::new(value))
            }
            _ => return None,
        })
    }

    /// All values of a signature with several complete types.
    fn read_all(&mut self, sig: &[u8]) -> Option<Vec<V>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < sig.len() {
            let n = type_len(&sig[i..])?;
            out.push(self.read(&sig[i..i + n])?);
            i += n;
        }
        Some(out)
    }
}

pub const CALL: u8 = 1;
pub const RETURN: u8 = 2;
pub const ERROR: u8 = 3;
pub const SIGNAL: u8 = 4;
/// Message flag: no answer is wanted.
const NO_REPLY: u8 = 1;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Msg {
    pub kind: u8,
    pub flags: u8,
    pub serial: u32,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error: Option<String>,
    pub reply_to: Option<u32>,
    pub dest: Option<String>,
    pub sender: Option<String>,
    pub body: Vec<V>,
}

impl Msg {
    fn encode(&self, serial: u32) -> Vec<u8> {
        let mut body = Enc::default();
        for v in &self.body {
            body.put(v);
        }
        let sig: String = self.body.iter().map(V::signature).collect();
        let mut fields = Vec::new();
        let mut field = |code: u8, v: V| fields.push(V::R(vec![V::Y(code), V::var(v)]));
        if let Some(p) = &self.path {
            field(1, V::O(p.clone()));
        }
        if let Some(i) = &self.interface {
            field(2, V::s(i));
        }
        if let Some(m) = &self.member {
            field(3, V::s(m));
        }
        if let Some(e) = &self.error {
            field(4, V::s(e));
        }
        if let Some(r) = self.reply_to {
            field(5, V::U(r));
        }
        if let Some(d) = &self.dest {
            field(6, V::s(d));
        }
        if !sig.is_empty() {
            field(8, V::G(sig));
        }
        let mut h = Enc { b: vec![b'l', self.kind, self.flags, 1] };
        h.b.extend((body.b.len() as u32).to_le_bytes());
        h.b.extend(serial.to_le_bytes());
        h.put(&V::A("(yv)".into(), fields));
        h.align(8);
        h.b.extend(body.b);
        h.b
    }

    /// Parses a whole message (`b` = header and body).
    fn decode(b: &[u8]) -> Option<Msg> {
        if b.len() < 16 || b[0] != b'l' {
            return None;
        }
        let mut d = Dec { b, p: 12 };
        let V::A(_, fields) = d.read(b"a(yv)")? else { return None };
        d.align(8);
        let body_start = d.p;
        let mut m = Msg { kind: b[1], flags: b[2], serial: u32::from_le_bytes([b[8], b[9], b[10], b[11]]), ..Default::default() };
        let mut sig = String::new();
        for f in fields {
            let V::R(f) = f else { continue };
            let (Some(V::Y(code)), Some(V::Var(v))) = (f.first(), f.get(1)) else { continue };
            let text = v.as_str().map(str::to_string);
            match code {
                1 => m.path = text,
                2 => m.interface = text,
                3 => m.member = text,
                4 => m.error = text,
                5 => m.reply_to = v.as_i64().map(|n| n as u32),
                6 => m.dest = text,
                7 => m.sender = text,
                8 => sig = text.unwrap_or_default(),
                _ => {}
            }
        }
        m.body = Dec { b: &b[body_start..], p: 0 }.read_all(sig.as_bytes())?;
        Some(m)
    }

    pub fn is(&self, interface: &str, member: &str) -> bool {
        self.interface.as_deref() == Some(interface) && self.member.as_deref() == Some(member)
    }
}

/// Reads one message from the stream.
fn read_msg(r: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut head = [0u8; 16];
    r.read_exact(&mut head)?;
    let body = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
    let fields = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    let total = (16 + fields).div_ceil(8) * 8 + body;
    if total > 64 << 20 {
        return Err(std::io::Error::other("D-Bus message too large"));
    }
    let mut b = head.to_vec();
    b.resize(total, 0);
    r.read_exact(&mut b[16..])?;
    Ok(b)
}

/// The session bus socket from DBUS_SESSION_BUS_ADDRESS (unix:path= or unix:abstract=).
fn connect_address(address: &str) -> std::io::Result<UnixStream> {
    for part in address.split(';') {
        let Some(params) = part.strip_prefix("unix:") else { continue };
        for kv in params.split(',') {
            if let Some(path) = kv.strip_prefix("path=") {
                return UnixStream::connect(crate::net::percent_decode(path));
            }
            if let Some(name) = kv.strip_prefix("abstract=") {
                use std::os::linux::net::SocketAddrExt;
                let addr = std::os::unix::net::SocketAddr::from_abstract_name(crate::net::percent_decode(name).as_bytes())?;
                return UnixStream::connect_addr(&addr);
            }
        }
    }
    Err(std::io::Error::other(format!("unsupported D-Bus address {address}")))
}

pub struct Bus {
    w: Mutex<UnixStream>,
    serial: AtomicU32,
    pending: Mutex<HashMap<u32, Sender<Msg>>>,
    /// Our unique name on the bus (":1.42").
    pub name: String,
}

impl Bus {
    /// Connects to the session bus. `handler` gets method calls and signals (on the reader
    /// thread; it may send answers, but must not wait for answers itself).
    pub fn session(handler: impl Fn(&Bus, Msg) + Send + 'static) -> Result<Arc<Bus>, String> {
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS")
            .ok()
            .or_else(|| std::env::var("XDG_RUNTIME_DIR").ok().map(|d| format!("unix:path={}/bus", d.trim_end_matches('/'))))
            .ok_or("no session bus (DBUS_SESSION_BUS_ADDRESS is not set)")?;
        let mut s = connect_address(&address).map_err(|e| format!("D-Bus {address}: {e}"))?;
        s.set_read_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
        // SASL: EXTERNAL with our user id (as a decimal string, hex-encoded).
        let uid = unsafe { libc::getuid() }.to_string();
        let hex: String = uid.bytes().map(|c| format!("{c:02x}")).collect();
        s.write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes()).map_err(|e| e.to_string())?;
        let mut line = Vec::new();
        let mut byte = [0u8];
        while !line.ends_with(b"\r\n") {
            s.read_exact(&mut byte).map_err(|e| format!("D-Bus authentication: {e}"))?;
            line.push(byte[0]);
        }
        if !line.starts_with(b"OK ") {
            return Err(format!("D-Bus authentication refused: {}", String::from_utf8_lossy(&line).trim()));
        }
        s.write_all(b"BEGIN\r\n").map_err(|e| e.to_string())?;
        let hello = Msg {
            kind: CALL,
            path: Some("/org/freedesktop/DBus".into()),
            interface: Some("org.freedesktop.DBus".into()),
            member: Some("Hello".into()),
            dest: Some("org.freedesktop.DBus".into()),
            ..Default::default()
        };
        s.write_all(&hello.encode(1)).map_err(|e| e.to_string())?;
        let name = loop {
            let m = Msg::decode(&read_msg(&mut s).map_err(|e| format!("D-Bus: {e}"))?).ok_or("bad D-Bus message")?;
            if m.reply_to == Some(1) {
                break m.body.first().and_then(V::as_str).unwrap_or_default().to_string();
            }
        };
        s.set_read_timeout(None).map_err(|e| e.to_string())?;
        let reader = s.try_clone().map_err(|e| e.to_string())?;
        let bus = Arc::new(Bus { w: Mutex::new(s), serial: AtomicU32::new(1), pending: Mutex::new(HashMap::new()), name });
        let weak = Arc::downgrade(&bus);
        std::thread::Builder::new()
            .name("dbus".into())
            .spawn(move || {
                let mut r = reader;
                while let Ok(raw) = read_msg(&mut r) {
                    let Some(bus) = weak.upgrade() else { return };
                    let Some(m) = Msg::decode(&raw) else { continue };
                    match m.kind {
                        RETURN | ERROR => {
                            if let Some(tx) = m.reply_to.and_then(|s| bus.pending.lock().unwrap().remove(&s)) {
                                let _ = tx.send(m);
                            }
                        }
                        _ => handler(&bus, m),
                    }
                }
                crate::log!("D-Bus connection closed");
            })
            .map_err(|e| e.to_string())?;
        Ok(bus)
    }

    fn send(&self, m: &Msg) -> u32 {
        let serial = self.serial.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.w.lock().unwrap().write_all(&m.encode(serial));
        serial
    }

    /// Calls a method and waits for the answer (2 s at most).
    pub fn call(&self, dest: &str, path: &str, interface: &str, member: &str, body: Vec<V>) -> Result<Vec<V>, String> {
        let (tx, rx) = mpsc::channel();
        let m = Msg {
            kind: CALL,
            path: Some(path.into()),
            interface: Some(interface.into()),
            member: Some(member.into()),
            dest: Some(dest.into()),
            body,
            ..Default::default()
        };
        // The serial is known only when sending: reserve it first.
        let serial = self.serial.fetch_add(1, Ordering::Relaxed) + 1;
        self.pending.lock().unwrap().insert(serial, tx);
        let _ = self.w.lock().unwrap().write_all(&m.encode(serial));
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(r) if r.kind == ERROR => Err(format!("{}: {}", r.error.unwrap_or_default(), r.body.first().and_then(V::as_str).unwrap_or(""))),
            Ok(r) => Ok(r.body),
            Err(_) => {
                self.pending.lock().unwrap().remove(&serial);
                Err(format!("{dest}.{member}: no answer"))
            }
        }
    }

    /// Calls a method without waiting (also from the reader thread).
    pub fn call_nowait(&self, dest: &str, path: &str, interface: &str, member: &str, body: Vec<V>) {
        self.send(&Msg {
            kind: CALL,
            flags: NO_REPLY,
            path: Some(path.into()),
            interface: Some(interface.into()),
            member: Some(member.into()),
            dest: Some(dest.into()),
            body,
            ..Default::default()
        });
    }

    pub fn reply(&self, to: &Msg, body: Vec<V>) {
        if to.flags & NO_REPLY == 0 {
            self.send(&Msg { kind: RETURN, reply_to: Some(to.serial), dest: to.sender.clone(), body, ..Default::default() });
        }
    }

    pub fn error(&self, to: &Msg, name: &str, text: &str) {
        if to.flags & NO_REPLY == 0 {
            self.send(&Msg {
                kind: ERROR,
                error: Some(name.into()),
                reply_to: Some(to.serial),
                dest: to.sender.clone(),
                body: vec![V::s(text)],
                ..Default::default()
            });
        }
    }

    pub fn signal(&self, path: &str, interface: &str, member: &str, body: Vec<V>) {
        self.send(&Msg {
            kind: SIGNAL,
            path: Some(path.into()),
            interface: Some(interface.into()),
            member: Some(member.into()),
            body,
            ..Default::default()
        });
    }

    /// A call to the bus itself (org.freedesktop.DBus).
    pub fn bus_call(&self, member: &str, body: Vec<V>) -> Result<Vec<V>, String> {
        self.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", member, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_and_lengths() {
        assert_eq!(type_len(b"a{sv}i"), Some(5));
        assert_eq!(type_len(b"(ia{sv}av)"), Some(10));
        assert_eq!(V::dict(vec![("label", V::s("x"))]).signature(), "a{sv}");
        assert_eq!(V::R(vec![V::I(1), V::A("v".into(), vec![])]).signature(), "(iav)");
    }

    #[test]
    fn values_survive_a_round_trip() {
        let values = vec![
            V::Y(7),
            V::B(true),
            V::I(-5),
            V::s("tekst ąę"),
            V::O("/MenuBar".into()),
            V::dict(vec![("label", V::s("Ustawienia")), ("enabled", V::B(false))]),
            V::A("v".into(), vec![V::var(V::R(vec![V::I(3), V::dict(vec![]), V::A("v".into(), vec![])]))]),
            V::A("(iiay)".into(), vec![V::R(vec![V::I(1), V::I(1), V::A("y".into(), vec![V::Y(255), V::Y(0), V::Y(1), V::Y(2)])])]),
            V::X(-1),
            V::D(1.5),
        ];
        let mut e = Enc::default();
        for v in &values {
            e.put(v);
        }
        let sig: String = values.iter().map(V::signature).collect();
        assert_eq!(Dec { b: &e.b, p: 0 }.read_all(sig.as_bytes()), Some(values));
    }

    #[test]
    fn hello_message_bytes() {
        let hello = Msg {
            kind: CALL,
            path: Some("/org/freedesktop/DBus".into()),
            interface: Some("org.freedesktop.DBus".into()),
            member: Some("Hello".into()),
            dest: Some("org.freedesktop.DBus".into()),
            ..Default::default()
        };
        let b = hello.encode(1);
        // Fixed header: little endian, method call, no flags, version 1, empty body, serial 1.
        assert_eq!(&b[..12], &[b'l', 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(b.len() % 8, 0);
        let back = Msg::decode(&b).unwrap();
        assert_eq!(back.member.as_deref(), Some("Hello"));
        assert_eq!(back.dest.as_deref(), Some("org.freedesktop.DBus"));
        assert_eq!(back.serial, 1);
    }

    #[test]
    fn method_call_with_body() {
        let m = Msg {
            kind: CALL,
            path: Some("/MenuBar".into()),
            interface: Some("com.canonical.dbusmenu".into()),
            member: Some("Event".into()),
            body: vec![V::I(3), V::s("clicked"), V::var(V::I(0)), V::U(0)],
            ..Default::default()
        };
        let back = Msg::decode(&m.encode(9)).unwrap();
        assert!(back.is("com.canonical.dbusmenu", "Event"));
        assert_eq!(back.body, m.body);
        assert_eq!(read_msg(&mut &m.encode(9)[..]).unwrap().len(), m.encode(9).len());
    }

    #[test]
    fn address_escapes() {
        assert_eq!(crate::net::percent_decode("/run/user/1000/bus"), "/run/user/1000/bus");
        assert_eq!(crate::net::percent_decode("/tmp/dbus%2dx"), "/tmp/dbus-x");
    }
}
