pub mod analysis;
pub mod dns;
pub mod event;
pub mod incidents;
pub mod interfaces;
pub mod journal;
pub mod monitor;
pub mod probes;
pub mod replay;
pub mod report;
pub mod storage;

use crate::event::{
    DnsOutcome, Event, EventKind, KernelLinkSignalCategory, ProbeFailureCategory,
    ProbeLimitationCategory, ProbeOutcome, ProbeRole,
};
use crate::incidents::{analyze_events, Incident, IncidentError, IncidentStatus};
use crate::monitor::{MonitorConfig, DEFAULT_INTERVAL_MILLIS};
use crate::probes::{ProbeConfig, ProbeRunner, ProbeTarget, SystemClock, SystemTransport};
use crate::storage::{
    EventStore, RetentionPolicy, StorageError, DEFAULT_MAX_EVENTS, DEFAULT_RETENTION_DAYS,
};
use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const HELP: &str = "Network Forensics (netforensics)\n\nA local network diagnostics tool. Interface inventory, explicit one-shot TCP probes, foreground monitoring, local incident history, analysis, and reports are available.\n\nUsage:\n  netforensics [--help] [--version]\n  netforensics status\n  netforensics interfaces\n  netforensics probe <gateway|external> <IP:PORT> [TIMEOUT_MS]\n  netforensics monitor --external <IP:PORT> --external <IP:PORT> [--gateway <IP:PORT>] [--dns-name <FQDN>] [--kernel-journal] [--interval-ms MS] [--timeout-ms MS] [--retention-days DAYS] [--max-events COUNT]\n  netforensics incidents\n  netforensics analyze --latest\n  netforensics report --latest [--format text|json] [--include-identifiers]\n  netforensics replay <FIXTURE.json>\n\nCommands:\n  status           Summarize latest retained observations (not live monitor state)\n  interfaces       List local interfaces and the detected default route\n  probe            Attempt one TCP connection to the explicitly supplied IP:port\n  monitor          Emit and save interface/probe observations until Ctrl+C\n  incidents        Show incidents derived from locally stored probe events\n  analyze          Analyze the latest retained local event history\n  report           Render a text or JSON report from local event history\n  replay           Analyze a versioned fixture offline\n\nOptions:\n  -h, --help       Print help\n  -V, --version    Print version\n  --include-identifiers  Include configured target addresses in a report\n  --kernel-journal       Opt in to normalized kernel link signals (journalctl)\n\nProbe timeout defaults to 3000 ms and must be between 1 and 30000 ms. Monitor interval defaults to 10000 ms and must be between 1000 and 3600000 ms. Retention defaults to 30 days (range 1–3650) or 100000 events (range 1–1000000), with a 256 MiB database limit. All TCP probe targets must be numeric IP addresses. DNS is queried only when --dns-name is explicitly supplied; it must be a fully qualified ASCII name ending in a dot. The name and answer count are stored locally, but returned IP addresses are not. History, analysis, replay, and report commands are read-only. Reports redact target addresses by default; use --include-identifiers only when raw addresses are appropriate to share. Replay reads one local JSON fixture and performs no networking.\n";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => print!("{HELP}"),
        [arg] if arg == "--help" || arg == "-h" => print!("{HELP}"),
        [arg] if arg == "--version" || arg == "-V" => {
            println!("netforensics {}", env!("CARGO_PKG_VERSION"))
        }
        [command] if command == "status" => {
            if let Err(error) = run_status() {
                eprintln!("Could not read local status: {error}");
                std::process::exit(1);
            }
        }
        [command, flag] if command == "status" && (flag == "--help" || flag == "-h") => {
            print!("{HELP}");
        }
        [command] if command == "incidents" => {
            if let Err(error) = run_incidents() {
                eprintln!("Could not read incident history: {error}");
                std::process::exit(1);
            }
        }
        [command, flag] if command == "analyze" && flag == "--latest" => {
            if let Err(error) = run_analyze_latest() {
                eprintln!("Could not analyze stored history: {error}");
                std::process::exit(1);
            }
        }
        [command, flag] if command == "analyze" && (flag == "--help" || flag == "-h") => {
            print!("{HELP}");
        }
        [command, flag] if command == "replay" && (flag == "--help" || flag == "-h") => {
            print!("{HELP}");
        }
        [command, path] if command == "replay" => {
            if let Err(error) = run_replay(path) {
                eprintln!("Could not replay fixture: {error}");
                std::process::exit(1);
            }
        }
        [command, flag] if command == "report" && (flag == "--help" || flag == "-h") => {
            print!("{HELP}");
        }
        [command, rest @ ..] if command == "report" => {
            let options = match parse_report_args(rest) {
                Ok(options) => options,
                Err(error) => {
                    eprintln!("Invalid report options: {error}\n\n{HELP}");
                    std::process::exit(2);
                }
            };
            if options.include_identifiers {
                eprintln!("Warning: report includes raw configured target addresses; review before sharing.");
            }
            if let Err(error) = run_report_latest(options) {
                eprintln!("Could not report stored history: {error}");
                std::process::exit(1);
            }
        }
        [arg] if arg == "interfaces" => match interfaces::discover() {
            Ok(discovery) => print_interfaces(discovery),
            Err(error) => {
                eprintln!("Could not read local interface list: {error}");
                std::process::exit(1);
            }
        },
        [command, flag] if command == "probe" && (flag == "--help" || flag == "-h") => {
            print!("{HELP}");
        }
        [command, flag] if command == "incidents" && (flag == "--help" || flag == "-h") => {
            print!("{HELP}");
        }
        [command, role, target] if command == "probe" => {
            run_probe(role, target, None);
        }
        [command, role, target, timeout] if command == "probe" => {
            run_probe(role, target, Some(timeout));
        }
        [command, flag] if command == "monitor" && (flag == "--help" || flag == "-h") => {
            print!("{HELP}");
        }
        [command, rest @ ..] if command == "monitor" => {
            let config = match parse_monitor_args(rest) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("Invalid monitor configuration: {error}\n\n{HELP}");
                    std::process::exit(2);
                }
            };
            if !config
                .targets
                .iter()
                .any(|target| target.role == ProbeRole::Gateway)
            {
                eprintln!("No gateway probe target configured; interface observations will still be collected.");
            }
            if let Err(error) = monitor::run_foreground(config) {
                eprintln!("Monitoring stopped with an error: {error}");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("Unknown or unsupported command or argument.\n\n{HELP}");
            std::process::exit(2);
        }
    }
}

