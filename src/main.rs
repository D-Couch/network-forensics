pub mod analysis;
pub mod dns;
pub mod event;
pub mod incidents;
pub mod interfaces;
pub mod monitor;
pub mod probes;
pub mod replay;
pub mod storage;

use crate::event::{
    Event, EventKind, ProbeFailureCategory, ProbeLimitationCategory, ProbeOutcome, ProbeRole,
};
use crate::incidents::{analyze_events, Incident, IncidentError, IncidentStatus};
use crate::monitor::{MonitorConfig, DEFAULT_INTERVAL_MILLIS};
use crate::probes::{ProbeConfig, ProbeRunner, ProbeTarget, SystemClock, SystemTransport};
use crate::storage::{
    EventStore, RetentionPolicy, StorageError, DEFAULT_MAX_EVENTS, DEFAULT_RETENTION_DAYS,
};
use std::env;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const HELP: &str = "Network Forensics (netforensics)\n\nA local network diagnostics tool. Interface inventory, explicit one-shot TCP probes, foreground monitoring, and local incident history are available.\n\nUsage:\n  netforensics [--help] [--version]\n  netforensics interfaces\n  netforensics probe <gateway|external> <IP:PORT> [TIMEOUT_MS]\n  netforensics monitor --external <IP:PORT> --external <IP:PORT> [--gateway <IP:PORT>] [--dns-name <FQDN>] [--interval-ms MS] [--timeout-ms MS] [--retention-days DAYS] [--max-events COUNT]\n  netforensics incidents\n\nCommands:\n  interfaces       List local interfaces and the detected default route\n  probe            Attempt one TCP connection to the explicitly supplied IP:port\n  monitor          Emit and save interface/probe observations until Ctrl+C\n  incidents        Show incidents derived from locally stored probe events\n\nOptions:\n  -h, --help       Print help\n  -V, --version    Print version\n\nProbe timeout defaults to 3000 ms and must be between 1 and 30000 ms. Monitor interval defaults to 10000 ms and must be between 1000 and 3600000 ms. Retention defaults to 30 days (range 1–3650) or 100000 events (range 1–1000000), with a 256 MiB database limit. All TCP probe targets must be numeric IP addresses. DNS is queried only when --dns-name is explicitly supplied; it must be a fully qualified ASCII name ending in a dot. The name and answer count are stored locally, but returned IP addresses are not. Incident history is read-only and uses local stored events.\n";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => print!("{HELP}"),
        [arg] if arg == "--help" || arg == "-h" => print!("{HELP}"),
        [arg] if arg == "--version" || arg == "-V" => {
            println!("netforensics {}", env!("CARGO_PKG_VERSION"))
        }
        [command] if command == "incidents" => {
            if let Err(error) = run_incidents() {
                eprintln!("Could not read incident history: {error}");
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

fn load_incidents(store: &EventStore) -> Result<Vec<Incident>, HistoryError> {
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
    Ok(analyze_events(
        &events,
        crate::incidents::IncidentConfig::default(),
    )?)
}

#[derive(Debug)]
enum HistoryError {
    Storage(StorageError),
    Incident(IncidentError),
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::Incident(error) => error.fmt(formatter),
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
    let mut index = 0;
    while index < args.len() {
        let option = args[index].as_str();
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
        .map(|config| config.with_dns_name(dns_name))
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
