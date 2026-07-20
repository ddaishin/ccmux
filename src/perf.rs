//! Frame-timing instrumentation, enabled by setting `CCMUX_PERF_LOG=<path>`.
//!
//! Writes one text line per frame plus a per-second summary:
//!
//! ```text
//! frame t=12.345 draw_us=1234 latency_us=2100 pty_ev=17 inp_ev=1
//! sec   t=13.000 fps=42 draw_avg_us=1100 draw_max_us=3400 pty_ev=310 inp_ev=12
//! ```
//!
//! `latency_us` measures the time from the first unserviced event arrival
//! (stamped in the reader/input threads) to the start of the draw that
//! serviced it — the end-to-end "event to screen" delay the event loop adds.
//!
//! When `CCMUX_PERF_LOG` is unset the only cost is one relaxed atomic load
//! per event and an `Option` check per frame.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Microseconds (since [`epoch`]) of the first event arrival not yet
/// serviced by a draw. 0 = none pending. Written by reader/input threads.
static FIRST_EVENT_US: AtomicU64 = AtomicU64::new(0);
static PTY_EVENTS: AtomicU32 = AtomicU32::new(0);
static INPUT_EVENTS: AtomicU32 = AtomicU32::new(0);

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_us() -> u64 {
    // +1 so that a stamp taken exactly at the epoch is never confused
    // with the 0 = "none pending" sentinel.
    epoch().elapsed().as_micros() as u64 + 1
}

/// Called from PTY reader threads just before sending an event.
pub fn mark_pty_event() {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    PTY_EVENTS.fetch_add(1, Ordering::Relaxed);
    let _ = FIRST_EVENT_US.compare_exchange(0, now_us(), Ordering::Relaxed, Ordering::Relaxed);
}

/// Called when a user input event (key/mouse/paste/resize) is received.
pub fn mark_input_event() {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    INPUT_EVENTS.fetch_add(1, Ordering::Relaxed);
    let _ = FIRST_EVENT_US.compare_exchange(0, now_us(), Ordering::Relaxed, Ordering::Relaxed);
}

pub struct PerfLog {
    out: BufWriter<File>,
    sec_start: Instant,
    frames_this_sec: u32,
    sec_draw_us_sum: u64,
    sec_draw_us_max: u64,
    sec_pty_events: u32,
    sec_input_events: u32,
}

impl PerfLog {
    /// Returns `Some` only when `CCMUX_PERF_LOG` names a creatable file.
    pub fn from_env() -> Option<PerfLog> {
        let path = std::env::var("CCMUX_PERF_LOG").ok()?;
        let file = File::create(path).ok()?;
        epoch(); // pin the epoch before any event can be stamped
        ENABLED.store(true, Ordering::Relaxed);
        Some(PerfLog {
            out: BufWriter::with_capacity(64 * 1024, file),
            sec_start: Instant::now(),
            frames_this_sec: 0,
            sec_draw_us_sum: 0,
            sec_draw_us_max: 0,
            sec_pty_events: 0,
            sec_input_events: 0,
        })
    }

    /// Record a completed draw. `draw_start` is when `terminal.draw` began,
    /// `draw_dur` how long it took.
    pub fn on_frame(&mut self, draw_start: Instant, draw_dur: Duration) {
        let draw_us = draw_dur.as_micros() as u64;
        let first_us = FIRST_EVENT_US.swap(0, Ordering::Relaxed);
        let start_us = draw_start.duration_since(epoch()).as_micros() as u64 + 1;
        let latency_us = if first_us > 0 {
            start_us.saturating_sub(first_us)
        } else {
            0 // draw triggered by a timer (cooldown expiry etc.), not an event
        };
        let pty_ev = PTY_EVENTS.swap(0, Ordering::Relaxed);
        let inp_ev = INPUT_EVENTS.swap(0, Ordering::Relaxed);

        let t = draw_start.duration_since(epoch());
        let _ = writeln!(
            self.out,
            "{}",
            format_frame_line(t, draw_us, latency_us, pty_ev, inp_ev)
        );

        self.frames_this_sec += 1;
        self.sec_draw_us_sum += draw_us;
        self.sec_draw_us_max = self.sec_draw_us_max.max(draw_us);
        self.sec_pty_events += pty_ev;
        self.sec_input_events += inp_ev;

        let now = draw_start + draw_dur;
        if now.duration_since(self.sec_start) >= Duration::from_secs(1) {
            let _ = writeln!(
                self.out,
                "{}",
                format_sec_line(
                    now.duration_since(epoch()),
                    self.frames_this_sec,
                    self.sec_draw_us_sum,
                    self.sec_draw_us_max,
                    self.sec_pty_events,
                    self.sec_input_events,
                )
            );
            let _ = self.out.flush();
            self.sec_start = now;
            self.frames_this_sec = 0;
            self.sec_draw_us_sum = 0;
            self.sec_draw_us_max = 0;
            self.sec_pty_events = 0;
            self.sec_input_events = 0;
        }
    }
}

impl Drop for PerfLog {
    fn drop(&mut self) {
        ENABLED.store(false, Ordering::Relaxed);
        let _ = self.out.flush();
    }
}

fn format_frame_line(t: Duration, draw_us: u64, latency_us: u64, pty_ev: u32, inp_ev: u32) -> String {
    format!(
        "frame t={:.3} draw_us={} latency_us={} pty_ev={} inp_ev={}",
        t.as_secs_f64(),
        draw_us,
        latency_us,
        pty_ev,
        inp_ev
    )
}

fn format_sec_line(
    t: Duration,
    frames: u32,
    draw_us_sum: u64,
    draw_us_max: u64,
    pty_ev: u32,
    inp_ev: u32,
) -> String {
    let avg = if frames > 0 { draw_us_sum / frames as u64 } else { 0 };
    format!(
        "sec   t={:.3} fps={} draw_avg_us={} draw_max_us={} pty_ev={} inp_ev={}",
        t.as_secs_f64(),
        frames,
        avg,
        draw_us_max,
        pty_ev,
        inp_ev
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_frame_line() {
        let line = format_frame_line(Duration::from_micros(12_345_678), 1234, 2100, 17, 1);
        assert_eq!(line, "frame t=12.346 draw_us=1234 latency_us=2100 pty_ev=17 inp_ev=1");
    }

    #[test]
    fn test_format_sec_line_avg() {
        let line = format_sec_line(Duration::from_secs(13), 42, 46_200, 3400, 310, 12);
        assert_eq!(line, "sec   t=13.000 fps=42 draw_avg_us=1100 draw_max_us=3400 pty_ev=310 inp_ev=12");
    }

    #[test]
    fn test_format_sec_line_zero_frames_no_div_by_zero() {
        let line = format_sec_line(Duration::from_secs(1), 0, 0, 0, 0, 0);
        assert_eq!(line, "sec   t=1.000 fps=0 draw_avg_us=0 draw_max_us=0 pty_ev=0 inp_ev=0");
    }

    #[test]
    fn test_marks_are_noops_when_disabled() {
        // ENABLED is false in tests (from_env not called without the env var)
        mark_pty_event();
        mark_input_event();
        assert_eq!(FIRST_EVENT_US.load(Ordering::Relaxed), 0);
        assert_eq!(PTY_EVENTS.load(Ordering::Relaxed), 0);
    }
}
