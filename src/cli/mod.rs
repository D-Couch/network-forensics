const HELP: &str = "Network Forensics (netforensics)\n\nA local network diagnostics tool. Interface inventory, explicit one-shot TCP probes, foreground monitoring, local incident history, analysis, and reports are available.\n\nUsage:\n  netforensics [--help] [--version]\n  netforensics status\n  netforensics interfaces\n  netforensics probe <gateway|external> <IP:PORT> [TIMEOUT_MS]\n  netforensics monitor --external <IP:PORT> --external <IP:PORT> [--gateway <IP:PORT>] [--dns-name <FQDN>] [--kernel-journal] [--interval-ms MS] [--timeout-ms MS] [--retention-days DAYS] [--max-events COUNT]\n  netforensics incidents [--since RFC3339] [--until RFC3339] [--target IP:PORT] [--compare]\n  netforensics analyze --latest\n  netforensics report --latest [--format text|json] [--include-identifiers] [--output FILE]\n  netforensics replay <FIXTURE.json>\n  netforensics setup\n\nCommands:\n  status           Summarize retained observations and monitor state/configuration\n  interfaces       List local interfaces and the detected default route\n  probe            Attempt one TCP connection to the explicitly supplied IP:port\n  monitor          Emit and save interface/probe observations until Ctrl+C\n  incidents        Filter incidents and compare same-target probes\n  analyze          Analyze the latest retained local event history\n  report           Render or save a text or JSON report from local event history\n  replay           Analyze a versioned fixture offline\n  setup            Guide target selection and print a monitor command\n\nOptions:\n  -h, --help       Print help\n  -V, --version    Print version\n  --include-identifiers  Include configured target addresses in a report\n  --kernel-journal       Opt in to normalized kernel link signals (journalctl)\n  --output FILE          Write a new report file without overwriting an existing path\n\nProbe timeout defaults to 3000 ms and must be between 1 and 30000 ms. Monitor interval defaults to 10000 ms and must be between 1000 and 3600000 ms; it is a pause after a completed round, not the per-target probe period. Each configured TCP target is attempted once per round, sequentially, without application retries, so the time between attempts to a target also includes the round duration. Use targets you are permitted to contact and that are expected to accept TCP on the selected port; increase --interval-ms to reduce request frequency and respect target owners' policies. Retention defaults to 30 days (range 1–3650) or 100000 events (range 1–1000000), with a 256 MiB database limit. All TCP probe targets must be numeric IP addresses. DNS is queried only when --dns-name is explicitly supplied; it must be a fully qualified ASCII name ending in a dot. The name and answer count are stored locally, but returned IP addresses are not. History, analysis, and report commands read the event database without modifying it. --output writes one new local report file and refuses to overwrite an existing path. Reports redact target addresses by default; use --include-identifiers only when raw addresses are appropriate to share. Replay reads one local JSON fixture and performs no networking.\n";

mod args;
mod commands;
pub(crate) use commands::next_event_id;
mod render;
mod types;

use crate::event::ProbeRole;
use crate::{interfaces, monitor};
use args::*;
use commands::*;
use std::env;
use std::io;

pub(super) fn run() {
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
        [command, flag] if command == "incidents" && (flag == "--help" || flag == "-h") => {
            print!("{HELP}");
        }
        [command, rest @ ..] if command == "incidents" => {
            let options = match parse_incident_args(rest) {
                Ok(options) => options,
                Err(error) => {
                    eprintln!("Invalid incident filters: {error}\n\n{HELP}");
                    std::process::exit(2);
                }
            };
            if let Err(error) = run_incidents(options) {
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
        [command] if command == "setup" => {
            if let Err(error) = run_setup(&mut io::stdin().lock(), &mut io::stdout().lock()) {
                eprintln!("Setup stopped: {error}");
                std::process::exit(1);
            }
        }
        [command, flag] if command == "setup" && (flag == "--help" || flag == "-h") => {
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

pub(super) fn print_interfaces(discovery: interfaces::Discovery) {
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

#[cfg(test)]
mod tests;
