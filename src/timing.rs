//! How late the pictures of a window are: one line in the log per window, so that a delayed
//! picture can be traced to the program, the network or the stream itself. Without a clock
//! shared by the cameras (go2rtc sends no RTCP sender reports) this cannot tell how far a
//! camera is behind reality, only how far a frame is behind its own stream.

use std::time::{Duration, Instant};

/// RTP clock of H.264 and H.265 video.
const CLOCK: f64 = 90_000.0;
/// How long the best arrival counts: RTP clocks of cameras run a little off, so older best
/// moments would add up to lateness that is not there.
const MEMORY: Duration = Duration::from_secs(30);
/// A bigger difference (seconds) is a new timestamp base (the stream restarted), not lateness.
const NEW_BASE: f64 = 30.0;

/// How much later a frame arrives than the RTP timestamps of its stream promise, compared
/// with the best moment of the last 30 to 60 seconds.
#[derive(Default)]
pub struct Lateness {
    state: Option<LatenessState>,
}

struct LatenessState {
    start: Instant,
    last_ts: u32,
    /// Seconds of stream since `start`, from the timestamps.
    stream: f64,
    /// Smallest `arrival - stream` since `since`, and in the period before it.
    best: f64,
    earlier: f64,
    since: Instant,
}

impl LatenessState {
    fn new(ts: u32, at: Instant) -> Self {
        LatenessState { start: at, last_ts: ts, stream: 0.0, best: 0.0, earlier: 0.0, since: at }
    }
}

impl Lateness {
    /// `ts` = RTP timestamp of the frame, `at` = when it arrived.
    pub fn frame(&mut self, ts: u32, at: Instant) -> Duration {
        let Some(s) = &mut self.state else {
            self.state = Some(LatenessState::new(ts, at));
            return Duration::ZERO;
        };
        s.stream += ts.wrapping_sub(s.last_ts) as i32 as f64 / CLOCK;
        s.last_ts = ts;
        let behind = at.saturating_duration_since(s.start).as_secs_f64() - s.stream;
        if (behind - s.best.min(s.earlier)).abs() > NEW_BASE {
            *s = LatenessState::new(ts, at);
            return Duration::ZERO;
        }
        if at.saturating_duration_since(s.since) >= MEMORY {
            s.earlier = s.best;
            s.best = behind;
            s.since = at;
        }
        s.best = s.best.min(behind);
        Duration::from_secs_f64((behind - s.best.min(s.earlier)).max(0.0))
    }
}

/// The timing of one window's video.
#[derive(Default)]
pub struct Timing {
    frames: u32,
    previous: Option<Instant>,
    longest_gap: Duration,
    worst_late: Duration,
    shown: u32,
    picture_total: Duration,
    picture_max: Duration,
}

impl Timing {
    /// A frame for this window, received at `arrived`, `late` behind its stream.
    pub fn received(&mut self, arrived: Instant, late: Duration) {
        self.frames += 1;
        if let Some(p) = self.previous {
            self.longest_gap = self.longest_gap.max(arrived.saturating_duration_since(p));
        }
        self.previous = Some(arrived);
        self.worst_late = self.worst_late.max(late);
    }

    /// The picture of the frame received at `arrived` is ready at `now`.
    pub fn shown(&mut self, arrived: Instant, now: Instant) {
        let took = now.saturating_duration_since(arrived);
        self.shown += 1;
        self.picture_total += took;
        self.picture_max = self.picture_max.max(took);
    }