fn run_incidents() -> Result<(), Box<dyn std::error::Error>> {
    let store = EventStore::open_default_read_only()?;
    let incidents = load_incidents(&store)?;
    print_incidents(&incidents);
    Ok(())
}

#[derive(Default)]
struct StatusSummary {
    event_count: usize,
    latest_event: Option<(i64, chrono::DateTime<chrono::Utc>)>,
    latest_probe: Option<storage::StoredEvent>,
    latest_interface: Option<storage::StoredEvent>,
    latest_counters: Option<storage::StoredEvent>,
    latest_dns: Option<storage::StoredEvent>,
    latest_kernel_signal: Option<storage::StoredEvent>,
}

fn run_status() -> Result<(), Box<dyn std::error::Error>> {
    let store = match EventStore::open_default_read_only() {
        Ok(store) => store,
        Err(StorageError::DatabaseNotFound) => {
            println!("{}", render_status(&StatusSummary::default()));
            return Ok(());
        }
        Err(error) => return Err(Box::new(error)),
    };
    println!("{}", render_status(&load_status(&store)?));
    Ok(())
}

fn load_status(store: &EventStore) -> Result<StatusSummary, StorageError> {
    let mut summary = StatusSummary::default();
    let mut cursor = 0;
    loop {
        let page = store.read_events_after(cursor, 1_000)?;
        let Some(last) = page.last() else {
            break;
        };
        cursor = last.sequence;
        for stored in page {
            summary.event_count += 1;
            summary.latest_event = Some((stored.sequence, stored.event.recorded_at_utc));
            match &stored.event.kind {
                EventKind::ProbeResult { .. } => summary.latest_probe = Some(stored),
                EventKind::InterfaceSnapshot { .. } => summary.latest_interface = Some(stored),
                EventKind::InterfaceCounterDelta { .. } => summary.latest_counters = Some(stored),
                EventKind::DnsResult { .. } => summary.latest_dns = Some(stored),
                EventKind::KernelLinkSignal { .. } => summary.latest_kernel_signal = Some(stored),
                EventKind::LinkStateChanged { .. } | EventKind::LinkSpeedChanged { .. } => {}
            }
        }
    }
    Ok(summary)
}

