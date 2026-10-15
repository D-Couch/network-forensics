use super::args::IncidentOptions;
use super::types::StatusSummary;
use crate::event::{
    DnsOutcome, EventKind, KernelLinkSignalCategory, ProbeFailureCategory, ProbeLimitationCategory,
    ProbeOutcome,
};
use crate::incidents::{Incident, IncidentStatus};
use crate::{monitor_state, replay, report, storage};

pub(super) fn render_status(summary: &StatusSummary) -> String {
    use std::fmt::Write as _;
    let mut output = String::from("Local status (retained observation history)\n");
    if summary.event_count == 0 {
        output.push_str(
            "No observations are stored yet. Start `netforensics monitor` to collect them.\n",
        );
    } else {
        writeln!(output, "Retained observations: {}", summary.event_count).unwrap();
        if let Some((sequence, recorded_at)) = &summary.latest_event {
            writeln!(
                output,
                "Latest observation: {} (sequence {})",
                recorded_at, sequence
            )
            .unwrap();
        }
    }

    monitor_state::render_status(&mut output, &summary.monitor);

    if let Some(probe) = &summary.latest_probe {
        if let EventKind::ProbeResult {
            role,
            target,
            outcome,
            ..
        } = &probe.event.kind
        {
            writeln!(
                output,
                "Latest probe: {role:?} {target} — {} at {}",
                describe_probe_outcome(outcome),
                probe.event.recorded_at_utc
            )
            .unwrap();
            output.push_str(
                "This is the last recorded probe, not a complete list of configured targets.\n",
            );
        }
    } else {
        output.push_str("Probe observations: none retained.\n");
    }

    render_latest_interface(&mut output, &summary.latest_interface);
    render_latest_counters(&mut output, &summary.latest_counters);

    match &summary.latest_dns {
        Some(stored) => {
            if let EventKind::DnsResult { outcome, .. } = &stored.event.kind {
                writeln!(
                    output,
                    "Latest DNS observation: {} at {} (query name omitted)",
                    describe_dns_outcome(outcome),
                    stored.event.recorded_at_utc
                )
                .unwrap();
            }
        }
        None => output
            .push_str("DNS: no query event retained (sampling may not have been configured).\n"),
    }

    match &summary.latest_kernel_signal {
        Some(stored) => {
            if let EventKind::KernelLinkSignal {
                category,
                interface,
            } = &stored.event.kind
            {
                let category = match category {
                    KernelLinkSignalCategory::LinkUp => "link_up",
                    KernelLinkSignalCategory::LinkDown => "link_down",
                    KernelLinkSignalCategory::NetdevWatchdog => "netdev_watchdog",
                };
                let interface = interface
                    .as_deref()
                    .map(|name| name.escape_debug().to_string())
                    .unwrap_or_else(|| "unknown".to_owned());
                writeln!(
                    output,
                    "Latest kernel link signal: {category} on {interface} at {}",
                    stored.event.recorded_at_utc
                )
                .unwrap();
            }
        }
        None => output.push_str("Kernel journal: no normalized link signal retained (collection may be disabled, unavailable, or have no matching messages).\n"),
    }

    output
}

pub(super) fn render_latest_interface(output: &mut String, latest: &Option<storage::StoredEvent>) {
    use std::fmt::Write as _;
    let Some(stored) = latest else {
        output.push_str("Interface snapshots: none retained.\n");
        return;
    };
    if let EventKind::InterfaceSnapshot {
        interface,
        operational_state,
        carrier,
        speed_mbps,
        is_default_route,
    } = &stored.event.kind
    {
        writeln!(
            output,
            "Latest interface snapshot: {} state={}, carrier={}, speed={}, default_route={} at {}",
            interface.escape_debug(),
            operational_state.escape_debug(),
            optional_bool(*carrier),
            optional_value(*speed_mbps, " Mbps"),
            optional_bool(*is_default_route),
            stored.event.recorded_at_utc
        )
        .unwrap();
    }
}