    /// The line for the log; `None` when no video came.
    pub fn summary(&self) -> Option<String> {
        if self.frames == 0 {
            return None;
        }
        let picture = match self.shown {
            0 => "never shown".to_string(),
            n => format!("picture in {} ms (max {})", (self.picture_total / n).as_millis(), self.picture_max.as_millis()),
        };
        Some(format!(
            "{} frames, {picture}, late up to {} ms, longest gap {} ms",
            self.frames,
            self.worst_late.as_millis(),
            self.longest_gap.as_millis()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 ms at the 90 kHz clock of RTP video.
    const FRAME: u32 = 9000;

    fn ms(v: u64) -> Duration {
        Duration::from_millis(v)
    }

    fn near(d: Duration, want: u64) -> bool {
        d.abs_diff(ms(want)) < ms(1)
    }

    #[test]
    fn a_steady_stream_is_on_time() {
        let t0 = Instant::now();
        let mut l = Lateness::default();
        for i in 0..50u32 {
            assert!(near(l.frame(i * FRAME, t0 + ms(i as u64 * 100)), 0));
        }
    }

    #[test]
    fn a_late_frame_is_reported_and_the_next_one_is_on_time_again() {
        let t0 = Instant::now();
        let mut l = Lateness::default();
        for i in 0..10u32 {
            l.frame(i * FRAME, t0 + ms(i as u64 * 100));
        }
        assert!(near(l.frame(10 * FRAME, t0 + ms(1300)), 300));
        assert!(near(l.frame(11 * FRAME, t0 + ms(1100)), 0));
    }

    #[test]
    fn frames_held_back_by_a_stall_show_how_far_they_are_behind() {
        let t0 = Instant::now();
        let mut l = Lateness::default();
        for i in 0..5u32 {
            l.frame(i * FRAME, t0 + ms(i as u64 * 100));
        }
        assert!(near(l.frame(5 * FRAME, t0 + ms(1000)), 500));
        assert!(near(l.frame(6 * FRAME, t0 + ms(1000)), 400));
        assert!(near(l.frame(7 * FRAME, t0 + ms(1000)), 300));
    }

    #[test]
    fn the_timestamp_may_wrap_around() {
        let t0 = Instant::now();
        let mut l = Lateness::default();
        let start = u32::MAX - 3 * FRAME;
        for i in 0..4u32 {
            assert!(near(l.frame(start.wrapping_add(i * FRAME), t0 + ms(i as u64 * 100)), 0));
        }
        // The measurement goes on across the wrap: this frame is still seen to be late.
        assert!(near(l.frame(start.wrapping_add(4 * FRAME), t0 + ms(700)), 300));
    }

    #[test]
    fn a_new_timestamp_base_starts_the_measurement_over() {
        let t0 = Instant::now();
        let mut l = Lateness::default();
        let start = 900_000_000;
        for i in 0..10u32 {
            l.frame(start + i * FRAME, t0 + ms(i as u64 * 100));
        }
        assert!(near(l.frame(0, t0 + ms(1000)), 0));
        assert!(near(l.frame(FRAME, t0 + ms(1100)), 0));
    }

    #[test]
    fn a_slow_rtp_clock_does_not_add_up() {
        let t0 = Instant::now();
        let mut l = Lateness::default();
        let mut last = Duration::ZERO;
        // 0.2 % slow, as measured on real cameras: 1.2 s after these 10 minutes if it added up.
        for i in 0..6000u32 {
            last = l.frame((i as f64 * FRAME as f64 * 0.998) as u32, t0 + ms(i as u64 * 100));
        }
        assert!(last < ms(200), "{last:?}");
    }

    #[test]
    fn the_summary_keeps_the_worst_moments() {
        let t0 = Instant::now();
        let mut t = Timing::default();
        t.received(t0, ms(0));
        t.received(t0 + ms(100), ms(20));
        t.received(t0 + ms(500), ms(90));
        t.received(t0 + ms(600), ms(10));
        t.shown(t0 + ms(100), t0 + ms(112));
        t.shown(t0 + ms(600), t0 + ms(640));
        assert_eq!(t.summary().unwrap(), "4 frames, picture in 26 ms (max 40), late up to 90 ms, longest gap 400 ms");
    }

    #[test]
    fn frames_never_shown_are_said_to_be() {
        let t0 = Instant::now();
        let mut t = Timing::default();
        t.received(t0, ms(5));
        assert_eq!(t.summary().unwrap(), "1 frames, never shown, late up to 5 ms, longest gap 0 ms");
    }

    #[test]
    fn without_video_there_is_no_line() {
        assert_eq!(Timing::default().summary(), None);
    }
}
