pub mod event;
pub mod interfaces;
pub mod monitor;
pub mod probes;

use crate::event::{
    Event, EventKind, ProbeFailureCategory, ProbeLimitationCategory, ProbeOutcome, ProbeRole,
};
use crate::monitor::{MonitorConfig, DEFAULT_INTERVAL_MILLIS};
use crate::probes::{ProbeConfig, ProbeRunner, ProbeTarget, SystemClock, SystemTransport};
use std::env;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const HELP: &str = "Network Forensics (netforensics)\n\nA local network diagnostics tool. Interface inventory, explicit one-shot TCP probes, and foreground monitoring are available; persistence and analysis are planned.\n\nUsage:\n  netforensics [--help] [--version]\n  netforensics interfaces\n  netforensics probe <gateway|external> <IP:PORT> [TIMEOUT_MS]\n  netforensics monitor --external <IP:PORT> --external <IP:PORT> [--gateway <IP:PORT>] [--interval-ms MS] [--timeout-ms MS]\n\nCommands:\n  interfaces       List local interfaces and the detected default route\n  probe            Attempt one TCP connection to the explicitly supplied IP:port\n  monitor          Emit interface and probe observations until Ctrl+C\n\nOptions:\n  -h, --help       Print help\n  -V, --version    Print version\n\nProbe timeout defaults to 3000 ms and must be between 1 and 30000 ms. Monitor interval defaults to 10000 ms and must be between 1000 and 3600000 ms. All probe targets must be numeric IP addresses; no DNS lookup is performed.\n";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => print!("{HELP}"),
        [arg] if arg == "--help" || arg == "-h" => print!("{HELP}"),
        [arg] if arg == "--version" || arg == "-V" => {
            println!("netforensics {}", env!("CARGO_PKG_VERSION"))
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

fn parse_monitor_args(args: &[String]) -> Result<MonitorConfig, String> {
    let mut interval_millis = DEFAULT_INTERVAL_MILLIS;
    let mut timeout_millis = probes::DEFAULT_TIMEOUT_MILLIS;
    let mut targets = Vec::new();
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
            "--gateway" => targets.push(parse_probe_target("gateway", value)?),
            "--external" => targets.push(parse_probe_target("external", value)?),
            _ => return Err(format!("unknown option {option}")),
        }
        index += 2;
    }
    MonitorConfig::new(interval_millis, timeout_millis, targets).map_err(str::to_owned)
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
