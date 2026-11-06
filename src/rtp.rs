//! SDP and RTP depacketization for H.264 (RFC 6184) and H.265 (RFC 7798).

use crate::timing::Lateness;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Codec {
    H264,
    H265,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::H265 => "H.265",
        }
    }

    fn nal_type(self, nal: &[u8]) -> u8 {
        match self {
            Codec::H264 => nal[0] & 0x1F,
            Codec::H265 => (nal[0] >> 1) & 0x3F,
        }
    }

    fn is_key(self, t: u8) -> bool {
        match self {
            Codec::H264 => t == 5,
            Codec::H265 => (16..=21).contains(&t),
        }
    }

    fn is_vcl(self, t: u8) -> bool {
        match self {
            Codec::H264 => (1..=5).contains(&t),
            Codec::H265 => t < 32,
        }
    }

    /// Parameter set index: 0 = VPS, 1 = SPS, 2 = PPS.
    fn param_slot(self, t: u8) -> Option<usize> {
        match (self, t) {
            (Codec::H264, 7) => Some(1),
            (Codec::H264, 8) => Some(2),
            (Codec::H265, 32) => Some(0),
            (Codec::H265, 33) => Some(1),
            (Codec::H265, 34) => Some(2),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoTrack {
    pub codec: Codec,
    pub payload_type: u8,
    pub control: String,
    /// Parameter sets from the SDP (VPS/SPS/PPS) without start codes.
    pub params: Vec<Vec<u8>>,
}

/// Picks the first H.264/H.265 video track from the SDP description.
pub fn parse_sdp(sdp: &str) -> Option<VideoTrack> {
    let mut in_video = false;
    let mut track: Option<VideoTrack> = None;
    for line in sdp.lines().map(str::trim) {
        if let Some(m) = line.strip_prefix("m=") {
            if track.is_some() {
                break;
            }
            in_video = m.starts_with("video");
            if in_video {
                let pt = m.split_whitespace().nth(3).and_then(|p| p.parse().ok()).unwrap_or(96);
                track = Some(VideoTrack { codec: Codec::H264, payload_type: pt, control: String::new(), params: vec![] });
            }
            continue;
        }
        let Some(t) = track.as_mut().filter(|_| in_video) else { continue };
        if let Some(r) = line.strip_prefix("a=rtpmap:") {
            let name = r.split_whitespace().nth(1).unwrap_or("").split('/').next().unwrap_or("").to_ascii_uppercase();
            t.codec = match name.as_str() {
                "H264" => Codec::H264,
                "H265" | "HEVC" => Codec::H265,
                _ => return None,
            };
        } else if let Some(c) = line.strip_prefix("a=control:") {
            t.control = c.trim().to_string();
        } else if let Some(f) = line.strip_prefix("a=fmtp:") {
            let params = f.split_once(' ').map(|(_, p)| p).unwrap_or("");
            for kv in params.split(';') {
                let Some((k, v)) = kv.trim().split_once('=') else { continue };
                match k.trim() {
                    "sprop-parameter-sets" => {
                        t.params.extend(v.split(',').filter_map(crate::util::base64_decode).filter(|p| !p.is_empty()));
                    }
                    "sprop-vps" | "sprop-sps" | "sprop-pps" => {
                        t.params.extend(crate::util::base64_decode(v.trim()).filter(|p| !p.is_empty()));
                    }
                    _ => {}
                }
            }
        }
    }
    track
}

/// A complete access unit in Annex B format (with start codes).
#[derive(Clone, Debug)]
pub struct AccessUnit {
    pub data: Vec<u8>,
    pub key: bool,
    /// When it was received (a prebuffered unit keeps the original moment).
    pub arrived: Instant,
    /// How far it came behind the timing of its stream.
    pub late: Duration,
}

pub struct Depacketizer {
    codec: Codec,
    params: [Option<Vec<u8>>; 3],
    nals: Vec<Vec<u8>>,
    nal_bytes: usize,
    fu: Option<Vec<u8>>,
    ts: Option<u32>,
    /// H.264: SPS rewritten to `max_num_reorder_frames = 0` (disabled once B-frames are detected).
    low_delay: bool,
    sps_original: Option<Vec<u8>>,
    lateness: Lateness,
}

const MAX_AU_BYTES: usize = 32 << 20;

impl Depacketizer {
    pub fn new(track: &VideoTrack) -> Self {
        let mut d = Depacketizer {
            codec: track.codec,
            params: [None, None, None],
            nals: vec![],
            nal_bytes: 0,
            fu: None,
            ts: None,
            low_delay: true,
            sps_original: None,
            lateness: Lateness::default(),
        };
        for p in &track.params {
            let p = d.adjust_sps(p.clone());
            if let Some(slot) = d.codec.param_slot(d.codec.nal_type(&p)) {
                d.params[slot] = Some(p);
            }
        }
        d
    }

    fn adjust_sps(&mut self, nal: Vec<u8>) -> Vec<u8> {
        if self.codec != Codec::H264 || self.codec.nal_type(&nal) != 7 {
            return nal;
        }
        self.sps_original = Some(nal.clone());
        if !self.low_delay {
            return nal;
        }
        crate::h264::sps_low_delay(&nal).unwrap_or(nal)
    }

    /// Accepts the payload of one RTP packet; completed units go to `out`.
    pub fn push(&mut self, payload: &[u8], ts: u32, marker: bool, out: &mut Vec<AccessUnit>) {
        if self.ts.is_some_and(|t| t != ts) {
            self.fu = None;
            if let Some(au) = self.flush() {
                out.push(au);
            }
        }
        self.ts = Some(ts);
        match self.codec {
            Codec::H264 => self.push_h264(payload),
            Codec::H265 => self.push_h265(payload),
        }
        if marker {
            if let Some(au) = self.flush() {
                out.push(au);
            }
        }
    }

    fn push_aggregate(&mut self, payload: &[u8], mut i: usize) {
        while i + 2 <= payload.len() {
            let n = u16::from_be_bytes([payload[i], payload[i + 1]]) as usize;
            i += 2;
            if n == 0 || i + n > payload.len() {
                break;
            }
            self.add_nal(payload[i..i + n].to_vec());
            i += n;
        }
    }

    fn push_fragment(&mut self, header: &[u8], data: &[u8], start: bool, end: bool) {
        if start {
            let mut v = Vec::with_capacity(header.len() + data.len() * 8);
            v.extend_from_slice(header);
            v.extend_from_slice(data);
            self.fu = Some(v);
        } else if let Some(v) = self.fu.as_mut() {
            v.extend_from_slice(data);
            if v.len() > MAX_AU_BYTES {
                self.fu = None;
            }
        }
        if end {
            if let Some(v) = self.fu.take() {
                self.add_nal(v);
            }
        }
    }

    fn push_h264(&mut self, p: &[u8]) {
        let Some(&first) = p.first() else { return };
        match first & 0x1F {
            1..=23 => self.add_nal(p.to_vec()),
            24 => self.push_aggregate(p, 1),
            28 if p.len() > 2 => {
                let fu = p[1];
                self.push_fragment(&[(first & 0xE0) | (fu & 0x1F)], &p[2..], fu & 0x80 != 0, fu & 0x40 != 0);
            }
            _ => {}
        }
    }

    fn push_h265(&mut self, p: &[u8]) {
        if p.len() < 3 {
            return;
        }
        match (p[0] >> 1) & 0x3F {
            48 => self.push_aggregate(p, 2),
            49 => {
                let fu = p[2];
                let header = [(p[0] & 0x81) | ((fu & 0x3F) << 1), p[1]];
                self.push_fragment(&header, &p[3..], fu & 0x80 != 0, fu & 0x40 != 0);
            }
            50 => {}
            _ => self.add_nal(p.to_vec()),
        }
    }

    fn add_nal(&mut self, nal: Vec<u8>) {
        if nal.is_empty() {
            return;
        }
        if self.nal_bytes + nal.len() > MAX_AU_BYTES {
            self.nals.clear();
            self.nal_bytes = 0;
        }
        let nal = self.adjust_sps(nal);
        if self.codec == Codec::H264 && self.low_delay && crate::h264::is_b_slice(&nal) {
            crate::log!("H.264 stream has B-frames - disabling SPS rewriting");
            self.low_delay = false;
            if let Some(orig) = &self.sps_original {
                self.params[1] = Some(orig.clone());
            }
        }
        if let Some(slot) = self.codec.param_slot(self.codec.nal_type(&nal)) {
            self.params[slot] = Some(nal.clone());
        }
        self.nal_bytes += nal.len();
        self.nals.push(nal);
    }

    fn flush(&mut self) -> Option<AccessUnit> {
        let nals = std::mem::take(&mut self.nals);
        self.nal_bytes = 0;
        let types: Vec<u8> = nals.iter().map(|n| self.codec.nal_type(n)).collect();
        if !types.iter().any(|&t| self.codec.is_vcl(t)) {
            return None;
        }
        let key = types.iter().any(|&t| self.codec.is_key(t));
        let mut data = Vec::with_capacity(nals.iter().map(|n| n.len() + 4).sum::<usize>() + 256);
        if key {
            for (slot, p) in self.params.iter().enumerate() {
                let Some(p) = p else { continue };
                let present = nals.iter().any(|n| self.codec.param_slot(self.codec.nal_type(n)) == Some(slot));
                if !present {
                    data.extend_from_slice(&[0, 0, 0, 1]);
                    data.extend_from_slice(p);
                }
            }
        }
        for n in &nals {
            data.extend_from_slice(&[0, 0, 0, 1]);
            data.extend_from_slice(n);
        }
        let arrived = Instant::now();
        let late = self.lateness.frame(self.ts.unwrap_or(0), arrived);
        Some(AccessUnit { data, key, arrived, late })
    }
}

/// RTP header: (payload, payload type, marker, timestamp).
pub fn parse_rtp(pkt: &[u8]) -> Option<(&[u8], u8, bool, u32)> {
    if pkt.len() < 12 || pkt[0] >> 6 != 2 {
        return None;
    }
    let cc = (pkt[0] & 0x0F) as usize;
    let mut off = 12 + cc * 4;
    if pkt[0] & 0x10 != 0 {
        let ext = pkt.get(off + 2..off + 4)?;
        off += 4 + u16::from_be_bytes([ext[0], ext[1]]) as usize * 4;
    }
    let mut end = pkt.len();
    if pkt[0] & 0x20 != 0 {
        end = end.checked_sub(*pkt.last()? as usize)?;
    }
    if off > end {
        return None;
    }
    let ts = u32::from_be_bytes([pkt[4], pkt[5], pkt[6], pkt[7]]);
    Some((&pkt[off..end], pkt[1] & 0x7F, pkt[1] & 0x80 != 0, ts))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDP_H264: &str = "v=0\r\no=- 0 0 IN IP4 0.0.0.0\r\ns=-\r\nt=0 0\r\nm=video 0 RTP/AVP 96\r\na=rtpmap:96 H264/90000\r\na=fmtp:96 packetization-mode=1; sprop-parameter-sets=Z0IAH5WoFAFuQA==,aM48gA==; profile-level-id=42001F\r\na=control:trackID=0\r\nm=audio 0 RTP/AVP 97\r\na=rtpmap:97 MPEG4-GENERIC/16000\r\na=control:trackID=1\r\n";
    const SDP_H265: &str = "v=0\r\nm=audio 0 RTP/AVP 0\r\na=control:trackID=0\r\nm=video 0 RTP/AVP 98\r\na=rtpmap:98 H265/90000\r\na=fmtp:98 sprop-vps=QAEMAf//; sprop-sps=QgEBAWA=; sprop-pps=RAHA\r\na=control:rtsp://h/s/trackID=1\r\n";

    fn track264() -> VideoTrack {
        VideoTrack { codec: Codec::H264, payload_type: 96, control: String::new(), params: vec![vec![0x67, 1], vec![0x68, 2]] }
    }

    #[test]
    fn units_say_when_they_arrived_and_how_late() {
        let mut d = Depacketizer::new(&track264());
        let mut out = vec![];
        let before = Instant::now();
        d.push(&[0x65, 1], 90_000, true, &mut out);
        d.push(&[0x41, 2], 180_000, true, &mut out);
        // A timestamp a second older arrives after the others: a second behind its stream.
        d.push(&[0x41, 3], 90_000, true, &mut out);
        assert!(out[0].arrived >= before && out[1].arrived >= out[0].arrived);
        assert!(out[0].late < Duration::from_millis(50));
        assert!(out[2].late > Duration::from_millis(900), "{:?}", out[2].late);
    }

    #[test]
    fn sdp_h264() {
        let t = parse_sdp(SDP_H264).unwrap();
        assert_eq!(t.codec, Codec::H264);
        assert_eq!(t.payload_type, 96);
        assert_eq!(t.control, "trackID=0");
        assert_eq!(t.params.len(), 2);
        assert_eq!(t.params[0][0] & 0x1F, 7);
        assert_eq!(t.params[1][0] & 0x1F, 8);
    }

    #[test]
    fn sdp_h265_after_audio() {
        let t = parse_sdp(SDP_H265).unwrap();
        assert_eq!(t.codec, Codec::H265);
        assert_eq!(t.payload_type, 98);
        assert_eq!(t.control, "rtsp://h/s/trackID=1");
        assert_eq!(t.params.len(), 3);
        assert_eq!((t.params[0][0] >> 1) & 0x3F, 32);
    }

    #[test]
    fn sdp_without_video() {
        assert!(parse_sdp("v=0\r\nm=audio 0 RTP/AVP 0\r\n").is_none());
    }

    #[test]
    fn h264_single_nal_and_param_injection() {
        let mut d = Depacketizer::new(&track264());
        let mut out = vec![];
        d.push(&[0x65, 0xAA, 0xBB], 1000, true, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].key);
        assert_eq!(out[0].data, vec![0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2, 0, 0, 0, 1, 0x65, 0xAA, 0xBB]);
        d.push(&[0x41, 0x01], 4000, true, &mut out);
        assert_eq!(out.len(), 2);
        assert!(!out[1].key);
        assert_eq!(out[1].data, vec![0, 0, 0, 1, 0x41, 0x01]);
    }

    #[test]
    fn h264_stap_a_with_inband_params_not_duplicated() {
        let mut d = Depacketizer::new(&track264());
        let mut out = vec![];
        let stap = [24, 0, 2, 0x67, 9, 0, 2, 0x68, 8, 0, 2, 0x65, 7];
        d.push(&stap, 1, true, &mut out);
        assert_eq!(out[0].data, vec![0, 0, 0, 1, 0x67, 9, 0, 0, 0, 1, 0x68, 8, 0, 0, 0, 1, 0x65, 7]);
    }

    #[test]
    fn h264_fu_a_reassembly() {
        let mut d = Depacketizer::new(&track264());
        let mut out = vec![];
        d.push(&[0x7C, 0x85, 1, 2], 5, false, &mut out);
        d.push(&[0x7C, 0x05, 3], 5, false, &mut out);
        assert!(out.is_empty());
        d.push(&[0x7C, 0x45, 4], 5, true, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].key);
        assert!(out[0].data.ends_with(&[0, 0, 0, 1, 0x65, 1, 2, 3, 4]));
    }

    #[test]
    fn timestamp_change_closes_unit_without_marker() {
        let mut d = Depacketizer::new(&track264());
        let mut out = vec![];
        d.push(&[0x41, 1], 10, false, &mut out);
        d.push(&[0x41, 2], 20, false, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data, vec![0, 0, 0, 1, 0x41, 1]);
    }

    #[test]
    fn params_only_unit_is_stored_not_emitted() {
        let mut d = Depacketizer::new(&VideoTrack { params: vec![], ..track264() });
        let mut out = vec![];
        d.push(&[0x67, 5], 1, true, &mut out);
        d.push(&[0x68, 6], 1, true, &mut out);
        assert!(out.is_empty());
        d.push(&[0x65, 7], 2, true, &mut out);
        assert_eq!(out[0].data, vec![0, 0, 0, 1, 0x67, 5, 0, 0, 0, 1, 0x68, 6, 0, 0, 0, 1, 0x65, 7]);
    }

    #[test]
    fn h265_fu_and_aggregation() {
        let t = VideoTrack {
            codec: Codec::H265,
            payload_type: 98,
            control: String::new(),
            params: vec![vec![0x40, 1, 9], vec![0x42, 1, 8], vec![0x44, 1, 7]],
        };
        let mut d = Depacketizer::new(&t);
        let mut out = vec![];
        // FU with IDR_W_RADL (19): NAL header of type 49 = 0x62 0x01, FU header S|19
        d.push(&[0x62, 0x01, 0x80 | 19, 0xA1], 7, false, &mut out);
        d.push(&[0x62, 0x01, 0x40 | 19, 0xA2], 7, true, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].key);
        let expect_tail = [0, 0, 0, 1, 19 << 1, 0x01, 0xA1, 0xA2];
        assert!(out[0].data.ends_with(&expect_tail));
        assert!(out[0].data.starts_with(&[0, 0, 0, 1, 0x40, 1, 9, 0, 0, 0, 1, 0x42, 1, 8, 0, 0, 0, 1, 0x44, 1, 7]));
        // AP (48) with two NALs of type TRAIL_R (1)
        out.clear();
        d.push(&[0x60, 0x01, 0, 3, 0x02, 0x01, 0xB1, 0, 3, 0x02, 0x01, 0xB2], 8, true, &mut out);
        assert_eq!(out[0].data, vec![0, 0, 0, 1, 2, 1, 0xB1, 0, 0, 0, 1, 2, 1, 0xB2]);
        assert!(!out[0].key);
    }

    #[test]
    fn rtp_header_parsing() {
        let mut pkt = vec![0x80, 0x80 | 96, 0, 1, 0, 0, 0x10, 0, 0, 0, 0, 1];
        pkt.extend_from_slice(&[0x65, 1]);
        let (p, pt, m, ts) = parse_rtp(&pkt).unwrap();
        assert_eq!((p, pt, m, ts), (&[0x65u8, 1][..], 96, true, 0x1000));
        // header extension (1 word) + 2 bytes of padding
        let pkt2 = [0xB0, 96, 0, 1, 0, 0, 0, 5, 0, 0, 0, 1, 0xBE, 0xDE, 0, 1, 9, 9, 9, 9, 0x41, 7, 0, 2];
        let (p, _, m, ts) = parse_rtp(&pkt2).unwrap();
        assert_eq!(p, &[0x41, 7]);
        assert!(!m);
        assert_eq!(ts, 5);
        assert!(parse_rtp(&[0x80, 0]).is_none());
    }
}
