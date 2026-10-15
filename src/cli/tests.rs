use super::render::*;
use super::types::StatusSummary;
use super::*;
use crate::event::{
    DnsOutcome, Event, EventKind, KernelLinkSignalCategory, ProbeMethod, ProbeOutcome, ProbeRole,
    SCHEMA_VERSION,
};
use crate::incidents::analyze_events;
use crate::probes::ProbeTarget;
use crate::storage::{EventStore, RetentionPolicy};
use crate::{replay, report, storage};
use std::fs::File;
use std::path::PathBuf;

#[test]
fn probe_target_parser_accepts_numeric_ipv4_and_ipv6() {
    assert_eq!(
        parse_probe_target("gateway", "192.0.2.1:443").unwrap(),
        ProbeTarget {
            role: ProbeRole::Gateway,
            address: "192.0.2.1:443".parse().unwrap(),
        }
    );
    assert_eq!(
        parse_probe_target("external", "[2001:db8::1]:443")
            .unwrap()
            .address,
        "[2001:db8::1]:443".parse().unwrap()
    );
}

#[test]
fn probe_target_parser_rejects_hostnames_without_resolving_them() {
    assert!(parse_probe_target("external", "example.com:443").is_err());
    assert!(parse_probe_target("local", "192.0.2.1:443").is_err());
}

#[test]
fn probe_config_parser_uses_default_and_rejects_invalid_timeouts() {
    assert_eq!(
        parse_probe_config(None).unwrap().timeout(),
        std::time::Duration::from_millis(crate::probes::DEFAULT_TIMEOUT_MILLIS)
    );
    assert!(parse_probe_config(Some("abc")).is_err());
    assert!(parse_probe_config(Some("0")).is_err());
    assert!(parse_probe_config(Some("30001")).is_err());
}

#[test]
fn monitor_parser_requires_distinct_explicit_external_targets() {
    let args = [
        "--external".to_owned(),
        "192.0.2.1:443".to_owned(),
        "--external".to_owned(),
        "192.0.2.2:443".to_owned(),
        "--interval-ms".to_owned(),
        "5000".to_owned(),
    ];
    let config = parse_monitor_args(&args).unwrap();
    assert_eq!(config.interval, std::time::Duration::from_millis(5000));
    assert_eq!(config.targets.len(), 2);
    assert!(!config.kernel_journal);

    let one_target = args[..2].to_vec();
    assert!(parse_monitor_args(&one_target).is_err());
    let repeated = [
        "--external".to_owned(),
        "192.0.2.1:443".to_owned(),
        "--external".to_owned(),
        "192.0.2.1:443".to_owned(),
    ];
    assert!(parse_monitor_args(&repeated).is_err());
}

#[test]
fn monitor_parser_makes_dns_explicit_and_validates_single_fqdn() {
    let base = ["--external", "192.0.2.1:443", "--external", "192.0.2.2:443"].map(str::to_owned);
    let without_dns = parse_monitor_args(&base).unwrap();
    assert!(without_dns.dns_name.is_none());
    let with_dns = [
        base.to_vec(),
        ["--dns-name", "example.com."].map(str::to_owned).to_vec(),
    ]
    .concat();
    assert_eq!(
        parse_monitor_args(&with_dns)
            .unwrap()
            .dns_name
            .unwrap()
            .as_str(),
        "example.com."
    );
    let relative = [
        base.to_vec(),
        ["--dns-name", "example.com"].map(str::to_owned).to_vec(),
    ]
    .concat();
    assert!(parse_monitor_args(&relative).is_err());
    let duplicate = [
        with_dns,
        ["--dns-name", "other.example."].map(str::to_owned).to_vec(),
    ]
    .concat();
    assert!(parse_monitor_args(&duplicate).is_err());
}

#[test]
fn kernel_journal_collection_requires_one_explicit_flag() {
    let base = ["--external", "192.0.2.1:443", "--external", "192.0.2.2:443"].map(str::to_owned);
    let enabled = [
        base.to_vec(),
        ["--kernel-journal"].map(str::to_owned).to_vec(),
    ]
    .concat();
    assert!(parse_monitor_args(&enabled).unwrap().kernel_journal);
    let duplicate = [enabled, ["--kernel-journal"].map(str::to_owned).to_vec()].concat();
    assert!(parse_monitor_args(&duplicate).is_err());
}

