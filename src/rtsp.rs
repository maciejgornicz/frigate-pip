//! RTSP client (RTP over interleaved TCP) with Basic/Digest authentication.

use crate::audio::{self, AacDepacketizer, AudioCodec, AudioTrack};
use crate::net::{self, Url};
use crate::rtp::{self, AccessUnit, Depacketizer, VideoTrack};
use crate::util::{base64_encode, md5_hex, random_u64, Cancel};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// Stream data: a video unit (Annex B) or an audio frame (AAC / G.711).
pub enum Media {
    Video(AccessUnit),
    Audio(Vec<u8>),
}

fn interleaved_channel(headers: &[(String, String)]) -> Option<u8> {
    net::header(headers, "Transport")
        .and_then(|t| t.split(';').find_map(|p| p.trim().strip_prefix("interleaved=")))
        .and_then(|p| p.split('-').next()?.parse::<u8>().ok())
}

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

enum Auth {
    None,
    Basic,
    Digest { realm: String, nonce: String, opaque: Option<String>, qop: bool },
}

struct Conn {
    r: BufReader<TcpStream>,
    w: TcpStream,
    cseq: u32,
    user: Option<String>,
    pass: Option<String>,
    auth: Auth,
    session: Option<String>,
    nc: u32,
}

fn auth_param(h: &str, name: &str) -> Option<String> {
    let lower = h.to_ascii_lowercase();
    let mut from = 0;
    while let Some(p) = lower[from..].find(&format!("{name}=")) {
        let start = from + p;
        let boundary = start == 0 || matches!(lower.as_bytes()[start - 1], b' ' | b',' | b'\t');
        let v = &h[start + name.len() + 1..];
        if boundary {
            return Some(if let Some(q) = v.strip_prefix('"') {
                q[..q.find('"').unwrap_or(q.len())].to_string()
            } else {
                v[..v.find(',').unwrap_or(v.len())].trim().to_string()
            });
        }
        from = start + 1;
    }
    None
}

impl Conn {
    fn auth_header(&mut self, method: &str, uri: &str) -> Option<String> {
        let (user, pass) = (self.user.as_deref()?, self.pass.as_deref().unwrap_or(""));
        match &self.auth {
            Auth::None => None,
            Auth::Basic => Some(format!("Basic {}", base64_encode(format!("{user}:{pass}").as_bytes()))),
            Auth::Digest { realm, nonce, opaque, qop } => {
                let ha1 = md5_hex(format!("{user}:{realm}:{pass}").as_bytes());
                let ha2 = md5_hex(format!("{method}:{uri}").as_bytes());
                let mut h = format!("Digest username=\"{user}\", realm=\"{realm}\", nonce=\"{nonce}\", uri=\"{uri}\"");
                if *qop {
                    self.nc += 1;
                    let nc = format!("{:08x}", self.nc);
                    let cnonce = format!("{:016x}", random_u64());
                    let resp = md5_hex(format!("{ha1}:{nonce}:{nc}:{cnonce}:auth:{ha2}").as_bytes());
                    h.push_str(&format!(", response=\"{resp}\", qop=auth, nc={nc}, cnonce=\"{cnonce}\""));
                } else {
                    let resp = md5_hex(format!("{ha1}:{nonce}:{ha2}").as_bytes());
                    h.push_str(&format!(", response=\"{resp}\""));
                }
                if let Some(o) = opaque {
                    h.push_str(&format!(", opaque=\"{o}\""));
                }
                Some(h)
            }
        }
    }

