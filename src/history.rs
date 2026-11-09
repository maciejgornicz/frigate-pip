//! The last seconds of a video stream as received (compressed, in whole GOPs): the
//! prebuffer's start for new windows and the material of a replay.

use crate::rtp::{AccessUnit, VideoTrack};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What a window keeps of its own video (without the prebuffer): minutes of a sub stream,
/// about half a minute of a 4K main stream.
const WINDOW_BYTES: usize = 32 << 20;

pub struct History {
    gops: VecDeque<Vec<Arc<AccessUnit>>>,
    bytes: usize,
    /// Reach back at least this far (older whole GOPs fall out); `None` = bytes only.
    max_age: Option<Duration>,
    max_bytes: usize,
}

impl History {
    pub fn new(max_age: Option<Duration>, max_bytes: usize) -> History {
        History { gops: VecDeque::new(), bytes: 0, max_age, max_bytes }
    }

    /// A received unit; those before the first key frame cannot be decoded and are dropped.
    pub fn push(&mut self, au: Arc<AccessUnit>) {
        if au.key {
            self.gops.push_back(Vec::new());
        }
        let Some(gop) = self.gops.back_mut() else { return };
        self.bytes += au.data.len();
        let newest = au.arrived;
        gop.push(au);
        self.trim(newest);
    }

    fn trim(&mut self, newest: Instant) {
        // The oldest GOP goes once the next one alone reaches back far enough.
        if let Some(age) = self.max_age {
            while self.gops.len() > 1 && self.gops[1][0].arrived + age <= newest {
                self.drop_oldest();
            }
        }
        while self.bytes > self.max_bytes && self.gops.len() > 1 {
            self.drop_oldest();
        }
        // One GOP over the limit: the stream sends no key frames; start again at the next one.
        if self.bytes > self.max_bytes {
            self.clear();
        }
    }

    fn drop_oldest(&mut self) {
        if let Some(gop) = self.gops.pop_front() {
            self.bytes -= gop.iter().map(|au| au.data.len()).sum::<usize>();
        }
    }

    pub fn clear(&mut self) {
        self.gops.clear();
        self.bytes = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.gops.is_empty()
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The current GOP: what a new window decodes first.
    pub fn last_gop(&self) -> &[Arc<AccessUnit>] {
        self.gops.back().map_or(&[], |g| g.as_slice())
    }

    /// Everything from the last key frame at or before `at` (from the oldest one when `at` is older).
    pub fn from(&self, at: Instant) -> Vec<Arc<AccessUnit>> {
        let start = self.gops.iter().rposition(|g| g[0].arrived <= at).unwrap_or(0);
        self.gops.iter().skip(start).flatten().cloned().collect()
    }
}

/// What a window recorded itself, with the track it belongs to.
pub struct Recorder {
    track: Option<VideoTrack>,
    history: History,
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder { track: None, history: History::new(None, WINDOW_BYTES) }
    }
}

impl Recorder {
    /// The stream's track: what was kept of another track goes.
    pub fn track(&mut self, track: &VideoTrack) {
        if self.track.as_ref() != Some(track) {
            self.history.clear();
            self.track = Some(track.clone());
        }
    }

    pub fn push(&mut self, au: Arc<AccessUnit>) {
        if self.track.is_some() {
            self.history.push(au);
        }
    }

    /// Everything kept, with its track; `None` when nothing is.
    pub fn replay(&self) -> Option<(VideoTrack, Vec<Arc<AccessUnit>>)> {
        let first = self.history.gops.front()?.first()?.arrived;
        Some((self.track.clone()?, self.history.from(first)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtp::Codec;

    fn unit(key: bool, at: Instant, bytes: usize) -> Arc<AccessUnit> {
        Arc::new(AccessUnit { data: vec![0; bytes], key, arrived: at, late: Duration::ZERO, ts: 0 })
    }

    fn s(v: u64) -> Duration {
        Duration::from_secs(v)
    }

    #[test]
    fn units_before_the_first_key_frame_are_dropped() {
        let t0 = Instant::now();
        let mut h = History::new(None, 1000);
        h.push(unit(false, t0, 10));
        assert!(h.is_empty());
        h.push(unit(true, t0, 10));
        assert_eq!(h.last_gop().len(), 1);
    }

    #[test]
    fn the_last_gop_starts_at_the_newest_key_frame() {
        let t0 = Instant::now();
        let mut h = History::new(None, 1000);
        for (i, key) in [true, false, false, true, false].into_iter().enumerate() {
            h.push(unit(key, t0 + s(i as u64), 10));
        }
        assert_eq!(h.last_gop().len(), 2);
        assert!(h.last_gop()[0].key);
    }

    #[test]
    fn old_gops_fall_out_but_the_age_is_still_covered() {
        let t0 = Instant::now();
        let mut h = History::new(Some(s(10)), 1 << 20);
        for k in (0..=28).step_by(4) {
            h.push(unit(true, t0 + s(k), 10));
            h.push(unit(false, t0 + s(k + 2), 10));
        }
        // The newest unit came at 30 s: the oldest GOP kept is the last one starting at or before 20 s.
        assert_eq!(h.from(t0)[0].arrived, t0 + s(20));
    }

    #[test]
    fn the_byte_limit_drops_whole_gops() {
        let t0 = Instant::now();
        let mut h = History::new(None, 100);
        for g in 0..4u64 {
            h.push(unit(true, t0 + s(g * 2), 30));
            h.push(unit(false, t0 + s(g * 2 + 1), 10));
        }
        assert_eq!(h.bytes(), 80);
        assert_eq!(h.from(t0)[0].arrived, t0 + s(4));
    }

    #[test]
    fn one_gop_over_the_limit_starts_over() {
        let t0 = Instant::now();
        let mut h = History::new(None, 100);
        h.push(unit(true, t0, 60));
        h.push(unit(false, t0 + s(1), 50));
        assert!(h.is_empty());
        h.push(unit(false, t0 + s(2), 10));
        assert!(h.is_empty());
        h.push(unit(true, t0 + s(3), 10));
        assert_eq!(h.bytes(), 10);
    }

    #[test]
    fn a_replay_starts_at_the_key_frame_before_the_moment() {
        let t0 = Instant::now();
        let mut h = History::new(None, 1000);
        for k in [10, 14, 18] {
            h.push(unit(true, t0 + s(k), 10));
            h.push(unit(false, t0 + s(k + 1), 10));
        }
        assert_eq!(h.from(t0 + s(15))[0].arrived, t0 + s(14));
        assert_eq!(h.from(t0 + s(14))[0].arrived, t0 + s(14));
        // Older than everything kept: from the oldest key frame.
        assert_eq!(h.from(t0)[0].arrived, t0 + s(10));
        assert_eq!(h.from(t0).len(), 6);
    }

    #[test]
    fn a_new_track_forgets_what_was_kept() {
        let t0 = Instant::now();
        let track = |pt| VideoTrack { codec: Codec::H264, payload_type: pt, control: String::new(), params: Vec::new() };
        let mut r = Recorder::default();
        r.push(unit(true, t0, 10));
        assert!(r.replay().is_none(), "nothing is kept without a track");
        r.track(&track(96));
        r.push(unit(true, t0, 10));
        r.track(&track(96));
        assert_eq!(r.replay().unwrap().1.len(), 1, "the same track keeps it");
        r.track(&track(97));
        assert!(r.replay().is_none());
    }
}
