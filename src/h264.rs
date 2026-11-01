//! Rewrites the H.264 SPS so that the decoder outputs frames immediately.
//!
//! Cameras often send an SPS without VUI/bitstream_restriction. The decoder must then
//! assume the maximum DPB size for the level (e.g. 16 frames) and holds back
//! output until the next IDR frame. Adding `max_num_reorder_frames = 0`
//! is valid for streams without B-frames (typical for IP cameras).

/// Removes emulation prevention bytes (00 00 03 → 00 00).
pub fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

/// Inserts emulation prevention bytes.
pub fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b <= 3 {
            out.push(3);
            zeros = 0;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

struct Reader<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn u(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = *self.d.get(self.pos / 8)?;
            v = v << 1 | ((byte >> (7 - self.pos % 8)) & 1) as u32;
            self.pos += 1;
        }
        Some(v)
    }

    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.u(1)? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1u32 << zeros) - 1 + self.u(zeros)?)
    }

    fn se(&mut self) -> Option<i32> {
        let v = self.ue()?;
        Some(if v % 2 == 1 { (v / 2 + 1) as i32 } else { -((v / 2) as i32) })
    }
}

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
    cur: u8,
    n: u8,
}

impl Writer {
    fn put(&mut self, bits: u32, v: u32) {
        for i in (0..bits).rev() {
            self.cur = self.cur << 1 | ((v >> i) & 1) as u8;
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.cur);
                self.cur = 0;
                self.n = 0;
            }
        }
    }

    fn ue(&mut self, v: u32) {
        let x = v + 1;
        let len = 32 - x.leading_zeros();
        self.put(len - 1, 0);
        self.put(len, x);
    }

    fn copy(&mut self, src: &[u8], bits: usize) {
        for p in 0..bits {
            self.put(1, ((src[p / 8] >> (7 - p % 8)) & 1) as u32);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        self.put(1, 1);
        while self.n != 0 {
            self.put(1, 0);
        }
        self.out
    }
}

fn skip_scaling_list(r: &mut Reader, size: u32) -> Option<()> {
    let (mut last, mut next) = (8i32, 8i32);
    for _ in 0..size {
        if next != 0 {
            next = (last + r.se()? + 256) % 256;
        }
        last = if next == 0 { last } else { next };
    }
    Some(())
}

fn skip_hrd(r: &mut Reader) -> Option<()> {
    let cnt = r.ue()?;
    r.u(8)?;
    for _ in 0..=cnt.min(31) {
        r.ue()?;
        r.ue()?;
        r.u(1)?;
    }
    r.u(20)?;
    Some(())
}

#[derive(Debug, PartialEq)]
pub struct SpsInfo {
    pub width: u32,
    pub height: u32,
    pub num_ref_frames: u32,
    /// `None` if the SPS has no bitstream_restriction.
    pub max_num_reorder: Option<u32>,
    vui_flag_pos: usize,
}

