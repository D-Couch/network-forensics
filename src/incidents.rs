//! Deterministic probe-target incident grouping over stored events.

use crate::event::{supports_schema_version, EventKind, ProbeFailureCategory, ProbeOutcome};
use crate::probes::ProbeTarget;
use crate::storage::StoredEvent;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::time::Duration;

pub const DEFAULT_FAILURE_THRESHOLD: u16 = 2;
pub const DEFAULT_RECOVERY_THRESHOLD: u16 = 2;
pub const DEFAULT_FAILURE_WINDOW: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_OBSERVATION_GAP: Duration = Duration::from_secs(60);
const MAX_THRESHOLD: u16 = 100;
const MAX_CONFIGURED_DURATION: Duration = Duration::from_secs(86_400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncidentConfig {
    failure_threshold: u16,
    recovery_threshold: u16,
    failure_window: Duration,
    max_observation_gap: Duration,
}

impl IncidentConfig {
    pub fn new(
        failure_threshold: u16,
        recovery_threshold: u16,
        failure_window: Duration,
        max_observation_gap: Duration,
    ) -> Result<Self, &'static str> {
        if !(2..=MAX_THRESHOLD).contains(&failure_threshold) {
            return Err("failure threshold must be between 2 and 100");
        }
        if !(2..=MAX_THRESHOLD).contains(&recovery_threshold) {
            return Err("recovery threshold must be between 2 and 100");
        }
        if failure_window.is_zero() || failure_window > MAX_CONFIGURED_DURATION {
            return Err("failure window must be greater than zero and no more than 24 hours");
        }
        if max_observation_gap.is_zero() || max_observation_gap > MAX_CONFIGURED_DURATION {
            return Err(
                "maximum observation gap must be greater than zero and no more than 24 hours",
            );
        }
        Ok(Self {
            failure_threshold,
            recovery_threshold,
            failure_window,
            max_observation_gap,
        })
    }

    pub fn failure_threshold(self) -> u16 {
        self.failure_threshold
    }

    pub fn recovery_threshold(self) -> u16 {
        self.recovery_threshold
    }

    pub fn failure_window(self) -> Duration {
        self.failure_window
    }

    pub fn max_observation_gap(self) -> Duration {
        self.max_observation_gap
    }
}