pub(super) fn render_latest_counters(output: &mut String, latest: &Option<storage::StoredEvent>) {
    use std::fmt::Write as _;
    let Some(stored) = latest else {
        output.push_str("Counter samples: none retained.\n");
        return;
    };
    if let EventKind::InterfaceCounterDelta {
        interface,
        sample_available,
        counter_reset_detected,
        rx_errors_delta,
        tx_errors_delta,
        rx_dropped_delta,
        tx_dropped_delta,
        ..
    } = &stored.event.kind
    {
        let availability = if *sample_available {
            "available"
        } else {
            "unavailable"
        };
        writeln!(
            output,
            "Latest counter sample: {} sample={}, reset_detected={}, rx_errors={}, tx_errors={}, rx_dropped={}, tx_dropped={} at {}",
            interface.escape_debug(),
            availability,
            counter_reset_detected,
            optional_value(*rx_errors_delta, ""),
            optional_value(*tx_errors_delta, ""),
            optional_value(*rx_dropped_delta, ""),
            optional_value(*tx_dropped_delta, ""),
            stored.event.recorded_at_utc
        )
        .unwrap();
    }
}

pub(super) fn optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

pub(super) fn optional_value<T: std::fmt::Display>(value: Option<T>, suffix: &str) -> String {
    value
        .map(|value| format!("{value}{suffix}"))
        .unwrap_or_else(|| "unknown".to_owned())
}

pub(super) fn describe_probe_outcome(outcome: &ProbeOutcome) -> String {
    match outcome {
        ProbeOutcome::Success => "success".to_owned(),
        ProbeOutcome::Timeout => "timeout".to_owned(),
        ProbeOutcome::Failure(category) => format!("failure ({})", failure_name(*category)),
        ProbeOutcome::UnsupportedOrBlocked(category) => {
            format!("unsupported or blocked ({})", limitation_name(*category))
        }
    }
}

pub(super) fn describe_dns_outcome(outcome: &DnsOutcome) -> String {
    match outcome {
        DnsOutcome::Success { answer_count } => format!("success ({answer_count} answers)"),
        DnsOutcome::Timeout => "timeout".to_owned(),
        DnsOutcome::Failure(category) => format!("failure ({category:?})"),
        DnsOutcome::UnsupportedOrBlocked(category) => format!("unavailable ({category:?})"),
    }
}

pub(super) fn render_replay(result: &replay::ReplayResult) -> String {
    let mut output = String::from("Offline fixture replay\n\nProbe incidents:\n");
    if result.incidents.is_empty() {
        output.push_str("No qualifying probe incidents in fixture.\n");
    } else {
        output.push_str(&render_incidents(&result.incidents));
        output.push('\n');
    }
    output.push_str("\nFindings:\n");
    if result.findings.is_empty() {
        output.push_str("No findings for this fixture.\n");
    } else {
        output.push_str(&report::render_analysis(&result.findings));
    }
    output
}

pub(super) fn render_incidents(incidents: &[Incident]) -> String {
    if incidents.is_empty() {
        return "No probe incidents found in local event history.".to_owned();
    }
    use std::fmt::Write as _;
    let mut output = String::new();
    for (index, incident) in incidents.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        let status = match incident.status {
            IncidentStatus::Open => "OPEN",
            IncidentStatus::Recovered => "RECOVERED",
        };
        writeln!(output, "{} [{status}]", incident.id).unwrap();
        writeln!(
            output,
            "  Target: {:?} {}",
            incident.target.role, incident.target.address
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
            format_evidence_ids(&incident.failure_event_ids)
        )
        .unwrap();
        if let (Some(recovered_at), Some(sequence)) =
            (incident.recovered_at_utc, incident.recovered_sequence)
        {
            writeln!(output, "  Recovered: {recovered_at} (sequence {sequence})").unwrap();
            writeln!(
                output,
                "  Recovery evidence: {}",
                format_evidence_ids(&incident.recovery_event_ids)
            )
            .unwrap();
        }
        if incident.contains_observation_gap {
            writeln!(
                output,
                "  Uncertainty: observation gap while incident was open"
            )
            .unwrap();
        }
        if incident.contains_clock_adjustment {
            writeln!(
                output,
                "  Uncertainty: event timestamps moved backwards while incident was open"
            )
            .unwrap();
        }
    }
    output
}

