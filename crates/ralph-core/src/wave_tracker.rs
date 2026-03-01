//! Wave tracking state machine for concurrent hat execution.
//!
//! Tracks active waves, records results and failures, manages timeouts,
//! and determines when aggregation gates should open.

use ralph_proto::{Event, HatId};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Central state machine for tracking active waves.
pub struct WaveTracker {
    active_waves: HashMap<String, WaveState>,
}

/// State of a single active wave.
pub struct WaveState {
    pub wave_id: String,
    pub expected_total: u32,
    pub worker_hat: HatId,
    pub results: Vec<WaveResult>,
    pub failures: Vec<WaveFailure>,
    pub started_at: Instant,
    pub timeout: Duration,
}

/// A successful result from a wave instance.
pub struct WaveResult {
    pub index: u32,
    pub events: Vec<Event>,
}

/// A failure from a wave instance.
pub struct WaveFailure {
    pub index: u32,
    pub error: String,
    pub duration: Duration,
}

/// A completed wave with all results and failures.
pub struct CompletedWave {
    pub wave_id: String,
    pub results: Vec<WaveResult>,
    pub failures: Vec<WaveFailure>,
    pub timed_out: bool,
    pub duration: Duration,
}

/// Progress indicator returned by `record_result`.
#[derive(Debug, PartialEq, Eq)]
pub enum WaveProgress {
    /// More results expected.
    InProgress { received: u32, expected: u32 },
    /// All results received, wave complete.
    Complete,
}

impl WaveTracker {
    /// Creates a new empty wave tracker.
    pub fn new() -> Self {
        Self {
            active_waves: HashMap::new(),
        }
    }

    /// Register a new wave.
    pub fn register_wave(
        &mut self,
        wave_id: String,
        expected_total: u32,
        worker_hat: HatId,
        timeout: Duration,
    ) {
        let state = WaveState {
            wave_id: wave_id.clone(),
            expected_total,
            worker_hat,
            results: Vec::new(),
            failures: Vec::new(),
            started_at: Instant::now(),
            timeout,
        };
        self.active_waves.insert(wave_id, state);
    }

    /// Record result events for a wave instance.
    /// Returns the wave progress after recording.
    pub fn record_result(
        &mut self,
        wave_id: &str,
        index: u32,
        events: Vec<Event>,
    ) -> WaveProgress {
        let Some(state) = self.active_waves.get_mut(wave_id) else {
            return WaveProgress::InProgress {
                received: 0,
                expected: 0,
            };
        };
        state.results.push(WaveResult { index, events });
        let received = state.results.len() as u32 + state.failures.len() as u32;
        if received >= state.expected_total {
            WaveProgress::Complete
        } else {
            WaveProgress::InProgress {
                received,
                expected: state.expected_total,
            }
        }
    }

    /// Record a failure for a wave instance.
    pub fn record_failure(
        &mut self,
        wave_id: &str,
        index: u32,
        error: String,
        duration: Duration,
    ) {
        if let Some(state) = self.active_waves.get_mut(wave_id) {
            state.failures.push(WaveFailure {
                index,
                error,
                duration,
            });
        }
    }

    /// Check if a wave is complete (all results + failures == expected total).
    pub fn is_complete(&self, wave_id: &str) -> bool {
        self.active_waves.get(wave_id).is_some_and(|state| {
            (state.results.len() + state.failures.len()) as u32 >= state.expected_total
        })
    }

    /// Check for timed-out waves. Returns wave IDs that have timed out.
    pub fn check_timeouts(&self) -> Vec<String> {
        self.active_waves
            .values()
            .filter(|state| state.started_at.elapsed() >= state.timeout)
            .map(|state| state.wave_id.clone())
            .collect()
    }

    /// Consume a completed wave, removing it from tracking.
    pub fn take_wave_results(&mut self, wave_id: &str) -> Option<CompletedWave> {
        let state = self.active_waves.remove(wave_id)?;
        Some(CompletedWave {
            wave_id: state.wave_id,
            results: state.results,
            failures: state.failures,
            timed_out: state.started_at.elapsed() >= state.timeout,
            duration: state.started_at.elapsed(),
        })
    }

    /// Check if any wave is currently active.
    pub fn has_active_waves(&self) -> bool {
        !self.active_waves.is_empty()
    }
}

