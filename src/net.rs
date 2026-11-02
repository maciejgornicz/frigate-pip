//! URLs, cancellable TCP connections and a minimal HTTP/1.1 client.

use crate::util::Cancel;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct Url {
    pub scheme: String,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub host: String,
    pub port: u16,
    /// Path with query string; always starts with `/`.
    pub path: String,
}

/// `%xx` escapes (URLs, D-Bus addresses, form fields).
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Url {
    pub fn parse(s: &str) -> Option<Url> {
        let (scheme, rest) = s.trim().split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        let (authority, path) = match rest.find(['/', '?']) {
            Some(i) if rest.as_bytes()[i] == b'/' => (&rest[..i], rest[i..].to_string()),
            Some(i) => (&rest[..i], format!("/{}", &rest[i..])),
            None => (rest, "/".to_string()),
        };
        let (userinfo, hostport) = match authority.rfind('@') {
            Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
            None => (None, authority),
        };
        let (user, pass) = match userinfo {
            Some(u) => match u.split_once(':') {
                Some((a, b)) => (Some(percent_decode(a)), Some(percent_decode(b))),
                None => (Some(percent_decode(u)), None),
            },
            None => (None, None),
        };
        let default_port = match scheme.as_str() {
            "http" | "ws" => 80,
            "https" | "wss" => 443,
            "rtsp" => 554,
            "mqtt" => 1883,
            _ => 0,
        };
        let (host, port) = if let Some(h) = hostport.strip_prefix('[') {
            let (h, rest) = h.split_once(']')?;
            let port = rest.strip_prefix(':').map(|p| p.parse().ok()).unwrap_or(Some(default_port))?;
            (h.to_string(), port)
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), p.parse().ok()?),
                None => (hostport.to_string(), default_port),
            }
        };
        if host.is_empty() || port == 0 {
            return None;
        }
        Some(Url { scheme, user, pass, host, port, path })
    }

    pub fn host_header(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// Address without credentials.
    pub fn without_credentials(&self) -> String {
        format!("{}://{}{}", self.scheme, self.host_header(), self.path)
    }
}

/// Connects with a timeout; the socket is registered with `cancel`.
pub fn connect(host: &str, port: u16, timeout: Duration, cancel: Option<&Cancel>) -> io::Result<TcpStream> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, format!("cannot resolve {host}"));
    for addr in (host, port).to_socket_addrs()? {
        if cancel.is_some_and(|c| c.is_set()) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                if let Some(c) = cancel {
                    c.register(&s);
                }
                return Ok(s);
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Reads an LF-terminated line (without CR/LF), with a length limit.
pub fn read_line<R: BufRead>(r: &mut R) -> io::Result<String> {
    let mut buf = Vec::new();
    loop {
        let avail = r.fill_buf()?;
        if avail.is_empty() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed"));
        }
        match avail.iter().position(|&c| c == b'\n') {
            Some(i) => {
                buf.extend_from_slice(&avail[..i]);
                r.consume(i + 1);
                break;
            }
            None => {
                let n = avail.len();
                buf.extend_from_slice(avail);
                r.consume(n);
            }
        }
        if buf.len() > 64 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
        }
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Reads the status line and headers (HTTP/RTSP).
pub fn read_head<R: BufRead>(r: &mut R) -> io::Result<(String, Vec<(String, String)>)> {
    let status = read_line(r)?;
    let mut headers = Vec::new();
    loop {
        let l = read_line(r)?;
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
        if headers.len() > 200 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "too many headers"));
        }
    }
    Ok((status, headers))
}

pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

pub fn status_code(status_line: &str) -> u16 {
    status_line.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0)
}

enum BodyKind {
    Len(u64),
    Chunked { left: u64, need_crlf: bool, done: bool },
    Close,
}

/// HTTP response body (Content-Length, chunked, or until close).
pub struct Body<R> {
    r: R,
    kind: BodyKind,
}

impl<R: BufRead> Body<R> {
    pub fn new(r: R, headers: &[(String, String)]) -> Self {
        let kind = if header(headers, "Transfer-Encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
            BodyKind::Chunked { left: 0, need_crlf: false, done: false }
        } else if let Some(n) = header(headers, "Content-Length").and_then(|v| v.parse().ok()) {
            BodyKind::Len(n)
        } else {
            BodyKind::Close
        };
        Body { r, kind }
    }
}