impl Default for IncidentConfig {
    fn default() -> Self {
        Self {
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            recovery_threshold: DEFAULT_RECOVERY_THRESHOLD,
            failure_window: DEFAULT_FAILURE_WINDOW,
            max_observation_gap: DEFAULT_MAX_OBSERVATION_GAP,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncidentStatus {
    Open,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incident {
    /// Derived from the first stored sequence and stable for this database history.
    pub id: String,
    /// This incident describes one configured TCP target, not an inferred fault domain.
    pub target: ProbeTarget,
    pub started_sequence: i64,
    pub started_at_utc: DateTime<Utc>,
    pub last_failure_sequence: i64,
    pub last_failure_at_utc: DateTime<Utc>,
    pub failure_event_ids: Vec<String>,
    pub status: IncidentStatus,
    /// The first successful probe in the recovery streak, when recovered.
    pub recovered_at_utc: Option<DateTime<Utc>>,
    /// The sequence that satisfied the recovery threshold, when recovered.
    pub recovered_sequence: Option<i64>,
    pub recovery_event_ids: Vec<String>,
    /// True when a timestamp gap exceeded the configured maximum while this episode was open.
    pub contains_observation_gap: bool,
    /// True when event timestamps moved backwards while this episode was open.
    pub contains_clock_adjustment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncidentError {
    InvalidSequence { previous: i64, current: i64 },
    InvalidEventSchema(u16),
}

impl std::fmt::Display for IncidentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSequence { previous, current } => write!(
                formatter,
                "stored event sequence must increase (received {current} after {previous})"
            ),
            Self::InvalidEventSchema(version) => {
                write!(formatter, "unsupported event schema version {version}")
            }
        }
    }
}

impl std::error::Error for IncidentError {}

#[derive(Debug, Clone)]
struct Evidence {
    sequence: i64,
    event_id: String,
    at_utc: DateTime<Utc>,
}

#[derive(Debug, Default)]
struct TargetState {
    last_observation_at: Option<DateTime<Utc>>,
    pending_failures: Vec<Evidence>,
    recovery_streak: Vec<Evidence>,
    active: Option<Incident>,
}

/// Groups probe outcomes without I/O, networking, or diagnosis.
///
/// Input must be in strictly increasing SQLite insertion-sequence order. Events
/// for separate configured targets are analyzed independently.
pub fn analyze_events(
    events: &[StoredEvent],
    config: IncidentConfig,
) -> Result<Vec<Incident>, IncidentError> {
    let mut previous_sequence = None;
    let mut states: HashMap<ProbeTarget, TargetState> = HashMap::new();
    let mut completed = Vec::new();

    for stored in events {
        if let Some(previous) = previous_sequence {
            if stored.sequence <= previous {
                return Err(IncidentError::InvalidSequence {
                    previous,
                    current: stored.sequence,
                });
            }
        }
        previous_sequence = Some(stored.sequence);

        if !supports_schema_version(stored.event.schema_version) {
            return Err(IncidentError::InvalidEventSchema(
                stored.event.schema_version,
            ));
        }

        let EventKind::ProbeResult {
            role,
            target,
            outcome,
            ..
        } = &stored.event.kind
        else {
            continue;
        };
        let probe_target = ProbeTarget {
            role: *role,
            address: *target,
        };
        let state = states.entry(probe_target).or_default();
        let observed_at = stored.event.recorded_at_utc;

        if let Some(previous_time) = state.last_observation_at {
            let delta_ms =
                observed_at.timestamp_millis() as i128 - previous_time.timestamp_millis() as i128;
            if delta_ms < 0 {
                state.pending_failures.clear();
                state.recovery_streak.clear();
                if let Some(incident) = state.active.as_mut() {
                    incident.contains_clock_adjustment = true;
                }
            } else if delta_ms > config.max_observation_gap.as_millis() as i128 {
                state.pending_failures.clear();
                state.recovery_streak.clear();
                if let Some(incident) = state.active.as_mut() {
                    incident.contains_observation_gap = true;
                }
            }
        }
        state.last_observation_at = Some(observed_at);

        match outcome {
            ProbeOutcome::Success => {
                state.pending_failures.clear();
                if state.active.is_some() {
                    state.recovery_streak.push(Evidence {
                        sequence: stored.sequence,
                        event_id: stored.event.id.clone(),
                        at_utc: observed_at,
                    });
                    if state.recovery_streak.len() >= usize::from(config.recovery_threshold) {
                        let recovery_evidence = std::mem::take(&mut state.recovery_streak);
                        let mut incident = state.active.take().expect("active incident exists");
                        incident.status = IncidentStatus::Recovered;
                        incident.recovered_at_utc = Some(recovery_evidence[0].at_utc);
                        incident.recovered_sequence =
                            recovery_evidence.last().map(|evidence| evidence.sequence);
                        incident.recovery_event_ids = recovery_evidence
                            .into_iter()
                            .map(|evidence| evidence.event_id)
                            .collect();
                        completed.push(incident);
                    }
                } else {
                    state.recovery_streak.clear();
                }
            }
            ProbeOutcome::Timeout
            | ProbeOutcome::Failure(
                ProbeFailureCategory::NetworkUnreachable | ProbeFailureCategory::HostUnreachable,
            ) => {
                state.recovery_streak.clear();
                if let Some(incident) = state.active.as_mut() {
                    incident.last_failure_sequence = stored.sequence;
                    incident.last_failure_at_utc = observed_at;
                    incident.failure_event_ids.push(stored.event.id.clone());
                } else {
                    if state.pending_failures.last().is_some_and(|last| {
                        let delta_ms = observed_at.timestamp_millis() as i128
                            - last.at_utc.timestamp_millis() as i128;
                        delta_ms > config.failure_window.as_millis() as i128
                    }) {
                        state.pending_failures.clear();
                    }
                    state.pending_failures.push(Evidence {
                        sequence: stored.sequence,
                        event_id: stored.event.id.clone(),
                        at_utc: observed_at,
                    });
                    if state.pending_failures.len() >= usize::from(config.failure_threshold) {
                        let evidence = std::mem::take(&mut state.pending_failures);
                        let first = &evidence[0];
                        state.active = Some(Incident {
                            id: format!("incident-{}", first.sequence),
                            target: probe_target,
                            started_sequence: first.sequence,
                            started_at_utc: first.at_utc,
                            last_failure_sequence: stored.sequence,
                            last_failure_at_utc: observed_at,
                            failure_event_ids: evidence
                                .into_iter()
                                .map(|item| item.event_id)
                                .collect(),
                            status: IncidentStatus::Open,
                            recovered_at_utc: None,
                            recovered_sequence: None,
                            recovery_event_ids: Vec::new(),
                            contains_observation_gap: false,
                            contains_clock_adjustment: false,
                        });
                    }
                }
            }
            ProbeOutcome::Failure(ProbeFailureCategory::ConnectionRefused)
            | ProbeOutcome::Failure(ProbeFailureCategory::Other)
            | ProbeOutcome::UnsupportedOrBlocked(_) => {
                state.pending_failures.clear();
                state.recovery_streak.clear();
            }
        }
    }

    let mut incidents = completed;
    for state in states.values_mut() {
        if let Some(incident) = state.active.take() {
            incidents.push(incident);
        }
    }
    incidents.sort_by_key(|incident| incident.started_sequence);
    Ok(incidents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, ProbeLimitationCategory, ProbeMethod, ProbeRole};

    fn target(address: &str) -> ProbeTarget {
        ProbeTarget {
            role: ProbeRole::External,
            address: address.parse().unwrap(),
        }
    }

    fn stored(
        sequence: i64,
        id: &str,
        at: &str,
        address: &str,
        outcome: ProbeOutcome,
    ) -> StoredEvent {
        let at = at.parse().unwrap();
        StoredEvent {
            sequence,
            event: Event {
                schema_version: crate::event::SCHEMA_VERSION,
                id: id.to_owned(),
                recorded_at_utc: at,
                source: "probe".to_owned(),
                kind: EventKind::ProbeResult {
                    role: ProbeRole::External,
                    target: address.parse().unwrap(),
                    method: ProbeMethod::TcpConnect,
                    started_at_utc: at,
                    elapsed_ms: 5,
                    outcome,
                },
            },
        }
    }

    #[test]
    fn opens_after_consecutive_failures_and_keeps_first_evidence() {
        let events = [
            stored(
                1,
                "fail-1",
                "2026-10-13T00:00:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                2,
                "fail-2",
                "2026-10-13T00:00:20Z",
                "192.0.2.1:443",
                ProbeOutcome::Failure(ProbeFailureCategory::NetworkUnreachable),
            ),
        ];
        let incidents = analyze_events(&events, IncidentConfig::default()).unwrap();
        assert_eq!(incidents.len(), 1);
        assert_eq!(incidents[0].id, "incident-1");
        assert_eq!(incidents[0].started_sequence, 1);
        assert_eq!(incidents[0].last_failure_sequence, 2);
        assert_eq!(incidents[0].failure_event_ids, ["fail-1", "fail-2"]);
        assert_eq!(incidents[0].status, IncidentStatus::Open);
        assert_eq!(incidents[0].target, target("192.0.2.1:443"));
    }

    #[test]
    fn one_failure_or_failures_outside_window_do_not_open_an_incident() {
        let events = [
            stored(
                1,
                "fail-1",
                "2026-10-13T00:00:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                2,
                "fail-2",
                "2026-10-13T00:00:31Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
        ];
        assert!(analyze_events(&events[..1], IncidentConfig::default())
            .unwrap()
            .is_empty());
        assert!(analyze_events(&events, IncidentConfig::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn targets_are_independent_and_unknown_or_refused_outcomes_break_streaks() {
        let events = [
            stored(
                1,
                "a-fail",
                "2026-10-13T00:00:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                2,
                "b-fail",
                "2026-10-13T00:00:01Z",
                "192.0.2.2:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                3,
                "a-refused",
                "2026-10-13T00:00:02Z",
                "192.0.2.1:443",
                ProbeOutcome::Failure(ProbeFailureCategory::ConnectionRefused),
            ),
            stored(
                4,
                "a-other",
                "2026-10-13T00:00:03Z",
                "192.0.2.1:443",
                ProbeOutcome::Failure(ProbeFailureCategory::Other),
            ),
            stored(
                5,
                "a-blocked",
                "2026-10-13T00:00:04Z",
                "192.0.2.1:443",
                ProbeOutcome::UnsupportedOrBlocked(ProbeLimitationCategory::MethodUnsupported),
            ),
            stored(
                6,
                "a-fail-again",
                "2026-10-13T00:00:05Z",
                "192.0.2.1:443",
                ProbeOutcome::Failure(ProbeFailureCategory::HostUnreachable),
            ),
        ];
        assert!(analyze_events(&events, IncidentConfig::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn two_successes_recover_and_a_failure_resets_recovery_streak() {
        let events = [
            stored(
                1,
                "fail-1",
                "2026-10-13T00:00:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                2,
                "fail-2",
                "2026-10-13T00:00:10Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                3,
                "ok-1",
                "2026-10-13T00:00:20Z",
                "192.0.2.1:443",
                ProbeOutcome::Success,
            ),
            stored(
                4,
                "fail-3",
                "2026-10-13T00:00:30Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                5,
                "ok-2",
                "2026-10-13T00:00:40Z",
                "192.0.2.1:443",
                ProbeOutcome::Success,
            ),
            stored(
                6,
                "ok-3",
                "2026-10-13T00:00:50Z",
                "192.0.2.1:443",
                ProbeOutcome::Success,
            ),
            stored(
                7,
                "fail-4",
                "2026-10-13T00:01:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                8,
                "fail-5",
                "2026-10-13T00:01:10Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
        ];
        let incidents = analyze_events(&events, IncidentConfig::default()).unwrap();
        assert_eq!(incidents.len(), 2);
        let incident = &incidents[0];
        assert_eq!(incident.status, IncidentStatus::Recovered);
        assert_eq!(incident.last_failure_sequence, 4);
        assert_eq!(incident.recovered_sequence, Some(6));
        assert_eq!(
            incident.recovered_at_utc,
            Some("2026-10-13T00:00:40Z".parse().unwrap())
        );
        assert_eq!(incident.recovery_event_ids, ["ok-2", "ok-3"]);
        assert_eq!(incident.failure_event_ids, ["fail-1", "fail-2", "fail-3"]);
        assert_eq!(incidents[1].id, "incident-7");
        assert_eq!(incidents[1].status, IncidentStatus::Open);
    }

    #[test]
    fn gaps_and_clock_changes_reset_streaks_and_mark_open_incidents() {
        let gap_events = [
            stored(
                1,
                "fail-1",
                "2026-10-13T00:00:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                2,
                "fail-2",
                "2026-10-13T00:00:10Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                3,
                "fail-after-gap",
                "2026-10-13T00:02:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                4,
                "ok-1",
                "2026-10-13T00:02:10Z",
                "192.0.2.1:443",
                ProbeOutcome::Success,
            ),
            stored(
                5,
                "ok-2",
                "2026-10-13T00:02:20Z",
                "192.0.2.1:443",
                ProbeOutcome::Success,
            ),
        ];
        let incident = &analyze_events(&gap_events, IncidentConfig::default()).unwrap()[0];
        assert!(incident.contains_observation_gap);
        assert_eq!(incident.status, IncidentStatus::Recovered);

        let clock_events = [
            stored(
                1,
                "fail-1",
                "2026-10-13T00:01:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                2,
                "fail-2",
                "2026-10-13T00:01:10Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                3,
                "clock-back",
                "2026-10-13T00:00:00Z",
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            stored(
                4,
                "ok-1",
                "2026-10-13T00:00:10Z",
                "192.0.2.1:443",
                ProbeOutcome::Success,
            ),
            stored(
                5,
                "ok-2",
                "2026-10-13T00:00:20Z",
                "192.0.2.1:443",
                ProbeOutcome::Success,
            ),
        ];
        let incident = &analyze_events(&clock_events, IncidentConfig::default()).unwrap()[0];
        assert!(incident.contains_clock_adjustment);
        assert_eq!(incident.status, IncidentStatus::Recovered);
    }

    #[test]
    fn rejects_non_increasing_sequence_and_future_event_schema() {
        let first = stored(
            2,
            "first",
            "2026-10-13T00:00:00Z",
            "192.0.2.1:443",
            ProbeOutcome::Timeout,
        );
        let second = stored(
            2,
            "second",
            "2026-10-13T00:00:01Z",
            "192.0.2.1:443",
            ProbeOutcome::Timeout,
        );
        assert!(matches!(
            analyze_events(&[first, second], IncidentConfig::default()),
            Err(IncidentError::InvalidSequence { .. })
        ));

        let mut future = stored(
            3,
            "future",
            "2026-10-13T00:00:00Z",
            "192.0.2.1:443",
            ProbeOutcome::Timeout,
        );
        future.event.schema_version = crate::event::SCHEMA_VERSION + 1;
        assert!(matches!(
            analyze_events(&[future], IncidentConfig::default()),
            Err(IncidentError::InvalidEventSchema(_))
        ));
    }

    #[test]
    fn validates_threshold_configuration() {
        assert!(IncidentConfig::new(1, 2, Duration::from_secs(1), Duration::from_secs(1)).is_err());
        assert!(
            IncidentConfig::new(2, 101, Duration::from_secs(1), Duration::from_secs(1)).is_err()
        );
        assert!(IncidentConfig::new(2, 2, Duration::ZERO, Duration::from_secs(1)).is_err());
        assert!(
            IncidentConfig::new(2, 2, Duration::from_secs(1), Duration::from_secs(86_401)).is_err()
        );
    }
}