impl Default for WaveTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result_event(topic: &str, payload: &str) -> Event {
        Event::new(topic, payload)
    }

    #[test]
    fn test_register_and_record_results_until_complete() {
        let mut tracker = WaveTracker::new();
        tracker.register_wave(
            "w-abc".into(),
            3,
            HatId::new("worker"),
            Duration::from_secs(300),
        );

        assert!(tracker.has_active_waves());
        assert!(!tracker.is_complete("w-abc"));

        // Record first result
        let progress = tracker.record_result(
            "w-abc",
            0,
            vec![make_result_event("review.done", "result 0")],
        );
        assert_eq!(
            progress,
            WaveProgress::InProgress {
                received: 1,
                expected: 3
            }
        );
        assert!(!tracker.is_complete("w-abc"));

        // Record second result
        let progress = tracker.record_result(
            "w-abc",
            1,
            vec![make_result_event("review.done", "result 1")],
        );
        assert_eq!(
            progress,
            WaveProgress::InProgress {
                received: 2,
                expected: 3
            }
        );

        // Record third result — should be complete
        let progress = tracker.record_result(
            "w-abc",
            2,
            vec![make_result_event("review.done", "result 2")],
        );
        assert_eq!(progress, WaveProgress::Complete);
        assert!(tracker.is_complete("w-abc"));
    }

    #[test]
    fn test_record_results_and_failure_completes_wave() {
        let mut tracker = WaveTracker::new();
        tracker.register_wave(
            "w-def".into(),
            3,
            HatId::new("worker"),
            Duration::from_secs(300),
        );

        // Two successes
        tracker.record_result(
            "w-def",
            0,
            vec![make_result_event("review.done", "ok 0")],
        );
        tracker.record_result(
            "w-def",
            1,
            vec![make_result_event("review.done", "ok 1")],
        );

        assert!(!tracker.is_complete("w-def"));

        // One failure — should complete the wave (2 results + 1 failure = 3 total)
        tracker.record_failure("w-def", 2, "backend crashed".into(), Duration::from_secs(5));

        assert!(tracker.is_complete("w-def"));
    }

    #[test]
    fn test_timeout_detection() {
        let mut tracker = WaveTracker::new();
        // Register with zero timeout so it's immediately timed out
        tracker.register_wave(
            "w-timeout".into(),
            5,
            HatId::new("worker"),
            Duration::from_nanos(0),
        );

        let timed_out = tracker.check_timeouts();
        assert_eq!(timed_out, vec!["w-timeout".to_string()]);
    }

    #[test]
    fn test_no_timeout_when_within_duration() {
        let mut tracker = WaveTracker::new();
        tracker.register_wave(
            "w-ok".into(),
            3,
            HatId::new("worker"),
            Duration::from_secs(300),
        );

        let timed_out = tracker.check_timeouts();
        assert!(timed_out.is_empty());
    }

    #[test]
    fn test_take_wave_results_returns_all_and_removes() {
        let mut tracker = WaveTracker::new();
        tracker.register_wave(
            "w-take".into(),
            3,
            HatId::new("worker"),
            Duration::from_secs(300),
        );

        tracker.record_result(
            "w-take",
            0,
            vec![make_result_event("review.done", "r0")],
        );
        tracker.record_result(
            "w-take",
            1,
            vec![make_result_event("review.done", "r1")],
        );
        tracker.record_failure("w-take", 2, "failed".into(), Duration::from_secs(3));

        let completed = tracker.take_wave_results("w-take").unwrap();
        assert_eq!(completed.wave_id, "w-take");
        assert_eq!(completed.results.len(), 2);
        assert_eq!(completed.failures.len(), 1);
        assert_eq!(completed.failures[0].index, 2);
        assert_eq!(completed.failures[0].error, "failed");
        assert!(!completed.timed_out);

        // Wave should be removed
        assert!(!tracker.has_active_waves());
        assert!(tracker.take_wave_results("w-take").is_none());
    }

    #[test]
    fn test_multiple_concurrent_waves_tracked_independently() {
        let mut tracker = WaveTracker::new();
        tracker.register_wave(
            "w-1".into(),
            2,
            HatId::new("reviewer"),
            Duration::from_secs(300),
        );
        tracker.register_wave(
            "w-2".into(),
            3,
            HatId::new("analyzer"),
            Duration::from_secs(600),
        );

        assert!(tracker.has_active_waves());

        // Complete wave 1
        tracker.record_result("w-1", 0, vec![make_result_event("done", "a")]);
        tracker.record_result("w-1", 1, vec![make_result_event("done", "b")]);
        assert!(tracker.is_complete("w-1"));
        assert!(!tracker.is_complete("w-2"));

        // Take wave 1 results
        let w1 = tracker.take_wave_results("w-1").unwrap();
        assert_eq!(w1.results.len(), 2);

        // Wave 2 still active
        assert!(tracker.has_active_waves());
        assert!(!tracker.is_complete("w-2"));

        // Complete wave 2
        tracker.record_result("w-2", 0, vec![make_result_event("done", "x")]);
        tracker.record_failure("w-2", 1, "error".into(), Duration::from_secs(1));
        tracker.record_result("w-2", 2, vec![make_result_event("done", "z")]);

        assert!(tracker.is_complete("w-2"));
        let w2 = tracker.take_wave_results("w-2").unwrap();
        assert_eq!(w2.results.len(), 2);
        assert_eq!(w2.failures.len(), 1);

        assert!(!tracker.has_active_waves());
    }

    #[test]
    fn test_record_result_for_unknown_wave() {
        let mut tracker = WaveTracker::new();
        let progress = tracker.record_result(
            "w-unknown",
            0,
            vec![make_result_event("done", "orphan")],
        );
        assert_eq!(
            progress,
            WaveProgress::InProgress {
                received: 0,
                expected: 0
            }
        );
    }

    #[test]
    fn test_take_wave_results_timed_out_flag() {
        let mut tracker = WaveTracker::new();
        // Zero timeout — immediately timed out
        tracker.register_wave(
            "w-expired".into(),
            5,
            HatId::new("worker"),
            Duration::from_nanos(0),
        );

        // Record partial results
        tracker.record_result(
            "w-expired",
            0,
            vec![make_result_event("done", "partial")],
        );

        let completed = tracker.take_wave_results("w-expired").unwrap();
        assert!(completed.timed_out);
        assert_eq!(completed.results.len(), 1);
    }

    #[test]
    fn test_result_with_multiple_events() {
        let mut tracker = WaveTracker::new();
        tracker.register_wave(
            "w-multi".into(),
            1,
            HatId::new("worker"),
            Duration::from_secs(300),
        );

        // Worker emits multiple events
        let events = vec![
            make_result_event("review.done", "main review"),
            make_result_event("review.note", "additional note"),
        ];
        let progress = tracker.record_result("w-multi", 0, events);
        assert_eq!(progress, WaveProgress::Complete);

        let completed = tracker.take_wave_results("w-multi").unwrap();
        assert_eq!(completed.results.len(), 1);
        assert_eq!(completed.results[0].events.len(), 2);
    }

    #[test]
    fn test_default_impl() {
        let tracker = WaveTracker::default();
        assert!(!tracker.has_active_waves());
    }
}
