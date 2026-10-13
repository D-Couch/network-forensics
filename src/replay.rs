//! Bounded deterministic replay of versioned synthetic event fixtures.

use crate::analysis::{analyze_incidents, AnalysisConfig, AnalysisError, Finding};
use crate::event::{supports_schema_version, Event};
use crate::incidents::{analyze_events, Incident, IncidentConfig, IncidentError};
use crate::storage::StoredEvent;
use serde::Deserialize;
use std::collections::HashSet;

pub const FIXTURE_SCHEMA_VERSION: u16 = 1;
pub const MAX_FIXTURE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FIXTURE_EVENTS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayResult {
    pub incidents: Vec<Incident>,
    pub findings: Vec<Finding>,
}

#[derive(Debug)]
pub enum ReplayError {
    InputTooLarge { actual: usize, maximum: usize },
    TooManyEvents { actual: usize, maximum: usize },
    UnsupportedFixtureSchema(u16),
    DuplicateEventId,
    Json(serde_json::Error),
    Incident(IncidentError),
    Analysis(AnalysisError),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooLarge { actual, maximum } => write!(
                formatter,
                "fixture is {actual} bytes; maximum supported size is {maximum} bytes"
            ),
            Self::TooManyEvents { actual, maximum } => write!(
                formatter,
                "fixture has {actual} events; maximum supported count is {maximum}"
            ),
            Self::UnsupportedFixtureSchema(version) => {
                write!(
                    formatter,
                    "unsupported replay fixture schema version {version}"
                )
            }
            Self::DuplicateEventId => formatter.write_str("fixture event IDs must be unique"),
            Self::Json(error) => write!(formatter, "invalid replay fixture: {error}"),
            Self::Incident(error) => write!(formatter, "incident replay failed: {error}"),
            Self::Analysis(error) => write!(formatter, "analysis replay failed: {error}"),
        }
    }
}

impl std::error::Error for ReplayError {}

impl From<serde_json::Error> for ReplayError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<IncidentError> for ReplayError {
    fn from(error: IncidentError) -> Self {
        Self::Incident(error)
    }
}

