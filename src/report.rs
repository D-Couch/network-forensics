//! Pure, versioned rendering of stored incident analysis.

use crate::analysis::{Finding, FindingClassification};
use crate::event::ProbeRole;
use crate::incidents::{Incident, IncidentStatus};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::net::SocketAddr;

pub const REPORT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticReport {
    pub report_schema_version: u16,
    pub identifiers_included: bool,
    pub incidents: Vec<ReportIncident>,
    pub findings: Vec<ReportFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportIncident {
    pub id: String,
    pub status: &'static str,
    pub target_role: &'static str,
    pub target: String,
    pub started_sequence: i64,
    pub last_failure_sequence: i64,
    pub recovered_sequence: Option<i64>,
    pub started_at_utc: DateTime<Utc>,
    pub last_failure_at_utc: DateTime<Utc>,
    pub recovered_at_utc: Option<DateTime<Utc>>,
    pub failure_event_ids: Vec<String>,
    pub recovery_event_ids: Vec<String>,
    pub contains_observation_gap: bool,
    pub contains_clock_adjustment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportFinding {
    pub incident_id: String,
    pub classification: &'static str,
    pub explanation: String,
    pub evidence_event_ids: Vec<String>,
    pub limitations: Vec<String>,
    pub suggested_follow_up: String,
}

/// Builds a shareable report. Identical targets receive stable labels within
/// one report; raw target addresses are present only when explicitly enabled.
pub fn build_report(
    incidents: &[Incident],
    findings: &[Finding],
    include_identifiers: bool,
) -> DiagnosticReport {
    let mut targets: Vec<(SocketAddr, ProbeRole, String)> = Vec::new();
    let mut incidents = incidents.to_vec();
    incidents.sort_by_key(|incident| incident.started_sequence);
    let incidents = incidents
        .iter()
        .map(|incident| {
            let label = if include_identifiers {
                incident.target.address.to_string()
            } else if let Some((_, _, label)) = targets.iter().find(|(address, role, _)| {
                *address == incident.target.address && *role == incident.target.role
            }) {
                label.clone()
            } else {
                let count = targets
                    .iter()
                    .filter(|(_, role, _)| *role == incident.target.role)
                    .count()
                    + 1;
                let prefix = role_label(incident.target.role);
                let label = format!("{prefix}-target-{count}");
                targets.push((incident.target.address, incident.target.role, label.clone()));
                label
            };
            ReportIncident {
                id: incident.id.clone(),
                status: match incident.status {
                    IncidentStatus::Open => "open",
                    IncidentStatus::Recovered => "recovered",
                },
                target_role: role_label(incident.target.role),
                target: label,
                started_sequence: incident.started_sequence,
                last_failure_sequence: incident.last_failure_sequence,
                recovered_sequence: incident.recovered_sequence,
                started_at_utc: incident.started_at_utc,
                last_failure_at_utc: incident.last_failure_at_utc,
                recovered_at_utc: incident.recovered_at_utc,
                failure_event_ids: incident.failure_event_ids.clone(),
                recovery_event_ids: incident.recovery_event_ids.clone(),
                contains_observation_gap: incident.contains_observation_gap,
                contains_clock_adjustment: incident.contains_clock_adjustment,
            }
        })
        .collect();
    let findings = findings
        .iter()
        .map(|finding| ReportFinding {
            incident_id: finding.incident_id.clone(),
            classification: classification_label(finding.classification),
            explanation: finding.explanation.clone(),
            evidence_event_ids: finding.evidence_event_ids.clone(),
            limitations: finding.limitations.clone(),
            suggested_follow_up: finding.suggested_follow_up.clone(),
        })
        .collect();
    DiagnosticReport {
        report_schema_version: REPORT_SCHEMA_VERSION,
        identifiers_included: include_identifiers,
        incidents,
        findings,
    }
}

pub fn render_json(report: &DiagnosticReport) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}

pub fn render_analysis(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "No probe incidents found in local event history.".to_owned();
    }
    let mut output = String::new();
    for (index, finding) in findings.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        use std::fmt::Write as _;
        writeln!(
            output,
            "{}: {}",
            finding.incident_id,
            classification_label(finding.classification)
        )
        .unwrap();
        writeln!(output, "  {}", finding.explanation).unwrap();
        writeln!(
            output,
            "  Evidence: {}",
            format_ids(&finding.evidence_event_ids)
        )
        .unwrap();
        writeln!(output, "  Limitations: {}", finding.limitations.join(" ")).unwrap();
        writeln!(output, "  Follow-up: {}", finding.suggested_follow_up).unwrap();
    }
    output
}

