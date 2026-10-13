pub mod event;
pub mod interfaces;

use std::env;

const HELP: &str = "Network Forensics (netforensics)\n\nA local network diagnostics tool. The interfaces command reports current local interface state. Monitoring and analysis commands are planned.\n\nUsage: netforensics [--help] [--version] [interfaces]\n\nCommands:\n  interfaces       List local interfaces and the detected default route\n\nOptions:\n  -h, --help       Print help\n  -V, --version    Print version\n";

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
        _ => {
            eprintln!("Unknown or unsupported command or argument.\n\n{HELP}");
            std::process::exit(2);
        }
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
