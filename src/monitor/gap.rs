//! Boot-time gap detection between completed monitor rounds.

use crate::event::{Event, EventKind};
use chrono::{DateTime, Utc};
use std::time::Duration;

const MONITORING_GAP_GRACE: Duration = Duration::from_secs(1);
pub(super) const MAX_MONITORING_GAP_DURATION_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

pub(super) trait MonitorElapsedClock {
    fn elapsed_now(&mut self) -> Option<Duration>;
}

pub(super) struct SystemBootClock;

impl MonitorElapsedClock for SystemBootClock {
    fn elapsed_now(&mut self) -> Option<Duration> {
        #[cfg(target_os = "linux")]
        {
            use nix::time::{clock_gettime, ClockId};
            let time = clock_gettime(ClockId::CLOCK_BOOTTIME).ok()?;
            let seconds = u64::try_from(time.tv_sec()).ok()?;
            let nanoseconds = u32::try_from(time.tv_nsec()).ok()?;
            if nanoseconds >= 1_000_000_000 {
                return None;
            }
            Some(Duration::new(seconds, nanoseconds))
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
}

pub(super) struct MonitorGapDetector<C> {
    clock: C,
    previous_cycle_end: Option<(Duration, DateTime<Utc>)>,
}

impl<C: MonitorElapsedClock> MonitorGapDetector<C> {
    pub(super) fn new(clock: C) -> Self {
        Self {
            clock,
            previous_cycle_end: None,
        }
    }

    pub(super) fn cycle_started<F>(
        &mut self,
        recorded_at_utc: DateTime<Utc>,
        configured_interval: Duration,
        mut next_id: F,
    ) -> Option<Event>
    where
        F: FnMut() -> String,
    {
        let now = self.clock.elapsed_now()?;
        let (previous_end, gap_started_at_utc) = self.previous_cycle_end?;
        let elapsed = now.checked_sub(previous_end)?;
        if elapsed <= configured_interval + MONITORING_GAP_GRACE {
            return None;
        }
        let elapsed_ms = u64::try_from(elapsed.as_millis())
            .ok()
            .filter(|milliseconds| *milliseconds <= MAX_MONITORING_GAP_DURATION_MS);
        Some(Event {
            schema_version: crate::event::SCHEMA_VERSION,
            id: next_id(),
            recorded_at_utc,
            source: "monitor".to_owned(),
            kind: EventKind::MonitoringGap {
                gap_started_at_utc,
                elapsed_ms,
            },
        })
    }

    pub(super) fn cycle_completed(&mut self, recorded_at_utc: DateTime<Utc>) {
        self.previous_cycle_end = self
            .clock
            .elapsed_now()
            .map(|elapsed| (elapsed, recorded_at_utc));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct FakeElapsedClock(RefCell<VecDeque<Option<Duration>>>);

    impl MonitorElapsedClock for FakeElapsedClock {
        fn elapsed_now(&mut self) -> Option<Duration> {
            self.0.borrow_mut().pop_front().unwrap()
        }
    }

    #[test]
    fn gap_detector_ignores_ordinary_interval_and_records_extended_elapsed_gap() {
        let clock = FakeElapsedClock(RefCell::new(VecDeque::from([
            Some(Duration::ZERO),
            Some(Duration::from_millis(100)),
            Some(Duration::from_millis(1_100)),
            Some(Duration::from_millis(1_200)),
            Some(Duration::from_millis(3_201)),
        ])));
        let mut detector = MonitorGapDetector::new(clock);
        let interval = Duration::from_secs(1);
        let first_end: DateTime<Utc> = "2026-10-14T12:00:00Z".parse().unwrap();
        assert!(detector
            .cycle_started(first_end, interval, || "unused".to_owned())
            .is_none());
        detector.cycle_completed(first_end);
        assert!(detector
            .cycle_started(first_end + chrono::Duration::seconds(1), interval, || {
                "unused".to_owned()
            })
            .is_none());
        detector.cycle_completed(first_end + chrono::Duration::seconds(1));

        let gap_at = first_end + chrono::Duration::seconds(2);
        let gap = detector
            .cycle_started(gap_at, interval, || "gap-event".to_owned())
            .unwrap();
        assert_eq!(gap.id, "gap-event");
        assert_eq!(gap.recorded_at_utc, gap_at);
        assert!(matches!(
            gap.kind,
            EventKind::MonitoringGap {
                gap_started_at_utc,
                elapsed_ms: Some(2_001),
            } if gap_started_at_utc == first_end + chrono::Duration::seconds(1)
        ));
    }

    #[test]
    fn gap_detection_uses_boottime_when_wall_clock_moves_backwards() {
        let clock = FakeElapsedClock(RefCell::new(VecDeque::from([
            Some(Duration::ZERO),
            Some(Duration::from_millis(100)),
            Some(Duration::from_millis(2_101)),
        ])));
        let mut detector = MonitorGapDetector::new(clock);
        let completed_at: DateTime<Utc> = "2026-10-14T12:00:10Z".parse().unwrap();
        let backwards_wall_time: DateTime<Utc> = "2026-10-14T11:59:00Z".parse().unwrap();
        assert!(detector
            .cycle_started(completed_at, Duration::from_secs(1), || "unused".to_owned())
            .is_none());
        detector.cycle_completed(completed_at);
        let gap = detector
            .cycle_started(backwards_wall_time, Duration::from_secs(1), || {
                "gap".to_owned()
            })
            .unwrap();
        assert!(gap.recorded_at_utc < completed_at);
        assert!(matches!(
            gap.kind,
            EventKind::MonitoringGap {
                gap_started_at_utc,
                elapsed_ms: Some(2_001),
            } if gap_started_at_utc == completed_at
        ));
    }

    #[test]
    fn gap_detection_does_not_infer_missing_or_backwards_elapsed_time() {
        let clock = FakeElapsedClock(RefCell::new(VecDeque::from([
            Some(Duration::from_secs(1)),
            Some(Duration::from_secs(2)),
            None,
            Some(Duration::from_secs(3)),
            Some(Duration::from_secs(2)),
        ])));
        let mut detector = MonitorGapDetector::new(clock);
        let at: DateTime<Utc> = "2026-10-14T12:00:00Z".parse().unwrap();
        assert!(detector
            .cycle_started(at, Duration::from_secs(1), || "unused".to_owned())
            .is_none());
        detector.cycle_completed(at);
        assert!(detector
            .cycle_started(at, Duration::from_secs(1), || "unused".to_owned())
            .is_none());
        detector.cycle_completed(at);
        assert!(detector
            .cycle_started(at, Duration::from_secs(1), || "unused".to_owned())
            .is_none());
    }

    #[test]
    fn gap_detector_leaves_duration_unknown_above_the_serialized_bound() {
        let previous_end = Duration::from_millis(100);
        let elapsed = Duration::from_millis(MAX_MONITORING_GAP_DURATION_MS + 2_000);
        let clock = FakeElapsedClock(RefCell::new(VecDeque::from([
            Some(Duration::ZERO),
            Some(previous_end),
            Some(previous_end + elapsed),
        ])));
        let mut detector = MonitorGapDetector::new(clock);
        let at: DateTime<Utc> = "2026-10-14T12:00:00Z".parse().unwrap();
        assert!(detector
            .cycle_started(at, Duration::from_secs(1), || "unused".to_owned())
            .is_none());
        detector.cycle_completed(at);
        let gap = detector
            .cycle_started(at, Duration::from_secs(1), || "gap".to_owned())
            .unwrap();
        assert!(matches!(
            gap.kind,
            EventKind::MonitoringGap {
                elapsed_ms: None,
                ..
            }
        ));
    }
}