    fn send(&mut self, method: &str, uri: &str, extra: &[(&str, &str)]) -> io::Result<()> {
        self.cseq += 1;
        let mut req = format!("{method} {uri} RTSP/1.0\r\nCSeq: {}\r\nUser-Agent: FrigatePiP\r\n", self.cseq);
        if let Some(a) = self.auth_header(method, uri) {
            req.push_str(&format!("Authorization: {a}\r\n"));
        }
        if let Some(s) = &self.session {
            req.push_str(&format!("Session: {s}\r\n"));
        }
        for (k, v) in extra {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        self.w.write_all(req.as_bytes())
    }

    /// Reads an RTSP response, skipping any interleaved frames.
    fn read_resp(&mut self) -> io::Result<Resp> {
        loop {
            let first = self.r.fill_buf()?.first().copied();
            match first {
                None => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the RTSP server closed the connection")),
                Some(b'$') => {
                    let mut h = [0u8; 4];
                    self.r.read_exact(&mut h)?;
                    let n = u16::from_be_bytes([h[2], h[3]]) as usize;
                    io::copy(&mut (&mut self.r).take(n as u64), &mut io::sink())?;
                }
                Some(b'R') => {
                    let (status, headers) = net::read_head(&mut self.r)?;
                    let len = net::header(&headers, "Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0usize);
                    let mut body = vec![0u8; len.min(1 << 20)];
                    self.r.read_exact(&mut body)?;
                    return Ok(Resp { status: net::status_code(&status), headers, body });
                }
                Some(_) => self.r.consume(1),
            }
        }
    }

    fn request(&mut self, method: &str, uri: &str, extra: &[(&str, &str)]) -> Result<Resp, String> {
        for attempt in 0..2 {
            self.send(method, uri, extra).map_err(|e| format!("{method}: {e}"))?;
            let resp = self.read_resp().map_err(|e| format!("{method}: {e}"))?;
            if resp.status == 401 && attempt == 0 && self.user.is_some() {
                let www: Vec<&str> =
                    resp.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case("WWW-Authenticate")).map(|(_, v)| v.as_str()).collect();
                if let Some(d) = www.iter().find(|h| h.to_ascii_lowercase().starts_with("digest")) {
                    self.auth = Auth::Digest {
                        realm: auth_param(d, "realm").unwrap_or_default(),
                        nonce: auth_param(d, "nonce").unwrap_or_default(),
                        opaque: auth_param(d, "opaque"),
                        qop: auth_param(d, "qop").is_some_and(|q| q.split(',').any(|x| x.trim() == "auth")),
                    };
                } else {
                    self.auth = Auth::Basic;
                }
                continue;
            }
            if resp.status != 200 {
                return Err(format!("{method} → RTSP {}", resp.status));
            }
            return Ok(resp);
        }
        Err(format!("{method}: access denied (check the RTSP user/password)"))
    }
}

fn resolve(base: &str, control: &str) -> String {
    if control.is_empty() || control == "*" {
        base.to_string()
    } else if control.starts_with("rtsp://") || control.starts_with("rtsps://") {
        control.to_string()
    } else {
        format!("{}/{}", base.trim_end_matches('/'), control.trim_start_matches('/'))
    }
}

/// Connects and performs DESCRIBE: (connection, base URL, SDP).
fn describe(url: &str, cancel: &Cancel) -> Result<(Conn, String, String), String> {
    let u = Url::parse(url).ok_or_else(|| format!("invalid RTSP address {url}"))?;
    let s = net::connect(&u.host, u.port, Duration::from_secs(3), Some(cancel)).map_err(|e| format!("RTSP {}: {e}", u.host))?;
    s.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let mut c = Conn {
        r: BufReader::with_capacity(256 * 1024, s.try_clone().map_err(|e| e.to_string())?),
        w: s,
        cseq: 0,
        user: u.user.clone(),
        pass: u.pass.clone(),
        auth: Auth::None,
        session: None,
        nc: 0,
    };
    let base = u.without_credentials();
    let d = c.request("DESCRIBE", &base, &[("Accept", "application/sdp")])?;
    let content_base = net::header(&d.headers, "Content-Base").map(str::to_string).unwrap_or(base);
    Ok((c, content_base, String::from_utf8_lossy(&d.body).into_owned()))
}

/// Checks the stream without receiving data (connection test in settings).
pub fn probe(url: &str, cancel: &Cancel) -> Result<(VideoTrack, Option<AudioTrack>), String> {
    let (_, _, sdp) = describe(url, cancel)?;
    let video = rtp::parse_sdp(&sdp).ok_or("the stream has no H.264/H.265 video")?;
    Ok((video, audio::parse_sdp_audio(&sdp)))
}

/// Receives video until an error, cancellation, or `on_au` returning `false`.
pub fn run(
    url: &str,
    cancel: &Cancel,
    want_audio: bool,
    mut on_track: impl FnMut(&VideoTrack, Option<&AudioTrack>),
    mut on_media: impl FnMut(Media) -> bool,
) -> Result<(), String> {
    let (mut c, content_base, sdp) = describe(url, cancel)?;
    let host = Url::parse(url).map(|u| u.host).unwrap_or_default();
    let track = rtp::parse_sdp(&sdp).ok_or("the stream has no H.264/H.265 video")?;
    let mut audio = if want_audio { audio::parse_sdp_audio(&sdp) } else { None };
    let setup = c.request("SETUP", &resolve(&content_base, &track.control), &[("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1")])?;
    c.session = net::header(&setup.headers, "Session").map(|s| s.split(';').next().unwrap_or("").trim().to_string());
    let channel = interleaved_channel(&setup.headers).unwrap_or(0);
    let mut audio_channel = None;
    if let Some(a) = &audio {
        // SETUP audio and PLAY in a single write: audio adds no server round trip before video starts.
        let audio_uri = resolve(&content_base, &a.control);
        c.send("SETUP", &audio_uri, &[("Transport", "RTP/AVP/TCP;unicast;interleaved=2-3")]).map_err(|e| e.to_string())?;
        c.send("PLAY", &content_base, &[("Range", "npt=0.000-")]).map_err(|e| e.to_string())?;
        let sa = c.read_resp().map_err(|e| format!("SETUP audio: {e}"))?;
        if sa.status == 200 {
            audio_channel = Some(interleaved_channel(&sa.headers).unwrap_or(2));
        } else {
            crate::log!("RTSP {host}: audio track rejected ({})", sa.status);
        }
        let play = c.read_resp().map_err(|e| format!("PLAY: {e}"))?;
        if play.status != 200 {
            return Err(format!("PLAY → RTSP {}", play.status));
        }
    } else {
        c.request("PLAY", &content_base, &[("Range", "npt=0.000-")])?;
    }
    if audio_channel.is_none() {
        audio = None;
    }
    on_track(&track, audio.as_ref());

    let mut depack = Depacketizer::new(&track);
    let mut aac = match audio.as_ref().map(|a| &a.codec) {
        Some(AudioCodec::Aac { size_len, index_len, index_delta_len, .. }) => Some(AacDepacketizer::new(*size_len, *index_len, *index_delta_len)),
        _ => None,
    };
    let mut audio_frames = Vec::new();
    let mut out = Vec::new();
    let mut pkt = vec![0u8; 65536];
    let mut last_keepalive = Instant::now();
    loop {
        if cancel.is_set() {
            return Ok(());
        }
        let first = c.r.fill_buf().map_err(|e| format!("RTSP: {e}"))?.first().copied();
        match first {
            None => return Err("the RTSP server closed the connection".into()),
            Some(b'$') => {
                let mut h = [0u8; 4];
                c.r.read_exact(&mut h).map_err(|e| e.to_string())?;
                let n = u16::from_be_bytes([h[2], h[3]]) as usize;
                c.r.read_exact(&mut pkt[..n]).map_err(|e| e.to_string())?;
                let Some((payload, pt, marker, ts)) = rtp::parse_rtp(&pkt[..n]) else { continue };
                if h[1] == channel && pt == track.payload_type {
                    depack.push(payload, ts, marker, &mut out);
                    for au in out.drain(..) {
                        if !on_media(Media::Video(au)) {
                            return Ok(());
                        }
                    }
                } else if Some(h[1]) == audio_channel && audio.as_ref().is_some_and(|a| a.payload_type == pt) {
                    match aac.as_mut() {
                        Some(d) => d.push(payload, &mut audio_frames),
                        None => audio_frames.push(payload.to_vec()),
                    }
                    for f in audio_frames.drain(..) {
                        if !on_media(Media::Audio(f)) {
                            return Ok(());
                        }
                    }
                }
            }
            Some(b'R') => {
                c.read_resp().map_err(|e| e.to_string())?;
            }
            Some(_) => c.r.consume(1),
        }
        if last_keepalive.elapsed() > Duration::from_secs(25) {
            last_keepalive = Instant::now();
            c.send("GET_PARAMETER", &content_base, &[]).map_err(|e| e.to_string())?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_auth_params() {
        let h = r#"Digest realm="IP Camera(C1234)", nonce="abc,def", qop="auth", opaque=xyz"#;
        assert_eq!(auth_param(h, "realm").as_deref(), Some("IP Camera(C1234)"));
        assert_eq!(auth_param(h, "nonce").as_deref(), Some("abc,def"));
        assert_eq!(auth_param(h, "qop").as_deref(), Some("auth"));
        assert_eq!(auth_param(h, "opaque").as_deref(), Some("xyz"));
        assert_eq!(auth_param(h, "stale"), None);
    }

    #[test]
    fn resolves_control_urls() {
        assert_eq!(resolve("rtsp://h:8554/cam/", "trackID=0"), "rtsp://h:8554/cam/trackID=0");
        assert_eq!(resolve("rtsp://h:8554/cam", "trackID=0"), "rtsp://h:8554/cam/trackID=0");
        assert_eq!(resolve("rtsp://h/cam", "rtsp://x/y"), "rtsp://x/y");
        assert_eq!(resolve("rtsp://h/cam", "*"), "rtsp://h/cam");
    }

    #[test]
    fn digest_response_matches_rfc2617_style() {
        let mut c = Conn {
            r: BufReader::new(local_stream()),
            w: local_stream(),
            cseq: 0,
            user: Some("admin".into()),
            pass: Some("pw".into()),
            auth: Auth::Digest { realm: "r".into(), nonce: "n".into(), opaque: None, qop: false },
            session: None,
            nc: 0,
        };
        let h = c.auth_header("DESCRIBE", "rtsp://h/c").unwrap();
        let ha1 = md5_hex(b"admin:r:pw");
        let ha2 = md5_hex(b"DESCRIBE:rtsp://h/c");
        let expect = md5_hex(format!("{ha1}:n:{ha2}").as_bytes());
        assert!(h.contains(&format!("response=\"{expect}\"")), "{h}");
    }

    /// go2rtc address and stream names for the live tests, e.g.:
    /// `FRIGATE_RTSP=rtsp://192.168.1.10:8554 FRIGATE_STREAMS=front_door_sub,garden cargo test --release live_ -- --ignored --nocapture`
    fn live_setup() -> Option<(String, Vec<String>)> {
        let base = std::env::var("FRIGATE_RTSP").ok()?;
        let streams: Vec<String> =
            std::env::var("FRIGATE_STREAMS").ok()?.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if streams.is_empty() {
            return None;
        }
        Some((base.trim_end_matches('/').to_string(), streams))
    }

    const LIVE_HELP: &str = "skipped: set FRIGATE_RTSP (e.g. rtsp://192.168.1.10:8554) and FRIGATE_STREAMS (comma-separated go2rtc stream names)";

    /// Receives ~3 s of each go2rtc stream and decodes it in software and hardware.
    #[test]
    #[cfg(windows)]
    #[ignore = "requires Frigate on the network"]
    fn live_decode() {
        use crate::decoder::{Decoder, Frame};
        use windows::Win32::Graphics::Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT};
        let Some((base, streams)) = live_setup() else { return println!("{LIVE_HELP}") };
        let device = crate::render::create_device(D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT).unwrap();
        for stream in &streams {
            for hw in [false, true] {
                let cancel = Cancel::new();
                let t0 = Instant::now();
                let track = std::cell::Cell::new(None);
                let mut dec: Option<Decoder> = None;
                let (mut aus, mut frames, mut first_key, mut first_frame) = (0, 0, None, None);
                let mut info = String::new();
                let mut fed = 0;
                let mut timeline = String::new();
                let r = run(
                    &format!("{base}/{stream}"),
                    &cancel,
                    false,
                    |t, _| track.set(Some(t.codec)),
                    |m| {
                        let Media::Video(au) = m else { return true };
                        aus += 1;
                        if dec.is_none() {
                            if !au.key {
                                return true;
                            }
                            first_key = Some(t0.elapsed().as_millis());
                            let codec = track.get().unwrap();
                            dec = Some(Decoder::new(codec, if hw { Some(&device) } else { None }).unwrap());
                        }
                        let d = dec.as_mut().unwrap();
                        fed += 1;
                        let before = frames;
                        let r = d.decode(&au.data, &mut |f| {
                            frames += 1;
                            if first_frame.is_none() {
                                first_frame = Some(t0.elapsed().as_millis());
                                info = match &f {
                                    Frame::Gpu { width, height, .. } => format!("GPU {width}x{height}"),
                                    Frame::Cpu { width, height, .. } => format!("CPU {width}x{height}"),
                                };
                            }
                        });
                        if fed <= 12 {
                            timeline.push_str(&format!("{}{}{} ", if au.key { "K" } else { "" }, frames - before, if r.is_err() { "!" } else { "" }));
                        }
                        if let Err(e) = r {
                            println!("    decoding error: {e}");
                            return false;
                        }
                        t0.elapsed() < Duration::from_secs(3)
                    },
                );
                let hw_used = dec.as_ref().map(|d| d.hw).unwrap_or(false);
                println!(
                    "{stream:16} hw={hw:5} ({hw_used:5}) {:?} codec={:?} AU={aus} frames={frames} keyframe@{first_key:?}ms first@{first_frame:?}ms {info}",
                    r.err(),
                    track.get()
                );
                println!("    output per input: {timeline}");
            }
        }
    }

    /// Audio from go2rtc: AAC track, frames and decoding.
    #[test]
    #[cfg(windows)]
    #[ignore = "requires Frigate on the network"]
    fn live_audio_decode() {
        use crate::audio::AudioDecoder;
        let Some((base, streams)) = live_setup() else { return println!("{LIVE_HELP}") };
        let cancel = Cancel::new();
        let t0 = Instant::now();
        let dec: std::cell::RefCell<Option<AudioDecoder>> = Default::default();
        let (mut frames, mut samples, mut videos) = (0, 0usize, 0);
        let mut first_audio = None;
        let mut first_video = None;
        let mut pcm = Vec::new();
        let r = run(
            &format!("{base}/{}", streams[0]),
            &cancel,
            true,
            |_, a| {
                let a = a.expect("no audio track");
                println!("audio track: {:?} {} Hz {} ch. after {} ms", a.codec, a.rate, a.channels, t0.elapsed().as_millis());
                *dec.borrow_mut() = Some(AudioDecoder::new(a).unwrap());
            },
            |m| {
                match m {
                    Media::Audio(f) => {
                        frames += 1;
                        first_audio.get_or_insert(t0.elapsed().as_millis());
                        pcm.clear();
                        dec.borrow_mut().as_mut().unwrap().decode(&f, &mut pcm).unwrap();
                        samples += pcm.len();
                    }
                    Media::Video(_) => {
                        videos += 1;
                        first_video.get_or_insert(t0.elapsed().as_millis());
                    }
                }
                t0.elapsed() < Duration::from_secs(3)
            },
        );
        let d = dec.into_inner().unwrap();
        let secs = (t0.elapsed().as_secs_f64() - first_audio.unwrap_or(0) as f64 / 1000.0).max(0.1);
        println!(
            "{:?}: audio frames {frames} (first after {first_audio:?} ms), video {videos} (first after {first_video:?} ms), PCM {:.0} samples/s per channel (output {} Hz)",
            r.err(),
            samples as f64 / d.channels as f64 / secs,
            d.rate
        );
        assert!(frames > 20 && samples > 0);
    }

    /// Level of decoded camera audio (peak and RMS in dBFS every 0.5 s).
    #[test]
    #[cfg(windows)]
    #[ignore = "requires Frigate on the network"]
    fn live_audio_levels() {
        use crate::audio::AudioDecoder;
        let Some((base, streams)) = live_setup() else { return println!("{LIVE_HELP}") };
        let db = |v: f64| if v <= 0.0 { -120.0 } else { 20.0 * (v / 32768.0).log10() };
        for cam in &streams {
            let cancel = Cancel::new();
            let t0 = Instant::now();
            let dec: std::cell::RefCell<Option<AudioDecoder>> = Default::default();
            let mut pcm = Vec::new();
            let mut window: Vec<i16> = Vec::new();
            let mut line = String::new();
            let (mut nframes, mut nsamples) = (0, 0usize);
            let res = run(
                &format!("{base}/{cam}"),
                &cancel,
                true,
                |_, a| *dec.borrow_mut() = a.map(|a| AudioDecoder::new(a).unwrap()),
                |m| {
                    if let Media::Audio(f) = m {
                        pcm.clear();
                        let mut d = dec.borrow_mut();
                        let d = d.as_mut().unwrap();
                        d.decode(&f, &mut pcm).unwrap();
                        nframes += 1;
                        nsamples += pcm.len();
                        window.extend_from_slice(&pcm);
                        if window.len() >= (d.rate as usize * d.channels as usize) / 2 {
                            let peak = window.iter().map(|s| (*s as f64).abs()).fold(0.0, f64::max);
                            let rms = (window.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / window.len() as f64).sqrt();
                            line.push_str(&format!("{:.0}/{:.0} ", db(peak), db(rms)));
                            window.clear();
                        }
                    }
                    t0.elapsed() < Duration::from_secs(4)
                },
            );
            let fmt = dec.borrow().as_ref().map(|d| format!("{}x{}", d.rate, d.channels));
            println!(
                "{cam:16} frames {nframes}, samples {nsamples}, format {fmt:?}, result {:?}, {} ms\n    peak/RMS dBFS every 0.5 s: {line}",
                res.err(),
                t0.elapsed().as_millis()
            );
        }
    }

    fn local_stream() -> TcpStream {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let s = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        let _ = l.accept().unwrap();
        s
    }
}