#[test]
fn report_parser_defaults_to_sanitized_text_and_validates_options() {
    let latest = ["--latest".to_owned()];
    assert_eq!(
        parse_report_args(&latest).unwrap(),
        ReportOptions {
            format: ReportFormat::Text,
            include_identifiers: false,
            output: None,
        }
    );
    let json = ["--latest", "--format", "json"];
    let json = json.map(str::to_owned);
    assert_eq!(parse_report_args(&json).unwrap().format, ReportFormat::Json);
    let output = ["--latest", "--output", "report.txt"].map(str::to_owned);
    assert_eq!(
        parse_report_args(&output).unwrap().output,
        Some(PathBuf::from("report.txt"))
    );
    let raw = ["--latest", "--include-identifiers"].map(str::to_owned);
    assert!(parse_report_args(&raw).unwrap().include_identifiers);
    for args in [
        vec!["--format".into(), "json".into()],
        vec!["--latest".into(), "--format".into(), "yaml".into()],
        vec!["--latest".into(), "--unknown".into()],
        vec!["--latest".into(), "--format".into()],
        vec![
            "--latest".into(),
            "--include-identifiers".into(),
            "--include-identifiers".into(),
        ],
        vec!["--latest".into(), "--output".into()],
        vec![
            "--latest".into(),
            "--output".into(),
            "report.txt".into(),
            "--output".into(),
            "second.txt".into(),
        ],
    ] {
        assert!(parse_report_args(&args).is_err());
    }
}

