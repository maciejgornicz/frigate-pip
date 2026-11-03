//! Minimal WebSocket client (RFC 6455) for `ws://` and `wss://`.

use crate::net::{self, Conn, Url};
use crate::util::{base64_encode, random_u64, Cancel};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::time::Duration;

pub enum Msg {
    Text(String),
    Binary,
    Pong,
    Close,
    /// No data received within `read_timeout`.
    Idle,
}

/// Reads go through the buffer, writes straight to the connection under it.
pub struct WebSocket {
    r: BufReader<Conn>,
}

const MAX_MESSAGE: usize = 16 << 20;

pub fn encode_frame(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut f = Vec::with_capacity(payload.len() + 14);
    f.push(0x80 | opcode);
    match payload.len() {
        n if n < 126 => f.push(0x80 | n as u8),
        n if n <= 0xFFFF => {
            f.push(0x80 | 126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(0x80 | 127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    f.extend_from_slice(&mask);
    f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    f
}

impl WebSocket {
    /// Opens the connection with extra request `headers` (e.g. `Authorization`).
    pub fn connect(url: &str, headers: &[(&str, String)], timeout: Duration, read_timeout: Duration, cancel: &Cancel) -> io::Result<WebSocket> {
        let u = Url::parse(url).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid address {url}")))?;
        if u.scheme != "ws" && u.scheme != "wss" {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "only ws:// and wss:// are supported"));
        }
        let mut s = net::open(&u, timeout, Some(cancel))?;
        let key = base64_encode(&random_u64().to_le_bytes().iter().chain(random_u64().to_le_bytes().iter()).copied().collect::<Vec<u8>>());
        let mut req = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: FrigatePiP\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n",
            u.path,
            u.host_header()
        );
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes())?;
        s.flush()?;
        let mut r = BufReader::with_capacity(64 * 1024, s);
        let (status, _) = net::read_head(&mut r)?;
        let code = net::status_code(&status);
        if code != 101 {
            let kind = if code == 401 { io::ErrorKind::PermissionDenied } else { io::ErrorKind::ConnectionRefused };
            return Err(io::Error::new(kind, format!("WebSocket rejected: {status}")));
        }
        r.get_ref().set_read_timeout(Some(read_timeout))?;
        Ok(WebSocket { r })
    }

    fn send(&mut self, opcode: u8, payload: &[u8]) -> io::Result<()> {
        let mask = (random_u64() as u32).to_le_bytes();
        let w = self.r.get_mut();
        w.write_all(&encode_frame(opcode, payload, mask))?;
        w.flush()
    }

    pub fn send_ping(&mut self) -> io::Result<()> {
        self.send(0x9, b"fpip")
    }

    /// Receives the next message. Handles server pings internally.
    pub fn read(&mut self) -> io::Result<Msg> {
        let mut message: Vec<u8> = Vec::new();
        let mut message_op = 0u8;
        loop {
            if message.is_empty() {
                // Waiting between frames: a timeout means the connection is idle.
                match self.r.fill_buf() {
                    Ok([]) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed")),
                    Ok(_) => {}
                    Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => return Ok(Msg::Idle),
                    Err(e) => return Err(e),
                }
            }
            let mut h = [0u8; 2];
            self.r.read_exact(&mut h)?;
            let fin = h[0] & 0x80 != 0;
            let op = h[0] & 0x0F;
            let masked = h[1] & 0x80 != 0;
            let len = match h[1] & 0x7F {
                126 => {
                    let mut b = [0u8; 2];
                    self.r.read_exact(&mut b)?;
                    u16::from_be_bytes(b) as usize
                }
                127 => {
                    let mut b = [0u8; 8];
                    self.r.read_exact(&mut b)?;
                    u64::from_be_bytes(b) as usize
                }
                n => n as usize,
            };
            if len > MAX_MESSAGE || message.len() + len > MAX_MESSAGE {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "WebSocket message too large"));
            }
            let mut mask = [0u8; 4];
            if masked {
                self.r.read_exact(&mut mask)?;
            }
            let mut payload = vec![0u8; len];
            self.r.read_exact(&mut payload)?;
            if masked {
                payload.iter_mut().enumerate().for_each(|(i, b)| *b ^= mask[i % 4]);
            }
            match op {
                0x9 => self.send(0xA, &payload)?,
                0xA => return Ok(Msg::Pong),
                0x8 => return Ok(Msg::Close),
                0x0..=0x2 => {
                    if op != 0 {
                        message_op = op;
                    }
                    message.extend_from_slice(&payload);
                    if fin {
                        return Ok(if message_op == 0x1 { Msg::Text(String::from_utf8_lossy(&message).into_owned()) } else { Msg::Binary });
                    }
                }
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "unknown WebSocket opcode")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_masked_frames() {
        let f = encode_frame(0x1, b"Hello", [0x37, 0xfa, 0x21, 0x3d]);
        assert_eq!(f, vec![0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58]);
        let big = encode_frame(0x2, &[0u8; 300], [0; 4]);
        assert_eq!(&big[..4], &[0x82, 0x80 | 126, 0x01, 0x2C]);
        assert_eq!(big.len(), 4 + 4 + 300);
    }
}
