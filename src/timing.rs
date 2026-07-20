//! Time-based scheduling primitives for the main event loop.
//!
//! Every method takes `now: Instant` as an argument instead of calling
//! `Instant::now()` internally, so all behavior is unit-testable with
//! synthetic clocks.

use std::time::{Duration, Instant};

/// A time-based hold that replaces the old frame-count cooldowns.
///
/// While active, rendering is suppressed; when it expires the caller is told
/// exactly once (via [`Cooldown::take_expired`]) so it can schedule a redraw.
#[derive(Debug, Default)]
pub struct Cooldown {
    until: Option<Instant>,
}

impl Cooldown {
    /// Arm the cooldown for `d` from `now`. Re-arming never shortens an
    /// existing deadline (max semantics — matches the old
    /// `resize_cooldown = max(resize_cooldown, 5)` behavior).
    pub fn arm(&mut self, now: Instant, d: Duration) {
        let new_until = now + d;
        self.until = Some(match self.until {
            Some(existing) if existing > new_until => existing,
            _ => new_until,
        });
    }

    pub fn is_active(&self, now: Instant) -> bool {
        self.until.is_some_and(|u| now < u)
    }

    /// Returns true exactly once when the deadline has passed, clearing it.
    pub fn take_expired(&mut self, now: Instant) -> bool {
        match self.until {
            Some(u) if now >= u => {
                self.until = None;
                true
            }
            _ => false,
        }
    }

    /// The pending deadline, if any (for recv_timeout scheduling).
    #[allow(dead_code)] // used by the event-driven loop (next commit)
    pub fn deadline(&self) -> Option<Instant> {
        self.until
    }
}

/// Caps the redraw rate at one frame per `min_interval` (frame pacing).
#[derive(Debug)]
#[allow(dead_code)] // used by the event-driven loop (next commit)
pub struct FramePacer {
    min_interval: Duration,
    next_frame: Option<Instant>,
}

#[allow(dead_code)] // used by the event-driven loop (next commit)
impl FramePacer {
    pub fn new(min_interval: Duration) -> Self {
        FramePacer {
            min_interval,
            next_frame: None,
        }
    }

    /// Whether enough time has passed since the last draw.
    pub fn ready(&self, now: Instant) -> bool {
        self.next_frame.is_none_or(|n| now >= n)
    }

    pub fn mark_drawn(&mut self, now: Instant) {
        self.next_frame = Some(now + self.min_interval);
    }

    /// When the loop should wake to draw a pending (dirty) frame.
    /// None when not dirty (nothing to schedule) or when a draw is
    /// already allowed (wake immediately via other means).
    pub fn deadline_if_dirty(&self, dirty: bool, now: Instant) -> Option<Instant> {
        if !dirty {
            return None;
        }
        match self.next_frame {
            Some(n) if now < n => Some(n),
            _ => Some(now),
        }
    }
}

/// Earliest of the given optional deadlines.
#[allow(dead_code)] // used by the event-driven loop (next commit)
pub fn earliest(deadlines: impl IntoIterator<Item = Option<Instant>>) -> Option<Instant> {
    deadlines.into_iter().flatten().min()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn test_cooldown_inactive_by_default() {
        let c = Cooldown::default();
        assert!(!c.is_active(t0()));
        assert_eq!(c.deadline(), None);
    }

    #[test]
    fn test_cooldown_arm_activate_expire() {
        let now = t0();
        let mut c = Cooldown::default();
        c.arm(now, 150 * MS);
        assert!(c.is_active(now));
        assert!(c.is_active(now + 149 * MS));
        assert!(!c.is_active(now + 150 * MS));
        assert!(!c.take_expired(now + 149 * MS));
        assert!(c.take_expired(now + 150 * MS));
        // exactly once
        assert!(!c.take_expired(now + 151 * MS));
        assert!(!c.is_active(now + 151 * MS));
    }

    #[test]
    fn test_cooldown_rearm_never_shortens() {
        let now = t0();
        let mut c = Cooldown::default();
        c.arm(now, 200 * MS);
        c.arm(now + 10 * MS, 50 * MS); // would expire earlier — must not shorten
        assert_eq!(c.deadline(), Some(now + 200 * MS));
        // but a later deadline extends
        c.arm(now + 100 * MS, 200 * MS);
        assert_eq!(c.deadline(), Some(now + 300 * MS));
    }

    #[test]
    fn test_pacer_first_frame_immediate() {
        let now = t0();
        let p = FramePacer::new(16 * MS);
        assert!(p.ready(now));
    }

    #[test]
    fn test_pacer_16ms_boundary() {
        let now = t0();
        let mut p = FramePacer::new(16 * MS);
        p.mark_drawn(now);
        assert!(!p.ready(now + 15 * MS));
        assert!(p.ready(now + 16 * MS));
    }

    #[test]
    fn test_pacer_deadline_only_when_dirty() {
        let now = t0();
        let mut p = FramePacer::new(16 * MS);
        p.mark_drawn(now);
        assert_eq!(p.deadline_if_dirty(false, now + 5 * MS), None);
        assert_eq!(p.deadline_if_dirty(true, now + 5 * MS), Some(now + 16 * MS));
        // past the interval: wake immediately
        assert_eq!(p.deadline_if_dirty(true, now + 20 * MS), Some(now + 20 * MS));
    }

    #[test]
    fn test_earliest_mixed_none() {
        let now = t0();
        assert_eq!(earliest([None, None]), None);
        assert_eq!(
            earliest([None, Some(now + 5 * MS), Some(now + 3 * MS), None]),
            Some(now + 3 * MS)
        );
    }
}