impl<R: BufRead> Read for Body<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        match &mut self.kind {
            BodyKind::Close => self.r.read(buf),
            BodyKind::Len(left) => {
                if *left == 0 {
                    return Ok(0);
                }
                let max = (*left).min(buf.len() as u64) as usize;
                let n = self.r.read(&mut buf[..max])?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated response"));
                }
                *left -= n as u64;
                Ok(n)
            }
            BodyKind::Chunked { left, need_crlf, done } => {
                if *done {
                    return Ok(0);
                }
                if *left == 0 {
                    if *need_crlf {
                        read_line(&mut self.r)?;
                        *need_crlf = false;
                    }
                    let line = read_line(&mut self.r)?;
                    let size_str = line.split(';').next().unwrap_or("").trim();
                    let size = u64::from_str_radix(size_str, 16).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid chunk size"))?;
                    if size == 0 {
                        while !read_line(&mut self.r)?.is_empty() {}
                        *done = true;
                        return Ok(0);
                    }
                    *left = size;
                }
                let max = (*left).min(buf.len() as u64) as usize;
                let n = self.r.read(&mut buf[..max])?;
                if n == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated chunk"));
                }
                *left -= n as u64;
                if *left == 0 {
                    *need_crlf = true;
                }
                Ok(n)
            }
        }
    }
}

pub struct Response {
    pub status: u16,
    pub body: Body<BufReader<TcpStream>>,
}

impl Response {
    pub fn read_all(mut self, limit: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut chunk = [0u8; 16384];
        loop {
            let n = self.body.read(&mut chunk)?;
            if n == 0 {
                return Ok(out);
            }
            out.extend_from_slice(&chunk[..n]);
            if out.len() > limit {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "response too large"));
            }
        }
    }
}

/// GET request. `timeout` applies to the connection and to each read.
pub fn get(url: &str, timeout: Duration, cancel: Option<&Cancel>) -> io::Result<Response> {
    let u = Url::parse(url).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid address {url}")))?;
    if u.scheme != "http" {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "only http:// is supported"));
    }
    let mut s = connect(&u.host, u.port, timeout, cancel)?;
    s.set_read_timeout(Some(timeout))?;
    let mut req = format!("GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: FrigatePiP\r\nAccept: */*\r\nConnection: close\r\n", u.path, u.host_header());
    if let (Some(user), Some(pass)) = (&u.user, &u.pass) {
        req.push_str(&format!("Authorization: Basic {}\r\n", crate::util::base64_encode(format!("{user}:{pass}").as_bytes())));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())?;
    let mut r = BufReader::with_capacity(64 * 1024, s);
    let (status, headers) = read_head(&mut r)?;
    let body = Body::new(r, &headers);
    Ok(Response { status: status_code(&status), body })
}

/// GET of the whole body; error if the status ≠ 200.
pub fn get_bytes(url: &str, timeout: Duration, cancel: Option<&Cancel>, limit: usize) -> io::Result<Vec<u8>> {
    let resp = get(url, timeout, cancel)?;
    if resp.status != 200 {
        return Err(io::Error::other(format!("HTTP {} for {url}", resp.status)));
    }
    resp.read_all(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls() {
        let u = Url::parse("http://192.168.1.10:5000").unwrap();
        assert_eq!((u.scheme.as_str(), u.host.as_str(), u.port, u.path.as_str()), ("http", "192.168.1.10", 5000, "/"));
        let u = Url::parse("rtsp://admin:p%40ss@cam.local/Preview_01_main").unwrap();
        assert_eq!(u.user.as_deref(), Some("admin"));
        assert_eq!(u.pass.as_deref(), Some("p@ss"));
        assert_eq!(u.port, 554);
        assert_eq!(u.path, "/Preview_01_main");
        assert_eq!(u.without_credentials(), "rtsp://cam.local:554/Preview_01_main");
        let u = Url::parse("ws://[::1]:5000/ws").unwrap();
        assert_eq!((u.host.as_str(), u.port), ("::1", 5000));
        let u = Url::parse("http://h:5000?x=1").unwrap();
        assert_eq!(u.path, "/?x=1");
        assert!(Url::parse("nope").is_none());
        assert!(Url::parse("http://:80/").is_none());
    }

    #[test]
    fn reads_chunked_body() {
        let raw = b"5\r\nhello\r\n6;ext=1\r\n world\r\n0\r\nX-Trailer: 1\r\n\r\n";
        let headers = vec![("Transfer-Encoding".to_string(), "chunked".to_string())];
        let mut body = Body::new(&raw[..], &headers);
        let mut out = String::new();
        body.read_to_string(&mut out).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn reads_length_body() {
        let headers = vec![("content-length".to_string(), "3".to_string())];
        let mut body = Body::new(&b"abcdef"[..], &headers);
        let mut out = Vec::new();
        body.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"abc");
    }

    #[test]
    fn reads_head() {
        let mut r = &b"HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nX: a:b\r\n\r\nBODY"[..];
        let (status, h) = read_head(&mut r).unwrap();
        assert_eq!(status_code(&status), 200);
        assert_eq!(header(&h, "content-type"), Some("image/jpeg"));
        assert_eq!(header(&h, "x"), Some("a:b"));
        assert_eq!(r, b"BODY");
    }
}
