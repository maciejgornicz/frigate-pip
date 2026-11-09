//! Replay of the event start: what the Windows and Linux players share.

use crate::rtp::AccessUnit;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// With the prebuffer a replay starts this long before the window opened.
pub const PRE_ROLL: Duration = Duration::from_secs(5);
/// Longest pause between two replayed frames: a jump of the timestamps (a new base, a lost
/// stretch) must not stall the replay.
const MAX_GAP: Duration = Duration::from_secs(1);

/// The pause before the frame with RTP timestamp `next` after the one with `prev` (90 kHz).
pub fn gap(prev: u32, next: u32) -> Duration {
    let ticks = next.wrapping_sub(prev) as i32;
    if ticks <= 0 {
        return Duration::ZERO;
    }
    Duration::from_secs_f64(ticks as f64 / 90_000.0).min(MAX_GAP)
}

/// Where a replay from the prebuffer starts.
pub fn start(event: Instant) -> Instant {
    event.checked_sub(PRE_ROLL).unwrap_or(event)
}

/// The caption during a replay: how far behind live the picture is, then the usual text.
pub fn caption(word: &str, behind: Duration, text: &str) -> String {
    let label = format!("{word} −{} s", behind.as_secs_f32().round() as u64);
    if text.is_empty() {
        label
    } else {
        format!("{label} · {text}")
    }
}

/// For the log: how far back a replay reaches and how much it holds.
pub fn describe(units: &[Arc<AccessUnit>], now: Instant) -> String {
    let back = units.first().map_or(Duration::ZERO, |u| now.saturating_duration_since(u.arrived));
    let bytes: usize = units.iter().map(|u| u.data.len()).sum();
    format!("{:.1} s back, {} frames, {} KB", back.as_secs_f32(), units.len(), bytes / 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pause_follows_the_timestamps() {
        assert_eq!(gap(0, 9000), Duration::from_millis(100));
    }

    #[test]
    fn the_pause_survives_the_wrap() {
        assert_eq!(gap(u32::MAX - 4499, 4500), Duration::from_millis(100));
    }

    #[test]
    fn going_back_is_no_pause() {
        assert_eq!(gap(9000, 0), Duration::ZERO);
    }

    #[test]
    fn a_jump_is_cut_to_a_second() {
        assert_eq!(gap(0, 900_000), Duration::from_secs(1));
    }

    #[test]
    fn a_replay_starts_five_seconds_before_the_event() {
        let now = Instant::now() + Duration::from_secs(60);
        assert_eq!(start(now), now - Duration::from_secs(5));
    }

    #[test]
    fn the_caption_says_how_far_behind_live() {
        assert_eq!(caption("Replay", Duration::from_millis(11_600), "Front door · person"), "Replay −12 s · Front door · person");
        assert_eq!(caption("Replay", Duration::from_secs(3), ""), "Replay −3 s");
    }

    #[test]
    fn the_log_describes_the_material() {
        let now = Instant::now() + Duration::from_secs(60);
        let unit = |back_ms: u64, bytes| {
            Arc::new(AccessUnit { data: vec![0; bytes], key: true, arrived: now - Duration::from_millis(back_ms), late: Duration::ZERO, ts: 0 })
        };
        assert_eq!(describe(&[unit(12_300, 1024), unit(12_200, 1024)], now), "12.3 s back, 2 frames, 2 KB");
    }
}
