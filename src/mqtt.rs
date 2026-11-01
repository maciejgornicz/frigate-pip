//! Minimal MQTT 3.1.1 client (subscribe only, QoS 0/1).

use crate::net;
use crate::util::Cancel;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub enum Packet {
    Publish { topic: String, payload: Vec<u8> },
    Other,
    Idle,
}

pub struct Mqtt {
    r: BufReader<TcpStream>,
    w: TcpStream,
}

pub fn encode_len(mut n: usize, out: &mut Vec<u8>) {
    loop {
        let mut b = (n % 128) as u8;
        n /= 128;
        if n > 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}

fn put_str(s: &str, out: &mut Vec<u8>) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn packet(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut p = vec![kind];
    encode_len(body.len(), &mut p);
    p.extend_from_slice(body);
    p
}

pub fn connect_packet(client_id: &str, user: &str, pass: &str, keepalive: u16) -> Vec<u8> {
    let mut b = Vec::new();
    put_str("MQTT", &mut b);
    b.push(4);
    let mut flags = 0x02;
    if !user.is_empty() {
        flags |= 0x80;
        if !pass.is_empty() {
            flags |= 0x40;
        }
    }
    b.push(flags);
    b.extend_from_slice(&keepalive.to_be_bytes());
    put_str(client_id, &mut b);
    if !user.is_empty() {
        put_str(user, &mut b);
        if !pass.is_empty() {
            put_str(pass, &mut b);
        }
    }
    packet(0x10, &b)
}

impl Mqtt {
    #[allow(clippy::too_many_arguments)]
    pub fn connect(
        host: &str,
        port: u16,
        client_id: &str,
        user: &str,
        pass: &str,
        keepalive: u16,
        timeout: Duration,
        cancel: &Cancel,
    ) -> io::Result<Mqtt> {
        let mut s = net::connect(host, port, timeout, Some(cancel))?;
        s.set_read_timeout(Some(timeout))?;
        s.write_all(&connect_packet(client_id, user, pass, keepalive))?;
        let mut r = BufReader::new(s.try_clone()?);
        let mut ack = [0u8; 4];
        r.read_exact(&mut ack)?;
        if ack[0] != 0x20 || ack[3] != 0 {
            let why = match ack[3] {
                4 | 5 => "wrong MQTT user/password",
                _ => "the MQTT broker rejected the connection",
            };
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, why));
        }
        s.set_read_timeout(Some(Duration::from_secs(keepalive as u64 * 3 / 4)))?;
        Ok(Mqtt { r, w: s })
    }

    pub fn subscribe(&mut self, topics: &[String]) -> io::Result<()> {
        let mut b = vec![0, 1];
        for t in topics {
            put_str(t, &mut b);
            b.push(0);
        }
        self.w.write_all(&packet(0x82, &b))
    }

    pub fn ping(&mut self) -> io::Result<()> {
        self.w.write_all(&[0xC0, 0])
    }

    pub fn read(&mut self) -> io::Result<Packet> {
        match self.r.fill_buf() {
            Ok([]) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the broker closed the connection")),
            Ok(_) => {}
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => return Ok(Packet::Idle),
            Err(e) => return Err(e),
        }
        let mut h = [0u8; 1];
        self.r.read_exact(&mut h)?;
        let mut len = 0usize;
        let mut mul = 1usize;
        for _ in 0..4 {
            let mut b = [0u8; 1];
            self.r.read_exact(&mut b)?;
            len += (b[0] & 0x7F) as usize * mul;
            mul *= 128;
            if b[0] & 0x80 == 0 {
                break;
            }
        }
        if len > 16 << 20 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "MQTT packet too large"));
        }
        let mut body = vec![0u8; len];
        self.r.read_exact(&mut body)?;
        if h[0] >> 4 != 3 || body.len() < 2 {
            return Ok(Packet::Other);
        }
        let qos = (h[0] >> 1) & 3;
        let tlen = u16::from_be_bytes([body[0], body[1]]) as usize;
        let mut i = 2 + tlen;
        if i > body.len() {
            return Ok(Packet::Other);
        }
        let topic = String::from_utf8_lossy(&body[2..i]).into_owned();
        if qos > 0 {
            if i + 2 > body.len() {
                return Ok(Packet::Other);
            }
            self.w.write_all(&[0x40, 2, body[i], body[i + 1]])?;
            i += 2;
        }
        Ok(Packet::Publish { topic, payload: body[i..].to_vec() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_length_encoding() {
        let mut v = vec![];
        encode_len(0, &mut v);
        encode_len(127, &mut v);
        encode_len(128, &mut v);
        encode_len(16383, &mut v);
        encode_len(2_097_152, &mut v);
        assert_eq!(v, vec![0x00, 0x7F, 0x80, 0x01, 0xFF, 0x7F, 0x80, 0x80, 0x80, 0x01]);
    }

    #[test]
    fn connect_packet_layout() {
        let p = connect_packet("fpip", "u", "p", 30);
        assert_eq!(p, vec![0x10, 22, 0, 4, b'M', b'Q', b'T', b'T', 4, 0xC2, 0, 30, 0, 4, b'f', b'p', b'i', b'p', 0, 1, b'u', 0, 1, b'p']);
        let anon = connect_packet("x", "", "", 30);
        assert_eq!(anon[9], 0x02);
    }
}