impl From<AnalysisError> for ReplayError {
    fn from(error: AnalysisError) -> Self {
        Self::Analysis(error)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    fixture_schema_version: u16,
    events: Vec<FixtureEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureEvent {
    sequence: i64,
    event: Event,
}

/// Replays a strict JSON fixture without I/O, networking, or system clock use.
///
/// Each event carries its stored insertion sequence. Input is never sorted or
/// repaired: lifecycle and analysis APIs reject non-increasing sequences.
pub fn replay_json(json: &str) -> Result<ReplayResult, ReplayError> {
    if json.len() > MAX_FIXTURE_BYTES {
        return Err(ReplayError::InputTooLarge {
            actual: json.len(),
            maximum: MAX_FIXTURE_BYTES,
        });
    }
    let fixture: Fixture = serde_json::from_str(json)?;
    if fixture.fixture_schema_version != FIXTURE_SCHEMA_VERSION {
        return Err(ReplayError::UnsupportedFixtureSchema(
            fixture.fixture_schema_version,
        ));
    }
    validate_event_count(fixture.events.len())?;

    let mut ids = HashSet::with_capacity(fixture.events.len());
    let events = fixture
        .events
        .into_iter()
        .map(|item| {
            if !supports_schema_version(item.event.schema_version) {
                return Err(ReplayError::Incident(IncidentError::InvalidEventSchema(
                    item.event.schema_version,
                )));
            }
            if !ids.insert(item.event.id.clone()) {
                return Err(ReplayError::DuplicateEventId);
            }
            Ok(StoredEvent {
                sequence: item.sequence,
                event: item.event,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let incidents = analyze_events(&events, IncidentConfig::default())?;
    let findings = analyze_incidents(&events, &incidents, AnalysisConfig::default())?;
    Ok(ReplayResult {
        incidents,
        findings,
    })
}

fn validate_event_count(count: usize) -> Result<(), ReplayError> {
    if count > MAX_FIXTURE_EVENTS {
        return Err(ReplayError::TooManyEvents {
            actual: count,
            maximum: MAX_FIXTURE_EVENTS,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::FindingClassification;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ExpectedResults {
        classifications: Vec<String>,
        evidence_event_ids: Vec<Vec<String>>,
    }

    fn check_fixture(fixture: &str, expected: &str) {
        let result = replay_json(fixture).unwrap();
        let expected: ExpectedResults = serde_json::from_str(expected).unwrap();
        let classifications = result
            .findings
            .iter()
            .map(|finding| match finding.classification {
                FindingClassification::LocalLinkRelatedEvidence => "local_link_related_evidence",
                FindingClassification::ConfiguredGatewayTargetFailure => {
                    "configured_gateway_target_failure"
                }
                FindingClassification::MultipleExternalTargetsFailed => {
                    "multiple_external_targets_failed"
                }
                FindingClassification::EndpointSpecificEvidence => "endpoint_specific_evidence",
                FindingClassification::DnsQueryFailureWithIndependentTcpSuccess => {
                    "dns_query_failure_with_independent_tcp_success"
                }
                FindingClassification::Undetermined => "undetermined",
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let evidence = result
            .findings
            .iter()
            .map(|finding| finding.evidence_event_ids.clone())
            .collect::<Vec<_>>();
        assert_eq!(classifications, expected.classifications);
        assert_eq!(evidence, expected.evidence_event_ids);
        assert_eq!(replay_json(fixture).unwrap(), result);
    }

    #[test]
    fn replays_carrier_loss_scenario_against_expected_results() {
        check_fixture(
            include_str!("../tests/fixtures/carrier-loss/events.json"),
            include_str!("../tests/fixtures/carrier-loss/expected.json"),
        );
    }

    #[test]
    fn replays_endpoint_specific_scenario_against_expected_results() {
        check_fixture(
            include_str!("../tests/fixtures/endpoint-specific/events.json"),
            include_str!("../tests/fixtures/endpoint-specific/expected.json"),
        );
    }

    #[test]
    fn replays_ambiguous_scenario_as_undetermined() {
        check_fixture(
            include_str!("../tests/fixtures/ambiguous/events.json"),
            include_str!("../tests/fixtures/ambiguous/expected.json"),
        );
    }

    #[test]
    fn rejects_unsupported_versions_unknown_fields_bad_sequences_and_duplicate_ids() {
        let valid = include_str!("../tests/fixtures/ambiguous/events.json");
        assert!(matches!(
            replay_json(&valid.replace(
                "\"fixture_schema_version\": 1",
                "\"fixture_schema_version\": 2"
            )),
            Err(ReplayError::UnsupportedFixtureSchema(2))
        ));
        assert!(matches!(
            replay_json(&valid.replace(
                "\"fixture_schema_version\": 1,",
                "\"fixture_schema_version\": 1, \"extra\": true,"
            )),
            Err(ReplayError::Json(_))
        ));
        assert!(matches!(
            replay_json(&valid.replace("\"sequence\": 2", "\"sequence\": 1")),
            Err(ReplayError::Incident(IncidentError::InvalidSequence { .. }))
        ));
        assert!(matches!(
            replay_json(&valid.replace("\"id\": \"failure-2\"", "\"id\": \"failure-1\"")),
            Err(ReplayError::DuplicateEventId)
        ));
        assert!(matches!(
            replay_json(&valid.replace("\"schema_version\": 1", "\"schema_version\": 4")),
            Err(ReplayError::Json(_))
        ));
    }

    #[test]
    fn empty_fixture_returns_no_incidents_or_findings() {
        let result = replay_json(r#"{"fixture_schema_version":1,"events":[]}"#).unwrap();
        assert!(result.incidents.is_empty());
        assert!(result.findings.is_empty());
    }

    #[test]
    fn rejects_oversized_inputs_and_excessive_event_counts() {
        let input = " ".repeat(MAX_FIXTURE_BYTES + 1);
        assert!(matches!(
            replay_json(&input),
            Err(ReplayError::InputTooLarge { .. })
        ));
        assert!(matches!(
            validate_event_count(MAX_FIXTURE_EVENTS + 1),
            Err(ReplayError::TooManyEvents { .. })
        ));
    }
}