fn render_status(summary: &StatusSummary) -> String {
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

    output.push_str("Monitor process: unknown (monitoring runs in the foreground; status does not inspect processes).\n");
    output.push_str("Complete current monitor configuration: not stored in event history.\n");

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

fn render_latest_interface(output: &mut String, latest: &Option<storage::StoredEvent>) {
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

fn render_latest_counters(output: &mut String, latest: &Option<storage::StoredEvent>) {
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

fn optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

fn optional_value<T: std::fmt::Display>(value: Option<T>, suffix: &str) -> String {
    value
        .map(|value| format!("{value}{suffix}"))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn describe_probe_outcome(outcome: &ProbeOutcome) -> String {
    match outcome {
        ProbeOutcome::Success => "success".to_owned(),
        ProbeOutcome::Timeout => "timeout".to_owned(),
        ProbeOutcome::Failure(category) => format!("failure ({})", failure_name(*category)),
        ProbeOutcome::UnsupportedOrBlocked(category) => {
            format!("unsupported or blocked ({})", limitation_name(*category))
        }
    }
}

fn describe_dns_outcome(outcome: &DnsOutcome) -> String {
    match outcome {
        DnsOutcome::Success { answer_count } => format!("success ({answer_count} answers)"),
        DnsOutcome::Timeout => "timeout".to_owned(),
        DnsOutcome::Failure(category) => format!("failure ({category:?})"),
        DnsOutcome::UnsupportedOrBlocked(category) => format!("unavailable ({category:?})"),
    }
}

fn load_incidents(store: &EventStore) -> Result<Vec<Incident>, HistoryError> {
    Ok(analyze_events(
        &load_events(store)?,
        crate::incidents::IncidentConfig::default(),
    )?)
}

fn load_events(store: &EventStore) -> Result<Vec<storage::StoredEvent>, StorageError> {
    let mut events = Vec::new();
    let mut cursor = 0;
    loop {
        let page = store.read_events_after(cursor, 1_000)?;
        let Some(last) = page.last() else {
            break;
        };
        cursor = last.sequence;
        events.extend(page);
    }
    Ok(events)
}

struct AnalysisHistory {
    incidents: Vec<Incident>,
    findings: Vec<analysis::Finding>,
}

fn load_analysis(store: &EventStore) -> Result<AnalysisHistory, HistoryError> {
    let events = load_events(store)?;
    let incidents = analyze_events(&events, crate::incidents::IncidentConfig::default())?;
    let findings =
        analysis::analyze_incidents(&events, &incidents, analysis::AnalysisConfig::default())?;
    Ok(AnalysisHistory {
        incidents,
        findings,
    })
}

fn run_analyze_latest() -> Result<(), HistoryError> {
    let store = EventStore::open_default_read_only()?;
    let history = load_analysis(&store)?;
    println!("{}", report::render_analysis(&history.findings));
    Ok(())
}

fn run_replay(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let input = read_bounded(File::open(path)?, replay::MAX_FIXTURE_BYTES)?;
    let input = String::from_utf8(input)?;
    let result = replay::replay_json(&input)?;
    println!("{}", render_replay(&result));
    Ok(())
}

fn read_bounded<R: Read>(reader: R, maximum: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(maximum.min(64 * 1024));
    reader
        .take((maximum as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("fixture exceeds the {maximum}-byte limit"),
        ));
    }
    Ok(bytes)
}

fn render_replay(result: &replay::ReplayResult) -> String {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReportOptions {
    format: ReportFormat,
    include_identifiers: bool,
}

fn parse_report_args(args: &[String]) -> Result<ReportOptions, String> {
    if args.first().map(String::as_str) != Some("--latest") {
        return Err("report requires --latest".to_owned());
    }
    let mut options = ReportOptions {
        format: ReportFormat::Text,
        include_identifiers: false,
    };
    let mut index = 1;
    let mut format_seen = false;
    let mut identifiers_seen = false;
    while index < args.len() {
        match args[index].as_str() {
            "--format" => {
                if format_seen {
                    return Err("--format may be specified only once".to_owned());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --format".to_owned())?;
                options.format = match value.as_str() {
                    "text" => ReportFormat::Text,
                    "json" => ReportFormat::Json,
                    _ => return Err("report format must be 'text' or 'json'".to_owned()),
                };
                format_seen = true;
                index += 2;
            }
            "--include-identifiers" => {
                if identifiers_seen {
                    return Err("--include-identifiers may be specified only once".to_owned());
                }
                options.include_identifiers = true;
                identifiers_seen = true;
                index += 1;
            }
            option => return Err(format!("unknown report option {option}")),
        }
    }
    Ok(options)
}

fn run_report_latest(options: ReportOptions) -> Result<(), HistoryError> {
    let store = EventStore::open_default_read_only()?;
    let history = load_analysis(&store)?;
    let report = report::build_report(
        &history.incidents,
        &history.findings,
        options.include_identifiers,
    );
    let rendered = match options.format {
        ReportFormat::Text => report::render_text(&report),
        ReportFormat::Json => report::render_json(&report).map_err(HistoryError::Json)?,
    };
    println!("{rendered}");
    Ok(())
}

#[derive(Debug)]
enum HistoryError {
    Storage(StorageError),
    Incident(IncidentError),
    Analysis(analysis::AnalysisError),
    Json(serde_json::Error),
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::Incident(error) => error.fmt(formatter),
            Self::Analysis(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for HistoryError {}

impl From<StorageError> for HistoryError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<IncidentError> for HistoryError {
    fn from(error: IncidentError) -> Self {
        Self::Incident(error)
    }
}

impl From<analysis::AnalysisError> for HistoryError {
    fn from(error: analysis::AnalysisError) -> Self {
        Self::Analysis(error)
    }
}

impl From<serde_json::Error> for HistoryError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

fn print_incidents(incidents: &[Incident]) {
    println!("{}", render_incidents(incidents));
}

fn render_incidents(incidents: &[Incident]) -> String {
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

fn format_evidence_ids(ids: &[String]) -> String {
    if ids.is_empty() {
        return "none".to_owned();
    }
    ids.iter()
        .map(|id| id.escape_debug().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_monitor_args(args: &[String]) -> Result<MonitorConfig, String> {
    let mut interval_millis = DEFAULT_INTERVAL_MILLIS;
    let mut timeout_millis = probes::DEFAULT_TIMEOUT_MILLIS;
    let mut retention_days = DEFAULT_RETENTION_DAYS;
    let mut max_events = DEFAULT_MAX_EVENTS;
    let mut targets = Vec::new();
    let mut dns_name = None;
    let mut kernel_journal = false;
    let mut index = 0;
    while index < args.len() {
        let option = args[index].as_str();
        if option == "--kernel-journal" {
            if kernel_journal {
                return Err("--kernel-journal may be specified only once".to_owned());
            }
            kernel_journal = true;
            index += 1;
            continue;
        }
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {option}"))?;
        match option {
            "--interval-ms" => {
                interval_millis = value
                    .parse()
                    .map_err(|_| "interval must be an integer in milliseconds".to_owned())?;
            }
            "--timeout-ms" => {
                timeout_millis = value
                    .parse()
                    .map_err(|_| "timeout must be an integer in milliseconds".to_owned())?;
            }
            "--retention-days" => {
                retention_days = value
                    .parse()
                    .map_err(|_| "retention days must be an integer".to_owned())?;
            }
            "--max-events" => {
                max_events = value
                    .parse()
                    .map_err(|_| "maximum events must be an integer".to_owned())?;
            }
            "--gateway" => targets.push(parse_probe_target("gateway", value)?),
            "--external" => targets.push(parse_probe_target("external", value)?),
            "--dns-name" => {
                if dns_name.is_some() {
                    return Err("only one --dns-name may be configured".to_owned());
                }
                dns_name = Some(crate::dns::DnsName::parse(value).map_err(str::to_owned)?);
            }
            _ => return Err(format!("unknown option {option}")),
        }
        index += 2;
    }
    let retention =
        RetentionPolicy::new(retention_days, max_events).map_err(|error| error.to_string())?;
    MonitorConfig::with_retention(interval_millis, timeout_millis, targets, retention)
        .map(|config| {
            config
                .with_dns_name(dns_name)
                .with_kernel_journal(kernel_journal)
        })
        .map_err(str::to_owned)
}

fn run_probe(role: &str, target: &str, timeout: Option<&String>) {
    let target = match parse_probe_target(role, target) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}\n\n{HELP}");
            std::process::exit(2);
        }
    };
    let config = match parse_probe_config(timeout.map(String::as_str)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Invalid probe timeout: {error}");
            std::process::exit(2);
        }
    };

    let mut runner = ProbeRunner::new(SystemTransport, SystemClock::new());
    let event = runner.run(next_event_id(), target, config);
    print_probe_result(event);
}

fn parse_probe_target(role: &str, address: &str) -> Result<ProbeTarget, String> {
    let role = match role {
        "gateway" => ProbeRole::Gateway,
        "external" => ProbeRole::External,
        _ => return Err("Probe role must be 'gateway' or 'external'.".to_owned()),
    };
    let address = address.parse::<SocketAddr>().map_err(|_| {
        "Probe target must be a numeric IP address with a port (IP:PORT).".to_owned()
    })?;
    Ok(ProbeTarget { role, address })
}

fn parse_probe_config(timeout: Option<&str>) -> Result<ProbeConfig, String> {
    match timeout {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| "timeout must be an integer in milliseconds".to_owned())
            .and_then(|value| ProbeConfig::from_millis(value).map_err(str::to_owned)),
        None => Ok(ProbeConfig::default()),
    }
}

fn print_probe_result(event: Event) {
    let EventKind::ProbeResult {
        role,
        target,
        method: _,
        started_at_utc,
        elapsed_ms,
        outcome,
    } = event.kind
    else {
        eprintln!("Internal error: expected probe result event");
        std::process::exit(1);
    };

    let result = match outcome {
        ProbeOutcome::Success => "success".to_owned(),
        ProbeOutcome::Timeout => "timeout".to_owned(),
        ProbeOutcome::Failure(category) => format!("failure ({})", failure_name(category)),
        ProbeOutcome::UnsupportedOrBlocked(category) => {
            format!("unsupported or blocked ({})", limitation_name(category))
        }
    };
    println!("Role: {role:?}\nTarget: {target}\nMethod: TCP connect\nStarted: {started_at_utc}\nElapsed: {elapsed_ms} ms\nResult: {result}");
}

fn failure_name(category: ProbeFailureCategory) -> &'static str {
    match category {
        ProbeFailureCategory::ConnectionRefused => "connection refused",
        ProbeFailureCategory::NetworkUnreachable => "network unreachable",
        ProbeFailureCategory::HostUnreachable => "host unreachable",
        ProbeFailureCategory::Other => "other network error",
    }
}

fn limitation_name(category: ProbeLimitationCategory) -> &'static str {
    match category {
        ProbeLimitationCategory::PermissionDenied => "permission denied",
        ProbeLimitationCategory::MethodUnsupported => "method unsupported",
    }
}

pub(crate) fn next_event_id() -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "probe-{}-{epoch_nanos}-{}",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ProbeMethod, SCHEMA_VERSION};

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
        let base =
            ["--external", "192.0.2.1:443", "--external", "192.0.2.2:443"].map(str::to_owned);
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
        let base =
            ["--external", "192.0.2.1:443", "--external", "192.0.2.2:443"].map(str::to_owned);
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
            }
        );
        let json = ["--latest", "--format", "json"];
        let json = json.map(str::to_owned);
        assert_eq!(parse_report_args(&json).unwrap().format, ReportFormat::Json);
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
        ] {
            assert!(parse_report_args(&args).is_err());
        }
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
        assert!(output.contains("Monitor process: unknown"));
        assert!(output.contains("Complete current monitor configuration: not stored"));
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
        assert!(output.contains("Complete current monitor configuration: not stored"));
    }

    #[test]
    fn incident_history_reports_empty_database_clearly() {
        let store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
        let incidents = load_incidents(&store).unwrap();
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
        let fixture = include_str!("../tests/fixtures/endpoint-specific/events.json");
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

        let incidents = load_incidents(&store).unwrap();
        assert_eq!(incidents.len(), 1);
        let output = render_incidents(&incidents);
        assert!(output.contains("incident-1000 [OPEN]"));
        assert!(output.contains("Target: External 192.0.2.8:443"));
        assert!(output.contains("Failure evidence: fail-a, fail-b"));
    }
}

fn print_interfaces(discovery: interfaces::Discovery) {
    println!(
        "Default route: {}",
        discovery
            .default_route
            .as_deref()
            .unwrap_or("unavailable (none detected or route table unreadable)")
    );
    println!("\nInterface\tState\tCarrier\tSpeed (Mbps)");
    for interface in discovery.interfaces {
        let carrier = match interface.carrier {
            Some(true) => "up",
            Some(false) => "down",
            None => "unavailable",
        };
        let speed = interface
            .speed_mbps
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unavailable".to_owned());
        println!(
            "{}{}\t{}\t{}\t{}",
            interface.name,
            if interface.is_default_route {
                " (default route)"
            } else {
                ""
            },
            interface.operational_state,
            carrier,
            speed
        );
    }
}