pub(super) fn render_incident_history(
    events: &[storage::StoredEvent],
    incidents: &[Incident],
    options: IncidentOptions,
) -> String {
    if incidents.is_empty() {
        let filters_active =
            options.since.is_some() || options.until.is_some() || options.target.is_some();
        return if filters_active {
            "No probe incidents match the selected filters.".to_owned()
        } else {
            render_incidents(incidents)
        };
    }

    let mut output = render_incidents(incidents);
    if !options.compare {
        return output;
    }
    for incident in incidents {
        use std::fmt::Write as _;
        writeln!(
            output,
            "\n{} probe comparison (same configured target; not a cause):",
            incident.id
        )
        .unwrap();
        let before = events.iter().rev().find(|stored| {
            stored.sequence < incident.started_sequence
                && target_probe_outcome(stored, incident).is_some()
        });
        match before {
            Some(stored) => writeln!(
                output,
                "  Before incident: {}",
                render_probe_comparison_event(stored, incident)
            )
            .unwrap(),
            None => writeln!(
                output,
                "  Before incident: no earlier probe for this target retained"
            )
            .unwrap(),
        }

        if incident.status == IncidentStatus::Open {
            writeln!(
                output,
                "  After recovery: unavailable (incident remains open)"
            )
            .unwrap();
        } else if let Some(first_recovery_id) = incident.recovery_event_ids.first() {
            if let Some(stored) = events
                .iter()
                .find(|stored| &stored.event.id == first_recovery_id)
            {
                writeln!(
                    output,
                    "  After recovery: {}",
                    render_probe_comparison_event(stored, incident)
                )
                .unwrap();
            } else {
                writeln!(
                    output,
                    "  After recovery: recovery observation is no longer retained"
                )
                .unwrap();
            }
        }
    }
    output
}

pub(super) fn target_probe_outcome<'a>(
    stored: &'a storage::StoredEvent,
    incident: &Incident,
) -> Option<&'a ProbeOutcome> {
    match &stored.event.kind {
        EventKind::ProbeResult {
            role,
            target,
            outcome,
            ..
        } if *role == incident.target.role && *target == incident.target.address => Some(outcome),
        _ => None,
    }
}

pub(super) fn render_probe_comparison_event(
    stored: &storage::StoredEvent,
    incident: &Incident,
) -> String {
    let outcome = target_probe_outcome(stored, incident).expect("matching probe event");
    format!(
        "{} at {} (sequence {}, event {})",
        describe_probe_outcome(outcome),
        stored.event.recorded_at_utc,
        stored.sequence,
        format_evidence_ids(std::slice::from_ref(&stored.event.id))
    )
}

pub(super) fn format_evidence_ids(ids: &[String]) -> String {
    if ids.is_empty() {
        return "none".to_owned();
    }
    ids.iter()
        .map(|id| id.escape_debug().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn failure_name(category: ProbeFailureCategory) -> &'static str {
    match category {
        ProbeFailureCategory::ConnectionRefused => "connection refused",
        ProbeFailureCategory::NetworkUnreachable => "network unreachable",
        ProbeFailureCategory::HostUnreachable => "host unreachable",
        ProbeFailureCategory::Other => "other network error",
    }
}

pub(super) fn limitation_name(category: ProbeLimitationCategory) -> &'static str {
    match category {
        ProbeLimitationCategory::PermissionDenied => "permission denied",
        ProbeLimitationCategory::MethodUnsupported => "method unsupported",
    }
}
