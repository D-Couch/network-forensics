use crate::event::ProbeRole;
use crate::incidents::Incident;
use crate::monitor::{MonitorConfig, DEFAULT_INTERVAL_MILLIS};
use crate::probes::{self, ProbeConfig, ProbeTarget};
use crate::storage::{RetentionPolicy, DEFAULT_MAX_EVENTS, DEFAULT_RETENTION_DAYS};
use chrono::{DateTime, Utc};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct IncidentOptions {
    pub(super) since: Option<DateTime<Utc>>,
    pub(super) until: Option<DateTime<Utc>>,
    pub(super) target: Option<SocketAddr>,
    pub(super) compare: bool,
}

pub(super) fn parse_incident_args(args: &[String]) -> Result<IncidentOptions, String> {
    let mut options = IncidentOptions::default();
    let mut since_seen = false;
    let mut until_seen = false;
    let mut target_seen = false;
    let mut compare_seen = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--since" => {
                if since_seen {
                    return Err("--since may be specified only once".to_owned());
                }
                options.since = Some(parse_rfc3339_filter(
                    "--since",
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --since".to_owned())?,
                )?);
                since_seen = true;
                index += 2;
            }
            "--until" => {
                if until_seen {
                    return Err("--until may be specified only once".to_owned());
                }
                options.until = Some(parse_rfc3339_filter(
                    "--until",
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --until".to_owned())?,
                )?);
                until_seen = true;
                index += 2;
            }
            "--target" => {
                if target_seen {
                    return Err("--target may be specified only once".to_owned());
                }
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --target".to_owned())?;
                options.target = Some(value.parse().map_err(|_| {
                    "target must be a numeric IP address with a port (IP:PORT)".to_owned()
                })?);
                target_seen = true;
                index += 2;
            }
            "--compare" => {
                if compare_seen {
                    return Err("--compare may be specified only once".to_owned());
                }
                options.compare = true;
                compare_seen = true;
                index += 1;
            }
            option => return Err(format!("unknown incident option {option}")),
        }
    }
    if options
        .since
        .zip(options.until)
        .is_some_and(|(since, until)| since > until)
    {
        return Err("--since must be earlier than or equal to --until".to_owned());
    }
    Ok(options)
}

pub(super) fn parse_rfc3339_filter(option: &str, value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| format!("{option} must be an RFC3339 timestamp"))
}

pub(super) fn filter_incidents(incidents: &[Incident], options: IncidentOptions) -> Vec<Incident> {
    incidents
        .iter()
        .filter(|incident| {
            options
                .since
                .is_none_or(|since| incident.started_at_utc >= since)
                && options
                    .until
                    .is_none_or(|until| incident.started_at_utc <= until)
                && options
                    .target
                    .is_none_or(|target| incident.target.address == target)
        })
        .cloned()
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReportFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReportOptions {
    pub(super) format: ReportFormat,
    pub(super) include_identifiers: bool,
    pub(super) output: Option<PathBuf>,
}

pub(super) fn parse_report_args(args: &[String]) -> Result<ReportOptions, String> {
    if args.first().map(String::as_str) != Some("--latest") {
        return Err("report requires --latest".to_owned());
    }
    let mut options = ReportOptions {
        format: ReportFormat::Text,
        include_identifiers: false,
        output: None,
    };
    let mut index = 1;
    let mut format_seen = false;
    let mut identifiers_seen = false;
    let mut output_seen = false;
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
            "--output" => {
                if output_seen {
                    return Err("--output may be specified only once".to_owned());
                }
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.is_empty() && !value.starts_with("--"))
                    .ok_or_else(|| "missing path for --output".to_owned())?;
                options.output = Some(PathBuf::from(value));
                output_seen = true;
                index += 2;
            }
            option => return Err(format!("unknown report option {option}")),
        }
    }
    Ok(options)
}

pub(super) fn parse_monitor_args(args: &[String]) -> Result<MonitorConfig, String> {
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

pub(super) fn parse_probe_target(role: &str, address: &str) -> Result<ProbeTarget, String> {
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

pub(super) fn parse_probe_config(timeout: Option<&str>) -> Result<ProbeConfig, String> {
    match timeout {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| "timeout must be an integer in milliseconds".to_owned())
            .and_then(|value| ProbeConfig::from_millis(value).map_err(str::to_owned)),
        None => Ok(ProbeConfig::default()),
    }
}