pub fn render_text(report: &DiagnosticReport) -> String {
    if report.incidents.is_empty() {
        return "No probe incidents found in local event history.".to_owned();
    }
    use std::fmt::Write as _;
    let mut output = String::new();
    if !report.identifiers_included {
        writeln!(output, "Target identifiers are redacted; use --include-identifiers to show configured addresses.").unwrap();
    }
    for (index, incident) in report.incidents.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        writeln!(
            output,
            "{} [{}]",
            incident.id,
            incident.status.to_uppercase()
        )
        .unwrap();
        writeln!(
            output,
            "  Target: {} {}",
            incident.target_role, incident.target
        )
        .unwrap();
        writeln!(
            output,
            "  Started: {} (sequence {})",
            incident.started_at_utc, incident.started_sequence
        )
        .unwrap();
        writeln!(
            output,
            "  Last failure: {} (sequence {})",
            incident.last_failure_at_utc, incident.last_failure_sequence
        )
        .unwrap();
        writeln!(
            output,
            "  Failure evidence: {}",
            format_ids(&incident.failure_event_ids)
        )
        .unwrap();
        if let (Some(recovered_at), Some(sequence)) =
            (incident.recovered_at_utc, incident.recovered_sequence)
        {
            writeln!(output, "  Recovered: {recovered_at} (sequence {sequence})").unwrap();
            writeln!(
                output,
                "  Recovery evidence: {}",
                format_ids(&incident.recovery_event_ids)
            )
            .unwrap();
        }
        if incident.contains_observation_gap {
            writeln!(output, "  Uncertainty: observation gap while open").unwrap();
        }
        if incident.contains_clock_adjustment {
            writeln!(
                output,
                "  Uncertainty: event timestamps moved backwards while open"
            )
            .unwrap();
        }
        if let Some(finding) = report
            .findings
            .iter()
            .find(|finding| finding.incident_id == incident.id)
        {
            writeln!(output, "  Finding: {}", finding.classification).unwrap();
            writeln!(output, "  {}", finding.explanation).unwrap();
            writeln!(
                output,
                "  Corroborating evidence: {}",
                format_ids(&finding.evidence_event_ids)
            )
            .unwrap();
            for limitation in &finding.limitations {
                writeln!(output, "  Limitation: {limitation}").unwrap();
            }
            writeln!(output, "  Follow-up: {}", finding.suggested_follow_up).unwrap();
        }
    }
    output
}

fn role_label(role: ProbeRole) -> &'static str {
    match role {
        ProbeRole::Gateway => "gateway",
        ProbeRole::External => "external",
    }
}

fn classification_label(classification: FindingClassification) -> &'static str {
    match classification {
        FindingClassification::LocalLinkRelatedEvidence => "local_link_related_evidence",
        FindingClassification::ConfiguredGatewayTargetFailure => {
            "configured_gateway_target_failure"
        }
        FindingClassification::MultipleExternalTargetsFailed => "multiple_external_targets_failed",
        FindingClassification::EndpointSpecificEvidence => "endpoint_specific_evidence",
        FindingClassification::DnsQueryFailureWithIndependentTcpSuccess => {
            "dns_query_failure_with_independent_tcp_success"
        }
        FindingClassification::Undetermined => "undetermined",
    }
}

fn format_ids(ids: &[String]) -> String {
    if ids.is_empty() {
        return "none".to_owned();
    }
    ids.iter()
        .map(|id| id.escape_debug().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::FindingClassification;
    use crate::probes::ProbeTarget;

    fn incident(id: &str, sequence: i64, address: &str) -> Incident {
        let at: DateTime<Utc> = "2026-10-13T00:00:00Z".parse().unwrap();
        Incident {
            id: id.into(),
            target: ProbeTarget {
                role: ProbeRole::External,
                address: address.parse().unwrap(),
            },
            started_sequence: sequence,
            started_at_utc: at,
            last_failure_sequence: sequence + 1,
            last_failure_at_utc: at,
            failure_event_ids: vec![format!("event-{sequence}")],
            status: IncidentStatus::Open,
            recovered_at_utc: None,
            recovered_sequence: None,
            recovery_event_ids: Vec::new(),
            contains_observation_gap: true,
            contains_clock_adjustment: false,
        }
    }

    fn finding(incident_id: &str) -> Finding {
        Finding {
            incident_id: incident_id.into(),
            classification: FindingClassification::Undetermined,
            explanation: "Evidence is insufficient.".into(),
            evidence_event_ids: vec!["event-1".into()],
            limitations: vec!["A limitation.".into()],
            suggested_follow_up: "Collect another observation.".into(),
        }
    }

    #[test]
    fn default_report_redacts_consistently_and_keeps_evidence() {
        let incidents = [
            incident("i-1", 1, "192.0.2.1:443"),
            incident("i-2", 3, "192.0.2.1:443"),
            incident("i-3", 5, "192.0.2.2:443"),
        ];
        let report = build_report(&incidents, &[finding("i-1")], false);
        assert!(!report.identifiers_included);
        assert_eq!(report.incidents[0].target, "external-target-1");
        assert_eq!(report.incidents[0].started_sequence, 1);
        assert_eq!(report.incidents[1].target, "external-target-1");
        assert_eq!(report.incidents[2].target, "external-target-2");
        assert_eq!(report.findings[0].evidence_event_ids, ["event-1"]);
        let json = render_json(&report).unwrap();
        assert!(!json.contains("192.0.2.1"));
        assert!(json.contains("external-target-1"));
        assert!(json.contains("evidence_event_ids"));
        assert!(render_text(&report).contains("observation gap"));
    }

    #[test]
    fn raw_target_is_included_only_when_requested_and_empty_history_is_clear() {
        let incident = incident("i-1", 1, "192.0.2.1:443");
        assert!(render_json(&build_report(&[], &[], false))
            .unwrap()
            .contains("\"incidents\": []"));
        let raw = build_report(&[incident], &[], true);
        assert!(raw.identifiers_included);
        assert_eq!(raw.incidents[0].target, "192.0.2.1:443");
    }
}