#[test]
fn incident_filter_parser_validates_timestamps_target_and_duplicates() {
    let valid = [
        "--since",
        "2026-10-13T12:00:00-05:00",
        "--until",
        "2026-10-13T18:00:00Z",
        "--target",
        "192.0.2.8:443",
        "--compare",
    ]
    .map(str::to_owned);
    let options = parse_incident_args(&valid).unwrap();
    assert_eq!(
        options.since.unwrap().to_rfc3339(),
        "2026-10-13T17:00:00+00:00"
    );
    assert_eq!(options.target.unwrap(), "192.0.2.8:443".parse().unwrap());
    assert!(options.compare);

    for invalid in [
        vec!["--since".into(), "yesterday".into()],
        vec!["--until".into()],
        vec!["--target".into(), "example.com:443".into()],
        vec!["--compare".into(), "--compare".into()],
        vec![
            "--since".into(),
            "2026-10-14T00:00:00Z".into(),
            "--until".into(),
            "2026-10-13T00:00:00Z".into(),
        ],
        vec![
            "--target".into(),
            "192.0.2.1:443".into(),
            "--target".into(),
            "192.0.2.2:443".into(),
        ],
    ] {
        assert!(
            parse_incident_args(&invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
fn incident_filters_are_inclusive_exact_and_comparison_uses_target_probes() {
    let events = vec![
        history_event_for(
            "before-a",
            "192.0.2.8:443",
            ProbeOutcome::Success,
            "2026-10-13T12:00:00Z",
        ),
        history_event_for(
            "fail-a1",
            "192.0.2.8:443",
            ProbeOutcome::Timeout,
            "2026-10-13T12:00:10Z",
        ),
        history_event_for(
            "fail-a2",
            "192.0.2.8:443",
            ProbeOutcome::Timeout,
            "2026-10-13T12:00:20Z",
        ),
        history_event_for(
            "recover-a1",
            "192.0.2.8:443",
            ProbeOutcome::Success,
            "2026-10-13T12:00:30Z",
        ),
        history_event_for(
            "recover-a2",
            "192.0.2.8:443",
            ProbeOutcome::Success,
            "2026-10-13T12:00:40Z",
        ),
        history_event_for(
            "fail-b1",
            "192.0.2.9:443",
            ProbeOutcome::Timeout,
            "2026-10-13T12:01:00Z",
        ),
        history_event_for(
            "fail-b2",
            "192.0.2.9:443",
            ProbeOutcome::Timeout,
            "2026-10-13T12:01:10Z",
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, event)| storage::StoredEvent {
        sequence: index as i64 + 1,
        event,
    })
    .collect::<Vec<_>>();
    let incidents = analyze_events(&events, crate::incidents::IncidentConfig::default()).unwrap();
    assert_eq!(incidents.len(), 2);

    let start = "2026-10-13T12:00:10Z".parse().unwrap();
    let filtered = filter_incidents(
        &incidents,
        IncidentOptions {
            since: Some(start),
            until: Some(start),
            target: Some("192.0.2.8:443".parse().unwrap()),
            compare: false,
        },
    );
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, incidents[0].id);

    let output = render_incident_history(
        &events,
        &incidents,
        IncidentOptions {
            compare: true,
            ..IncidentOptions::default()
        },
    );
    assert!(output.contains("Before incident: success"));
    assert!(output.contains("After recovery: success"));
    assert!(output.contains("After recovery: unavailable (incident remains open)"));
    assert!(output.contains("same configured target; not a cause"));
}

#[test]
fn monitor_parser_accepts_and_validates_retention_options() {
    let args = [
        "--external".to_owned(),
        "192.0.2.1:443".to_owned(),
        "--external".to_owned(),
        "192.0.2.2:443".to_owned(),
        "--retention-days".to_owned(),
        "7".to_owned(),
        "--max-events".to_owned(),
        "500".to_owned(),
    ];
    let config = parse_monitor_args(&args).unwrap();
    assert_eq!(config.retention.retention_days(), 7);
    assert_eq!(config.retention.max_events(), 500);

    for invalid in ["0", "3651", "abc"] {
        let mut args = args.to_vec();
        let index = args.iter().position(|value| value == "7").unwrap();
        args[index] = invalid.to_owned();
        assert!(parse_monitor_args(&args).is_err());
    }
    for invalid in ["0", "1000001", "abc"] {
        let mut args = args.to_vec();
        let index = args.iter().position(|value| value == "500").unwrap();
        args[index] = invalid.to_owned();
        assert!(parse_monitor_args(&args).is_err());
    }
}

fn history_event(id: String, outcome: ProbeOutcome) -> Event {
    let at = "2026-10-13T00:00:00Z".parse().unwrap();
    Event {
        schema_version: SCHEMA_VERSION,
        id,
        recorded_at_utc: at,
        source: "probe".to_owned(),
        kind: EventKind::ProbeResult {
            role: ProbeRole::External,
            target: "192.0.2.8:443".parse().unwrap(),
            method: ProbeMethod::TcpConnect,
            started_at_utc: at,
            elapsed_ms: 10,
            outcome,
        },
    }
}

fn history_event_for(id: &str, target: &str, outcome: ProbeOutcome, at: &str) -> Event {
    let at = at.parse().unwrap();
    let mut event = history_event(id.to_owned(), outcome);
    event.recorded_at_utc = at;
    if let EventKind::ProbeResult {
        target: event_target,
        started_at_utc,
        ..
    } = &mut event.kind
    {
        *event_target = target.parse().unwrap();
        *started_at_utc = at;
    }
    event
}

fn status_event(id: &str, kind: EventKind) -> Event {
    Event {
        schema_version: SCHEMA_VERSION,
        id: id.to_owned(),
        recorded_at_utc: "2026-10-13T00:00:00Z".parse().unwrap(),
        source: "test".to_owned(),
        kind,
    }
}

#[test]
fn status_reports_empty_history_without_claiming_monitor_state() {
    let output = render_status(&StatusSummary::default());
    assert!(output.contains("No observations are stored yet"));
    assert!(output.contains("Monitor process: not running"));
    assert!(output.contains("Monitor configuration: no local monitor record"));
    assert!(output.contains("DNS: no query event retained"));
    assert!(output.contains("Kernel journal: no normalized link signal retained"));
}

#[test]
fn status_summarizes_stored_events_read_only_and_omits_dns_query_names() {
    let mut store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
    store
        .store_events_at(
            &[
                status_event(
                    "interface",
                    EventKind::InterfaceSnapshot {
                        interface: "eth-test".to_owned(),
                        operational_state: "unknown".to_owned(),
                        carrier: None,
                        speed_mbps: None,
                        is_default_route: None,
                    },
                ),
                status_event(
                    "counters",
                    EventKind::InterfaceCounterDelta {
                        interface: "eth-test".to_owned(),
                        baseline_available: false,
                        sample_available: false,
                        counter_reset_detected: false,
                        rx_errors_delta: None,
                        tx_errors_delta: None,
                        rx_dropped_delta: None,
                        tx_dropped_delta: None,
                    },
                ),
                history_event("probe".to_owned(), ProbeOutcome::Timeout),
                status_event(
                    "dns",
                    EventKind::DnsResult {
                        query_name: "private.example.".to_owned(),
                        started_at_utc: "2026-10-13T00:00:00Z".parse().unwrap(),
                        elapsed_ms: 12,
                        outcome: DnsOutcome::Success { answer_count: 1 },
                    },
                ),
                status_event(
                    "kernel",
                    EventKind::KernelLinkSignal {
                        category: KernelLinkSignalCategory::LinkDown,
                        interface: Some("eth-test".to_owned()),
                    },
                ),
            ],
            "2026-10-13T00:00:00Z".parse().unwrap(),
        )
        .unwrap();

    let count_before = store.event_count().unwrap();
    let status = load_status(&store).unwrap();
    let output = render_status(&status);
    assert_eq!(status.latest_event.as_ref().unwrap().0, 5);
    assert_eq!(store.event_count().unwrap(), count_before);
    assert!(output.contains("Retained observations: 5"));
    assert!(output.contains("carrier=unknown, speed=unknown, default_route=unknown"));
    assert!(output.contains("sample=unavailable"));
    assert!(output.contains("Latest probe: External 192.0.2.8:443 — timeout"));
    assert!(output.contains("Latest DNS observation: success (1 answers)"));
    assert!(!output.contains("private.example."));
    assert!(output.contains("Latest kernel link signal: link_down"));
    assert!(output.contains("Monitor process: not running"));
}

#[test]
fn incident_history_reports_empty_database_clearly() {
    let store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
    let incidents = analyze_events(
        &load_events(&store).unwrap(),
        crate::incidents::IncidentConfig::default(),
    )
    .unwrap();
    assert!(incidents.is_empty());
    assert_eq!(
        render_incidents(&incidents),
        "No probe incidents found in local event history."
    );
}

#[test]
fn analysis_history_uses_paged_stored_events_without_writing() {
    let mut store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
    store
        .store_events_at(
            &[
                history_event("fail-a".to_owned(), ProbeOutcome::Timeout),
                history_event("fail-b".to_owned(), ProbeOutcome::Timeout),
            ],
            "2026-10-13T00:00:00Z".parse().unwrap(),
        )
        .unwrap();
    let before = store.event_count().unwrap();
    let history = load_analysis(&store).unwrap();
    assert_eq!(history.incidents.len(), 1);
    assert_eq!(history.findings.len(), 1);
    assert_eq!(store.event_count().unwrap(), before);
    let output = report::render_analysis(&history.findings);
    assert!(output.contains("undetermined"));
    assert!(output.contains("fail-a, fail-b"));
}

#[test]
fn replay_reads_bounded_fixture_and_renders_offline_findings() {
    let fixture = include_str!("../../tests/fixtures/endpoint-specific/events.json");
    let path = env::temp_dir().join(format!(
        "netforensics-replay-test-{}-{}.json",
        std::process::id(),
        next_event_id()
    ));
    std::fs::write(&path, fixture).unwrap();
    let bytes = read_bounded(File::open(&path).unwrap(), replay::MAX_FIXTURE_BYTES).unwrap();
    std::fs::remove_file(path).unwrap();
    let result = replay::replay_json(std::str::from_utf8(&bytes).unwrap()).unwrap();
    let output = render_replay(&result);
    assert!(output.contains("Offline fixture replay"));
    assert!(output.contains("Probe incidents:"));
    assert!(output.contains("Findings:"));
    assert!(output.contains("endpoint_specific_evidence"));
    assert!(output.contains("endpoint-a-fail-1, endpoint-a-fail-2, endpoint-b-success"));
}

#[test]
fn replay_file_reader_rejects_input_over_limit_without_reading_it_all() {
    let error = read_bounded(&b"12345"[..], 4).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn guided_setup_validates_targets_and_prints_reviewable_command() {
    let answers = "example.com:443\n192.0.2.1:443\n192.0.2.1:8443\n198.51.100.2:443\n192.0.2.1:443\ninvalid\n[2001:db8::1]:443\n";
    let mut input = io::Cursor::new(answers);
    let mut output = Vec::new();
    run_setup(&mut input, &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("setup makes no network requests"));
    assert!(output.contains("once per round, sequentially, with no application retries"));
    assert!(output.contains("Increase --interval-ms to reduce request frequency"));
    assert!(output.contains("does not verify the active gateway"));
    assert!(output.contains("Choose an external target with a different IP address."));
    assert!(output.contains("Choose a gateway-role IP:PORT different from the external targets."));
    assert!(output.contains("netforensics monitor --external '192.0.2.1:443' --external '198.51.100.2:443' --gateway '[2001:db8::1]:443'"));
    assert!(output.contains("setup will not start monitoring"));
}

#[test]
fn help_explains_probe_target_policy_and_cadence() {
    assert!(HELP.contains("not the per-target probe period"));
    assert!(HELP.contains("increase --interval-ms to reduce request frequency"));
    assert!(HELP.contains("respect target owners' policies"));
}

#[test]
fn guided_setup_reports_end_of_input_without_starting_monitor() {
    let mut input = io::Cursor::new("");
    let mut output = Vec::new();
    let error = run_setup(&mut input, &mut output).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert!(String::from_utf8(output)
        .unwrap()
        .contains("setup makes no network requests"));
}

#[test]
fn incident_evidence_crosses_stored_event_page_boundary() {
    let mut store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
    let mut events = (0..999)
        .map(|index| history_event(format!("ok-{index}"), ProbeOutcome::Success))
        .collect::<Vec<_>>();
    events.push(history_event("fail-a".to_owned(), ProbeOutcome::Timeout));
    events.push(history_event("fail-b".to_owned(), ProbeOutcome::Timeout));
    store
        .store_events_at(&events, "2026-10-13T00:00:00Z".parse().unwrap())
        .unwrap();

    let incidents = analyze_events(
        &load_events(&store).unwrap(),
        crate::incidents::IncidentConfig::default(),
    )
    .unwrap();
    assert_eq!(incidents.len(), 1);
    let output = render_incidents(&incidents);
    assert!(output.contains("incident-1000 [OPEN]"));
    assert!(output.contains("Target: External 192.0.2.8:443"));
    assert!(output.contains("Failure evidence: fail-a, fail-b"));
}