/// Parses the SPS RBSP (without the NAL header).
fn parse(rbsp: &[u8]) -> Option<SpsInfo> {
    let mut r = Reader { d: rbsp, pos: 0 };
    let profile = r.u(8)?;
    r.u(16)?;
    r.ue()?;
    let mut chroma = 1;
    if [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135].contains(&profile) {
        chroma = r.ue()?;
        if chroma == 3 {
            r.u(1)?;
        }
        r.ue()?;
        r.ue()?;
        r.u(1)?;
        if r.u(1)? == 1 {
            for i in 0..if chroma != 3 { 8 } else { 12 } {
                if r.u(1)? == 1 {
                    skip_scaling_list(&mut r, if i < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    r.ue()?;
    match r.ue()? {
        0 => {
            r.ue()?;
        }
        1 => {
            r.u(1)?;
            r.se()?;
            r.se()?;
            for _ in 0..r.ue()?.min(255) {
                r.se()?;
            }
        }
        _ => {}
    }
    let num_ref_frames = r.ue()?;
    r.u(1)?;
    let w_mbs = r.ue()? + 1;
    let h_units = r.ue()? + 1;
    let frame_mbs_only = r.u(1)?;
    if frame_mbs_only == 0 {
        r.u(1)?;
    }
    r.u(1)?;
    let mut crop = [0u32; 4];
    if r.u(1)? == 1 {
        for c in crop.iter_mut() {
            *c = r.ue()?;
        }
    }
    let (cx, cy) = match chroma {
        0 => (1, 2 - frame_mbs_only),
        1 => (2, 2 * (2 - frame_mbs_only)),
        2 => (2, 2 - frame_mbs_only),
        _ => (1, 2 - frame_mbs_only),
    };
    let width = (w_mbs * 16).checked_sub(cx * (crop[0] + crop[1]))?;
    let height = (h_units * 16 * (2 - frame_mbs_only)).checked_sub(cy * (crop[2] + crop[3]))?;
    let vui_flag_pos = r.pos;
    let mut max_num_reorder = None;
    if r.u(1)? == 1 {
        if r.u(1)? == 1 && r.u(8)? == 255 {
            r.u(32)?;
        }
        if r.u(1)? == 1 {
            r.u(1)?;
        }
        if r.u(1)? == 1 {
            r.u(4)?;
            if r.u(1)? == 1 {
                r.u(24)?;
            }
        }
        if r.u(1)? == 1 {
            r.ue()?;
            r.ue()?;
        }
        if r.u(1)? == 1 {
            r.u(32)?;
            r.u(32)?;
            r.u(1)?;
        }
        let nal = r.u(1)? == 1;
        if nal {
            skip_hrd(&mut r)?;
        }
        let vcl = r.u(1)? == 1;
        if vcl {
            skip_hrd(&mut r)?;
        }
        if nal || vcl {
            r.u(1)?;
        }
        r.u(1)?;
        if r.u(1)? == 1 {
            r.u(1)?;
            for _ in 0..4 {
                r.ue()?;
            }
            max_num_reorder = Some(r.ue()?);
            r.ue()?;
        }
    }
    Some(SpsInfo { width, height, num_ref_frames, max_num_reorder, vui_flag_pos })
}

#[cfg(test)]
pub fn sps_info(nal: &[u8]) -> Option<SpsInfo> {
    parse(&unescape(nal.get(1..)?))
}

/// Returns the SPS with `max_num_reorder_frames = 0`, or `None` if it need not
/// (or cannot safely) be changed. An existing VUI without
/// bitstream_restriction is copied bit for bit.
pub fn sps_low_delay(nal: &[u8]) -> Option<Vec<u8>> {
    let rbsp = unescape(nal.get(1..)?);
    let info = parse(&rbsp)?;
    if info.max_num_reorder.is_some() {
        return None;
    }
    let mut r = Reader { d: &rbsp, pos: info.vui_flag_pos };
    let has_vui = r.u(1)? == 1;
    let mut w = Writer::default();
    if has_vui {
        // Copy the VUI up to bitstream_restriction_flag (the last VUI bit = 0).
        let mut tmp = Reader { d: &rbsp, pos: 0 };
        let mut end = info.vui_flag_pos + 1;
        // Second pass to find the position of bitstream_restriction_flag.
        tmp.pos = end;
        let flag = |t: &mut Reader| t.u(1).map(|v| v == 1);
        if flag(&mut tmp)? && tmp.u(8)? == 255 {
            tmp.u(32)?;
        }
        if flag(&mut tmp)? {
            tmp.u(1)?;
        }
        if flag(&mut tmp)? {
            tmp.u(4)?;
            if flag(&mut tmp)? {
                tmp.u(24)?;
            }
        }
        if flag(&mut tmp)? {
            tmp.ue()?;
            tmp.ue()?;
        }
        if flag(&mut tmp)? {
            tmp.u(65)?;
        }
        let nal_hrd = flag(&mut tmp)?;
        if nal_hrd {
            skip_hrd(&mut tmp)?;
        }
        let vcl_hrd = flag(&mut tmp)?;
        if vcl_hrd {
            skip_hrd(&mut tmp)?;
        }
        if nal_hrd || vcl_hrd {
            tmp.u(1)?;
        }
        tmp.u(1)?;
        end = tmp.pos;
        w.copy(&rbsp, end);
    } else {
        w.copy(&rbsp, info.vui_flag_pos);
        w.put(1, 1);
        // aspect, overscan, video_signal, chroma_loc, timing, nal_hrd, vcl_hrd, pic_struct
        w.put(8, 0);
    }
    w.put(1, 1); // bitstream_restriction_flag
    w.put(1, 1); // motion_vectors_over_pic_boundaries_flag
    w.ue(2); // max_bytes_per_pic_denom
    w.ue(1); // max_bits_per_mb_denom
    w.ue(15); // log2_max_mv_length_horizontal
    w.ue(15); // log2_max_mv_length_vertical
    w.ue(0); // max_num_reorder_frames
    w.ue(info.num_ref_frames.max(1)); // max_dec_frame_buffering
    let mut out = vec![nal[0]];
    out.extend(escape(&w.finish()));
    Some(out)
}

/// `true` for a B slice (slice_type 1 or 6).
pub fn is_b_slice(nal: &[u8]) -> bool {
    let t = nal.first().map(|b| b & 0x1F).unwrap_or(0);
    if t != 1 && t != 5 {
        return false;
    }
    let rbsp = unescape(&nal[1..nal.len().min(24)]);
    let mut r = Reader { d: &rbsp, pos: 0 };
    r.ue().and_then(|_| r.ue()).is_some_and(|st| st % 5 == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn parses_reolink_sps() {
        let sps = hex("67640033ac1514a0a02ff950");
        let info = sps_info(&sps).unwrap();
        assert_eq!((info.width, info.height, info.num_ref_frames, info.max_num_reorder), (640, 360, 1, None));
    }

    #[test]
    fn rewrites_sps_without_vui() {
        let sps = hex("67640033ac1514a0a02ff950");
        let out = sps_low_delay(&sps).unwrap();
        let info = sps_info(&out).unwrap();
        assert_eq!((info.width, info.height, info.num_ref_frames, info.max_num_reorder), (640, 360, 1, Some(0)));
        assert!(sps_low_delay(&out).is_none(), "a second rewrite is not needed");
    }

    #[test]
    fn rewrites_sps_with_vui_keeping_fields() {
        // Baseline 1280x720 with VUI: timing_info (fps) without bitstream_restriction.
        let mut w = Writer::default();
        w.put(8, 66);
        w.put(8, 0);
        w.put(8, 31);
        w.ue(0); // sps id
        w.ue(0); // log2_max_frame_num-4
        w.ue(2); // poc type 2
        w.ue(1); // num_ref_frames
        w.put(1, 0);
        w.ue(79); // 80 MB
        w.ue(44); // 45 MB
        w.put(1, 1); // frame_mbs_only
        w.put(1, 1); // direct8x8
        w.put(1, 0); // no crop
        w.put(1, 1); // vui
        w.put(4, 0); // aspect, overscan, video_signal, chroma_loc
        w.put(1, 1); // timing
        w.put(32, 1);
        w.put(32, 50);
        w.put(1, 1);
        w.put(4, 0); // nal hrd, vcl hrd, pic_struct, bitstream_restriction
        let mut nal = vec![0x67];
        nal.extend(escape(&w.finish()));
        let before = sps_info(&nal).unwrap();
        assert_eq!((before.width, before.height, before.max_num_reorder), (1280, 720, None));
        let out = sps_low_delay(&nal).unwrap();
        let after = sps_info(&out).unwrap();
        assert_eq!((after.width, after.height, after.max_num_reorder), (1280, 720, Some(0)));
        // timing_info fields copied unchanged
        assert_eq!(&unescape(&out[1..])[..12], &unescape(&nal[1..])[..12]);
    }

    #[test]
    fn emulation_prevention_roundtrip() {
        let raw = vec![0, 0, 0, 1, 0, 0, 3, 5, 0, 0];
        let esc = escape(&raw);
        assert_eq!(esc, vec![0, 0, 3, 0, 1, 0, 0, 3, 3, 5, 0, 0]);
        assert_eq!(unescape(&esc), raw);
    }

    #[test]
    fn detects_b_slices() {
        let mk = |slice_type: u32| {
            let mut w = Writer::default();
            w.ue(0);
            w.ue(slice_type);
            w.put(8, 0xFF);
            let mut v = vec![0x41];
            v.extend(w.finish());
            v
        };
        assert!(is_b_slice(&mk(1)));
        assert!(is_b_slice(&mk(6)));
        assert!(!is_b_slice(&mk(0)));
        assert!(!is_b_slice(&mk(5)));
        assert!(!is_b_slice(&[0x67, 0x42]));
    }
}
