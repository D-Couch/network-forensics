//! Conservative, evidence-citing rules over stored observations.

use crate::event::{
    supports_schema_version, DnsOutcome, EventKind, KernelLinkSignalCategory, ProbeFailureCategory,
    ProbeOutcome, ProbeRole,
};
use crate::incidents::Incident;
use crate::storage::StoredEvent;
use std::net::SocketAddr;
use std::time::Duration;

pub const DEFAULT_CORRELATION_WINDOW: Duration = Duration::from_secs(60);
const MAX_CORRELATION_WINDOW: Duration = Duration::from_secs(86_400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisConfig {
    correlation_window: Duration,
}

impl AnalysisConfig {
    pub fn new(correlation_window: Duration) -> Result<Self, &'static str> {
        if correlation_window.is_zero() || correlation_window > MAX_CORRELATION_WINDOW {
            return Err("correlation window must be greater than zero and no more than 24 hours");
        }
        Ok(Self { correlation_window })
    }

    pub fn correlation_window(self) -> Duration {
        self.correlation_window
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            correlation_window: DEFAULT_CORRELATION_WINDOW,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingClassification {
    LocalLinkRelatedEvidence,
    ConfiguredGatewayTargetFailure,
    MultipleExternalTargetsFailed,
    EndpointSpecificEvidence,
    DnsQueryFailureWithIndependentTcpSuccess,
    Undetermined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub incident_id: String,
    pub classification: FindingClassification,
    pub explanation: String,
    pub evidence_event_ids: Vec<String>,
    pub limitations: Vec<String>,
    pub suggested_follow_up: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    InvalidSequence { previous: i64, current: i64 },
    InvalidEventSchema(u16),
}

impl std::fmt::Display for AnalysisError {
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

impl std::error::Error for AnalysisError {}

/// Analyzes supplied incidents using only nearby stored observations.
///
/// Input events must be in strictly increasing insertion-sequence order. When
/// an incident contains a backwards clock adjustment, timestamp-based
/// cross-event correlations are skipped; its own probe evidence remains valid.
pub fn analyze_incidents(
    events: &[StoredEvent],
    incidents: &[Incident],
    config: AnalysisConfig,
) -> Result<Vec<Finding>, AnalysisError> {
    validate_events(events)?;
    let timestamps_monotonic = events
        .windows(2)
        .all(|pair| pair[1].event.recorded_at_utc >= pair[0].event.recorded_at_utc);
    let mut ordered_incidents = incidents.iter().collect::<Vec<_>>();
    ordered_incidents.sort_by_key(|incident| incident.started_sequence);
    Ok(ordered_incidents
        .into_iter()
        .map(|incident| analyze_one(events, incident, config, timestamps_monotonic))
        .collect())
}

fn validate_events(events: &[StoredEvent]) -> Result<(), AnalysisError> {
    let mut previous = None;
    for stored in events {
        if let Some(sequence) = previous {
            if stored.sequence <= sequence {
                return Err(AnalysisError::InvalidSequence {
                    previous: sequence,
                    current: stored.sequence,
                });
            }
        }
        previous = Some(stored.sequence);
        if !supports_schema_version(stored.event.schema_version) {
            return Err(AnalysisError::InvalidEventSchema(
                stored.event.schema_version,
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct ProbeEvidence {
    id: String,
    role: ProbeRole,
    target: SocketAddr,
    outcome: ProbeOutcome,
}

fn analyze_one(
    events: &[StoredEvent],
    incident: &Incident,
    config: AnalysisConfig,
    timestamps_monotonic: bool,
) -> Finding {
    let clock_adjusted = incident.contains_clock_adjustment;
    let lower_ms =
        incident.started_at_utc.timestamp_millis() - config.correlation_window.as_millis() as i64;
    let end_at = incident
        .recovered_at_utc
        .unwrap_or(incident.last_failure_at_utc);
    let upper_ms = end_at.timestamp_millis() + config.correlation_window.as_millis() as i64;

    let mut probes = Vec::new();
    let mut local_link_signal_ids = Vec::new();
    let mut dns_failures = Vec::new();
    for stored in events {
        let in_window = !clock_adjusted
            && timestamps_monotonic
            && stored.event.recorded_at_utc.timestamp_millis() >= lower_ms
            && stored.event.recorded_at_utc.timestamp_millis() <= upper_ms;
        match &stored.event.kind {
            EventKind::ProbeResult {
                role,
                target,
                outcome,
                ..
            } if in_window => probes.push(ProbeEvidence {
                id: stored.event.id.clone(),
                role: *role,
                target: *target,
                outcome: outcome.clone(),
            }),
            EventKind::LinkStateChanged {
                carrier: Some(false),
                ..
            } if in_window => {
                local_link_signal_ids.push((stored.sequence, stored.event.id.clone()))
            }
            EventKind::KernelLinkSignal {
                category:
                    KernelLinkSignalCategory::LinkDown | KernelLinkSignalCategory::NetdevWatchdog,
                ..
            } if in_window => {
                local_link_signal_ids.push((stored.sequence, stored.event.id.clone()))
            }
            EventKind::DnsResult { outcome, .. } if in_window => {
                if matches!(outcome, DnsOutcome::Timeout | DnsOutcome::Failure(_)) {
                    dns_failures.push((stored.event.id.clone(), outcome.clone()));
                }
            }
            _ => {}
        }
    }

    let external_failures = probes
        .iter()
        .filter(|probe| probe.role == ProbeRole::External && qualifies_as_failure(&probe.outcome))
        .collect::<Vec<_>>();
    let external_failed_targets = external_failures.iter().map(|probe| probe.target).fold(
        Vec::new(),
        |mut targets, target| {
            if !targets.contains(&target) {
                targets.push(target);
            }
            targets
        },
    );
    let other_external_successes = probes
        .iter()
        .filter(|probe| {
            probe.role == ProbeRole::External
                && probe.target != incident.target.address
                && probe.outcome == ProbeOutcome::Success
        })
        .collect::<Vec<_>>();
    let has_other_target_failure = external_failures
        .iter()
        .any(|probe| probe.target != incident.target.address);
    let successful_independent_targets = other_external_successes
        .iter()
        .filter(|probe| !external_failed_targets.contains(&probe.target))
        .collect::<Vec<_>>();

    let (classification, explanation, suggested_follow_up, supporting_ids) =
        if !local_link_signal_ids.is_empty() {
            local_link_signal_ids.sort_by_key(|(sequence, _)| *sequence);
            (
                FindingClassification::LocalLinkRelatedEvidence,
                "A carrier-down or normalized kernel link-down/watchdog observation occurred within the incident correlation window. This is local-link-related evidence, not a diagnosis of a physical cause or proof that the signal caused the incident.".to_owned(),
                "Review the nearby interface snapshot and counter deltas, then compare another monitoring period if the signal recurs. This does not identify a failed component or prove the interface carried these probes.".to_owned(),
                local_link_signal_ids
                    .into_iter()
                    .map(|(_, id)| id)
                    .collect::<Vec<_>>(),
            )
        } else if incident.target.role == ProbeRole::Gateway {
            (
                FindingClassification::ConfiguredGatewayTargetFailure,
                "The configured gateway-role TCP target repeatedly failed. The role is caller-provided and does not verify that this endpoint is the active gateway.".to_owned(),
                "Confirm this IP:port is the endpoint you intended to label as the gateway; compare its failures with interface/route observations and a separate external target. The role does not verify the active gateway.".to_owned(),
                Vec::new(),
            )
        } else if external_failed_targets.len() >= 2 {
            (
                FindingClassification::MultipleExternalTargetsFailed,
                "Qualifying failures affected multiple configured external targets in the correlation window. This does not establish a shared upstream cause.".to_owned(),
                "Compare the failure times with a configured gateway-role probe and interface observations; repeat the same targets in another period before drawing broader conclusions.".to_owned(),
                external_failures
                    .iter()
                    .map(|probe| probe.id.clone())
                    .collect::<Vec<_>>(),
            )
        } else if !dns_failures.is_empty()
            && probes.iter().any(|probe| {
                probe.role == ProbeRole::External && probe.outcome == ProbeOutcome::Success
            })
        {
            (
            FindingClassification::DnsQueryFailureWithIndependentTcpSuccess,
            "A configured DNS query failed while an independent external TCP target succeeded in the same correlation window. This is DNS-query failure evidence and does not establish that DNS caused the incident or that other destinations are unreachable.".to_owned(),
            "Confirm the configured FQDN is the intended name, then repeat the DNS observation and compare it with independent external TCP probes. This result describes only that query.".to_owned(),
            dns_failures.iter().map(|(id, _)| id.clone()).collect(),
        )
        } else if !successful_independent_targets.is_empty() && !has_other_target_failure {
            (
                FindingClassification::EndpointSpecificEvidence,
                "A different configured external target succeeded while this target had qualifying failures. The evidence is target-specific and does not establish why this endpoint failed.".to_owned(),
                "Verify that the intended service should accept TCP at this IP:port, then compare a fresh probe to it with another configured external target. This narrows the evidence to the endpoint but does not explain the failure.".to_owned(),
                successful_independent_targets
                    .iter()
                    .map(|probe| probe.id.clone())
                    .collect::<Vec<_>>(),
            )
        } else {
            (
                FindingClassification::Undetermined,
                "The stored observations do not distinguish a fault domain for this target-level incident.".to_owned(),
                "Collect another monitoring period with two distinct external targets and, if useful, an explicitly configured gateway target. Add optional DNS or kernel-journal observations only when those signals are relevant.".to_owned(),
                Vec::new(),
            )
        };

    let mut evidence_event_ids = incident.failure_event_ids.clone();
    for id in &incident.recovery_event_ids {
        push_unique(&mut evidence_event_ids, id.clone());
    }
    for id in supporting_ids {
        push_unique(&mut evidence_event_ids, id);
    }
    let mut limitations = vec![
        "TCP probe outcomes describe only the configured IP:port endpoints.".to_owned(),
        "Gateway and external roles are labels supplied by the caller.".to_owned(),
        "No route-to-interface mapping is available to verify that a carrier or kernel journal observation affected this probe path.".to_owned(),
        "DNS-specific causes remain undetermined without a correlated DNS-query failure and independent TCP success; DNS observations describe only the explicitly configured query name."
            .to_owned(),
    ];
    if incident.contains_observation_gap {
        limitations.push(
            "An observation gap occurred while the incident was open; continuous conditions are unknown."
                .to_owned(),
        );
    }
    if clock_adjusted {
        limitations.push(
            "A backward timestamp was observed; cross-event timestamp correlations were skipped."
                .to_owned(),
        );
    }
    if !timestamps_monotonic && !clock_adjusted {
        limitations.push(
            "Stored event timestamps moved backwards; cross-event timestamp correlations were skipped."
                .to_owned(),
        );
    }
    Finding {
        incident_id: incident.id.clone(),
        classification,
        explanation,
        evidence_event_ids,
        limitations,
        suggested_follow_up,
    }
}

fn qualifies_as_failure(outcome: &ProbeOutcome) -> bool {
    matches!(
        outcome,
        ProbeOutcome::Timeout
            | ProbeOutcome::Failure(
                ProbeFailureCategory::NetworkUnreachable | ProbeFailureCategory::HostUnreachable
            )
    )
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, KernelLinkSignalCategory, ProbeMethod};
    use crate::incidents::{analyze_events as group_incidents, IncidentConfig};

    fn event(sequence: i64, id: &str, at: &str, kind: EventKind) -> StoredEvent {
        StoredEvent {
            sequence,
            event: Event {
                schema_version: crate::event::SCHEMA_VERSION,
                id: id.to_owned(),
                recorded_at_utc: at.parse().unwrap(),
                source: "synthetic".to_owned(),
                kind,
            },
        }
    }

    fn probe(
        sequence: i64,
        id: &str,
        at: &str,
        role: ProbeRole,
        address: &str,
        outcome: ProbeOutcome,
    ) -> StoredEvent {
        event(
            sequence,
            id,
            at,
            EventKind::ProbeResult {
                role,
                target: address.parse().unwrap(),
                method: ProbeMethod::TcpConnect,
                started_at_utc: at.parse().unwrap(),
                elapsed_ms: 5,
                outcome,
            },
        )
    }

    fn kernel_signal(
        sequence: i64,
        id: &str,
        at: &str,
        category: KernelLinkSignalCategory,
    ) -> StoredEvent {
        event(
            sequence,
            id,
            at,
            EventKind::KernelLinkSignal {
                category,
                interface: Some("eno1".to_owned()),
            },
        )
    }

    fn incidents(events: &[StoredEvent]) -> Vec<Incident> {
        group_incidents(events, IncidentConfig::default()).unwrap()
    }

    #[test]
    fn carrier_loss_is_link_related_evidence_with_explicit_limitations() {
        let events = [
            event(
                1,
                "carrier-down",
                "2026-10-13T00:00:00Z",
                EventKind::LinkStateChanged {
                    interface: "eth0".to_owned(),
                    carrier: Some(false),
                },
            ),
            probe(
                2,
                "fail-1",
                "2026-10-13T00:00:10Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                3,
                "fail-2",
                "2026-10-13T00:00:20Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
        ];
        let findings =
            analyze_incidents(&events, &incidents(&events), AnalysisConfig::default()).unwrap();
        assert_eq!(
            findings[0].classification,
            FindingClassification::LocalLinkRelatedEvidence
        );
        assert_eq!(
            findings[0].evidence_event_ids,
            ["fail-1", "fail-2", "carrier-down"]
        );
        assert!(findings[0].explanation.contains("not a diagnosis"));
        assert!(findings[0]
            .suggested_follow_up
            .contains("interface snapshot and counter deltas"));
        assert!(findings[0]
            .suggested_follow_up
            .contains("does not identify a failed component"));
        assert!(findings[0]
            .limitations
            .iter()
            .any(|limitation| limitation.contains("DNS-specific")));
    }

    #[test]
    fn kernel_link_down_and_watchdog_support_existing_local_link_finding() {
        for (category, id) in [
            (KernelLinkSignalCategory::LinkDown, "kernel-down"),
            (KernelLinkSignalCategory::NetdevWatchdog, "kernel-watchdog"),
        ] {
            let events = [
                kernel_signal(1, id, "2026-10-13T00:00:00Z", category),
                probe(
                    2,
                    "fail-1",
                    "2026-10-13T00:00:10Z",
                    ProbeRole::External,
                    "192.0.2.1:443",
                    ProbeOutcome::Timeout,
                ),
                probe(
                    3,
                    "fail-2",
                    "2026-10-13T00:00:20Z",
                    ProbeRole::External,
                    "192.0.2.1:443",
                    ProbeOutcome::Timeout,
                ),
            ];
            let finding =
                &analyze_incidents(&events, &incidents(&events), AnalysisConfig::default())
                    .unwrap()[0];
            assert_eq!(
                finding.classification,
                FindingClassification::LocalLinkRelatedEvidence
            );
            assert!(finding
                .evidence_event_ids
                .iter()
                .any(|event_id| event_id == id));
            assert!(finding.explanation.contains("proof that the signal caused"));
        }
    }

    #[test]
    fn kernel_link_up_out_of_window_and_backwards_time_do_not_support_local_link_finding() {
        let failures = || {
            vec![
                probe(
                    1,
                    "fail-1",
                    "2026-10-13T00:00:00Z",
                    ProbeRole::External,
                    "192.0.2.1:443",
                    ProbeOutcome::Timeout,
                ),
                probe(
                    3,
                    "fail-2",
                    "2026-10-13T00:00:10Z",
                    ProbeRole::External,
                    "192.0.2.1:443",
                    ProbeOutcome::Timeout,
                ),
            ]
        };

        let mut link_up = failures();
        link_up.insert(
            1,
            kernel_signal(
                2,
                "kernel-up",
                "2026-10-13T00:00:05Z",
                KernelLinkSignalCategory::LinkUp,
            ),
        );
        let finding = &analyze_incidents(&link_up, &incidents(&link_up), AnalysisConfig::default())
            .unwrap()[0];
        assert_eq!(finding.classification, FindingClassification::Undetermined);
        assert!(!finding
            .evidence_event_ids
            .iter()
            .any(|id| id == "kernel-up"));

        let mut out_of_window = failures();
        out_of_window.insert(
            0,
            kernel_signal(
                0,
                "stale-kernel-down",
                "2026-10-12T23:58:00Z",
                KernelLinkSignalCategory::LinkDown,
            ),
        );
        let finding = &analyze_incidents(
            &out_of_window,
            &incidents(&out_of_window),
            AnalysisConfig::default(),
        )
        .unwrap()[0];
        assert_eq!(finding.classification, FindingClassification::Undetermined);
        assert!(!finding
            .evidence_event_ids
            .iter()
            .any(|id| id == "stale-kernel-down"));

        let backwards = [
            probe(
                1,
                "fail-1",
                "2026-10-13T00:00:10Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            kernel_signal(
                2,
                "backwards-kernel-down",
                "2026-10-13T00:00:05Z",
                KernelLinkSignalCategory::LinkDown,
            ),
            probe(
                3,
                "fail-2",
                "2026-10-13T00:00:20Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
        ];
        let finding = &analyze_incidents(
            &backwards,
            &incidents(&backwards),
            AnalysisConfig::default(),
        )
        .unwrap()[0];
        assert_eq!(finding.classification, FindingClassification::Undetermined);
        assert!(!finding
            .evidence_event_ids
            .iter()
            .any(|id| id == "backwards-kernel-down"));
    }

    #[test]
    fn another_healthy_external_target_supports_endpoint_specific_evidence() {
        let events = [
            probe(
                1,
                "target-a-fail-1",
                "2026-10-13T00:00:00Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                2,
                "target-b-ok",
                "2026-10-13T00:00:10Z",
                ProbeRole::External,
                "192.0.2.2:443",
                ProbeOutcome::Success,
            ),
            probe(
                3,
                "target-a-fail-2",
                "2026-10-13T00:00:20Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
        ];
        let findings =
            analyze_incidents(&events, &incidents(&events), AnalysisConfig::default()).unwrap();
        assert_eq!(
            findings[0].classification,
            FindingClassification::EndpointSpecificEvidence
        );
        assert!(findings[0]
            .evidence_event_ids
            .contains(&"target-b-ok".to_owned()));
        assert!(findings[0]
            .suggested_follow_up
            .contains("intended service should accept TCP"));
        assert!(findings[0]
            .suggested_follow_up
            .contains("does not explain the failure"));
    }

    #[test]
    fn failures_at_multiple_external_targets_do_not_claim_a_shared_cause() {
        let events = [
            probe(
                1,
                "a-fail-1",
                "2026-10-13T00:00:00Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                2,
                "b-fail-1",
                "2026-10-13T00:00:10Z",
                ProbeRole::External,
                "192.0.2.2:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                3,
                "a-fail-2",
                "2026-10-13T00:00:20Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                4,
                "b-fail-2",
                "2026-10-13T00:00:30Z",
                ProbeRole::External,
                "192.0.2.2:443",
                ProbeOutcome::Timeout,
            ),
        ];
        let findings =
            analyze_incidents(&events, &incidents(&events), AnalysisConfig::default()).unwrap();
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|finding| {
            finding.classification == FindingClassification::MultipleExternalTargetsFailed
                && finding
                    .explanation
                    .contains("does not establish a shared upstream cause")
                && finding.suggested_follow_up.contains("another period")
        }));
    }

    #[test]
    fn caller_gateway_role_is_reported_as_a_target_label_not_a_verified_gateway() {
        let events = [
            probe(
                1,
                "gw-fail-1",
                "2026-10-13T00:00:00Z",
                ProbeRole::Gateway,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                2,
                "gw-fail-2",
                "2026-10-13T00:00:10Z",
                ProbeRole::Gateway,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
        ];
        let findings =
            analyze_incidents(&events, &incidents(&events), AnalysisConfig::default()).unwrap();
        assert_eq!(
            findings[0].classification,
            FindingClassification::ConfiguredGatewayTargetFailure
        );
        assert!(findings[0].explanation.contains("caller-provided"));
        assert!(findings[0]
            .suggested_follow_up
            .contains("does not verify the active gateway"));
    }

    #[test]
    fn stale_other_target_success_does_not_support_endpoint_specific_finding() {
        let events = [
            probe(
                1,
                "old-ok",
                "2026-10-12T23:00:00Z",
                ProbeRole::External,
                "192.0.2.2:443",
                ProbeOutcome::Success,
            ),
            probe(
                2,
                "fail-1",
                "2026-10-13T00:00:00Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                3,
                "fail-2",
                "2026-10-13T00:00:10Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
        ];
        let findings =
            analyze_incidents(&events, &incidents(&events), AnalysisConfig::default()).unwrap();
        assert_eq!(
            findings[0].classification,
            FindingClassification::Undetermined
        );
        assert_eq!(findings[0].evidence_event_ids, ["fail-1", "fail-2"]);
        assert!(findings[0]
            .suggested_follow_up
            .contains("two distinct external targets"));
    }

    #[test]
    fn clock_adjustment_skips_cross_event_correlation() {
        let events = [
            probe(
                1,
                "fail-1",
                "2026-10-13T00:00:00Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                2,
                "fail-2",
                "2026-10-13T00:00:10Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            probe(
                3,
                "other-ok",
                "2026-10-13T00:00:05Z",
                ProbeRole::External,
                "192.0.2.2:443",
                ProbeOutcome::Success,
            ),
        ];
        let incidents = incidents(&events);
        let findings = analyze_incidents(&events, &incidents, AnalysisConfig::default()).unwrap();
        assert_eq!(
            findings[0].classification,
            FindingClassification::Undetermined
        );
        assert!(findings[0]
            .limitations
            .iter()
            .any(|limitation| limitation.contains("Stored event timestamps moved backwards")));
    }

    #[test]
    fn rejects_invalid_sequence_schema_and_correlation_window() {
        let first = probe(
            2,
            "a",
            "2026-10-13T00:00:00Z",
            ProbeRole::External,
            "192.0.2.1:443",
            ProbeOutcome::Timeout,
        );
        let second = probe(
            1,
            "b",
            "2026-10-13T00:00:01Z",
            ProbeRole::External,
            "192.0.2.1:443",
            ProbeOutcome::Timeout,
        );
        assert!(matches!(
            analyze_incidents(&[first.clone(), second], &[], AnalysisConfig::default()),
            Err(AnalysisError::InvalidSequence { .. })
        ));
        let mut future = first;
        future.event.schema_version = 99;
        assert_eq!(
            analyze_incidents(&[future], &[], AnalysisConfig::default()).unwrap_err(),
            AnalysisError::InvalidEventSchema(99)
        );
        assert!(AnalysisConfig::new(Duration::ZERO).is_err());
        assert!(AnalysisConfig::new(Duration::from_secs(86_401)).is_err());
    }

    #[test]
    fn dns_failure_with_external_tcp_success_is_cautious_concurrent_evidence() {
        let events = [
            probe(
                1,
                "incident-fail-1",
                "2026-10-13T00:00:00Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
            event(
                2,
                "dns-fail",
                "2026-10-13T00:00:05Z",
                EventKind::DnsResult {
                    query_name: "example.com.".into(),
                    started_at_utc: "2026-10-13T00:00:05Z".parse().unwrap(),
                    elapsed_ms: 500,
                    outcome: DnsOutcome::Failure(crate::event::DnsFailureCategory::NameNotFound),
                },
            ),
            probe(
                3,
                "independent-success",
                "2026-10-13T00:00:10Z",
                ProbeRole::External,
                "192.0.2.2:443",
                ProbeOutcome::Success,
            ),
            probe(
                4,
                "incident-fail-2",
                "2026-10-13T00:00:20Z",
                ProbeRole::External,
                "192.0.2.1:443",
                ProbeOutcome::Timeout,
            ),
        ];
        let findings =
            analyze_incidents(&events, &incidents(&events), AnalysisConfig::default()).unwrap();
        assert_eq!(
            findings[0].classification,
            FindingClassification::DnsQueryFailureWithIndependentTcpSuccess
        );
        assert_eq!(
            findings[0].evidence_event_ids,
            ["incident-fail-1", "incident-fail-2", "dns-fail"]
        );
        assert!(findings[0]
            .explanation
            .contains("does not establish that DNS caused"));
        assert!(findings[0]
            .suggested_follow_up
            .contains("configured FQDN is the intended name"));
        assert!(findings[0]
            .suggested_follow_up
            .contains("describes only that query"));
    }
}
