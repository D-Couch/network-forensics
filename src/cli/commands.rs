use super::{args::*, render::*, types::*, HELP};
use crate::event::{Event, EventKind, ProbeOutcome};
use crate::incidents::{analyze_events, Incident, IncidentError};
use crate::probes::{ProbeRunner, SystemClock, SystemTransport};
use crate::storage::{EventStore, StorageError};
use crate::{analysis, monitor_state, replay, report, report_output, storage};
use std::fs::File;
use std::io::{self, BufRead, Read, Write};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn run_incidents(options: IncidentOptions) -> Result<(), Box<dyn std::error::Error>> {
    let store = EventStore::open_default_read_only()?;
    let events = load_events(&store)?;
    let incidents = analyze_events(&events, crate::incidents::IncidentConfig::default())?;
    let incidents = filter_incidents(&incidents, options);
    println!("{}", render_incident_history(&events, &incidents, options));
    Ok(())
}

pub(super) fn run_status() -> Result<(), Box<dyn std::error::Error>> {
    let mut summary = match EventStore::open_default_read_only() {
        Ok(store) => load_status(&store)?,
        Err(StorageError::DatabaseNotFound) => StatusSummary::default(),
        Err(error) => return Err(Box::new(error)),
    };
    summary.monitor = monitor_state::read_default_status();
    println!("{}", render_status(&summary));
    Ok(())
}

pub(super) fn load_status(store: &EventStore) -> Result<StatusSummary, StorageError> {
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
                EventKind::LinkStateChanged { .. }
                | EventKind::LinkSpeedChanged { .. }
                | EventKind::MonitoringGap { .. } => {}
            }
        }
    }
    Ok(summary)
}

pub(super) fn load_events(store: &EventStore) -> Result<Vec<storage::StoredEvent>, StorageError> {
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

pub(super) struct AnalysisHistory {
    pub(super) incidents: Vec<Incident>,
    pub(super) findings: Vec<analysis::Finding>,
}

pub(super) fn load_analysis(store: &EventStore) -> Result<AnalysisHistory, HistoryError> {
    let events = load_events(store)?;
    let incidents = analyze_events(&events, crate::incidents::IncidentConfig::default())?;
    let findings =
        analysis::analyze_incidents(&events, &incidents, analysis::AnalysisConfig::default())?;
    Ok(AnalysisHistory {
        incidents,
        findings,
    })
}

pub(super) fn run_analyze_latest() -> Result<(), HistoryError> {
    let store = EventStore::open_default_read_only()?;
    let history = load_analysis(&store)?;
    println!("{}", report::render_analysis(&history.findings));
    Ok(())
}

pub(super) fn run_replay(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let input = read_bounded(File::open(path)?, replay::MAX_FIXTURE_BYTES)?;
    let input = String::from_utf8(input)?;
    let result = replay::replay_json(&input)?;
    println!("{}", render_replay(&result));
    Ok(())
}

pub(super) fn run_setup<R: BufRead, W: Write>(reader: &mut R, writer: &mut W) -> io::Result<()> {
    writeln!(writer, "Guided monitoring setup")?;
    writeln!(writer, "Choose two distinct external services you are permitted to contact and expect to accept TCP on the chosen ports. Enter numeric IP:PORT values; setup makes no network requests.")?;
    writeln!(writer, "Monitoring attempts each configured TCP target once per round, sequentially, with no application retries. The interval is a pause after a completed round (10 seconds by default), so time between attempts also includes round duration. Increase --interval-ms to reduce request frequency and respect target owners' policies.")?;
    writeln!(writer, "A gateway-role target is optional. It is only a label for the endpoint you enter; it does not verify the active gateway or prove gateway reachability.")?;

    let first_external = prompt_target(reader, writer, "First external IP:PORT: ", "external")?;
    let second_external = loop {
        let candidate = prompt_target(reader, writer, "Second external IP:PORT: ", "external")?;
        if candidate.ip() != first_external.ip() {
            break candidate;
        }
        writeln!(
            writer,
            "Choose an external target with a different IP address."
        )?;
    };
    let gateway = loop {
        let candidate = prompt_optional_target(
            reader,
            writer,
            "Optional gateway-role IP:PORT (press Enter to skip): ",
            "gateway",
        )?;
        if candidate.is_some_and(|address| address == first_external || address == second_external)
        {
            writeln!(
                writer,
                "Choose a gateway-role IP:PORT different from the external targets."
            )?;
            continue;
        }
        break candidate;
    };

    write!(
        writer,
        "\nSuggested command (review it; setup will not start monitoring):\nnetforensics monitor --external '{first_external}' --external '{second_external}'"
    )?;
    if let Some(gateway) = gateway {
        write!(writer, " --gateway '{gateway}'")?;
    }
    writeln!(writer)?;
    writeln!(writer, "Running this command starts repeated probes to these configured endpoints and stores observations locally.")?;
    Ok(())
}

pub(super) fn prompt_target<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    prompt: &str,
    role: &str,
) -> io::Result<SocketAddr> {
    loop {
        write!(writer, "{prompt}")?;
        writer.flush()?;
        let mut input = String::new();
        if reader.read_line(&mut input)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "no target was provided",
            ));
        }
        match parse_probe_target(role, input.trim()) {
            Ok(target) => return Ok(target.address),
            Err(error) => writeln!(writer, "{error} Enter a numeric IP:PORT.")?,
        }
    }
}

pub(super) fn prompt_optional_target<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    prompt: &str,
    role: &str,
) -> io::Result<Option<SocketAddr>> {
    loop {
        write!(writer, "{prompt}")?;
        writer.flush()?;
        let mut input = String::new();
        if reader.read_line(&mut input)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "setup ended before target selection was complete",
            ));
        }
        let input = input.trim();
        if input.is_empty() {
            return Ok(None);
        }
        match parse_probe_target(role, input) {
            Ok(target) => return Ok(Some(target.address)),
            Err(error) => writeln!(
                writer,
                "{error} Enter a numeric IP:PORT or press Enter to skip."
            )?,
        }
    }
}

pub(super) fn read_bounded<R: Read>(reader: R, maximum: usize) -> io::Result<Vec<u8>> {
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

pub(super) fn run_report_latest(options: ReportOptions) -> Result<(), HistoryError> {
    let store = EventStore::open_default_read_only()?;
    let history = load_analysis(&store)?;
    let report = report::build_report(
        &history.incidents,
        &history.findings,
        options.include_identifiers,
    );
    let mut rendered = match options.format {
        ReportFormat::Text => report::render_text(&report),
        ReportFormat::Json => report::render_json(&report).map_err(HistoryError::Json)?,
    };
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    if let Some(path) = options.output {
        report_output::write_report(&path, &rendered).map_err(HistoryError::Io)?;
        println!("Report written to {}", path.display());
    } else {
        print!("{rendered}");
    }
    Ok(())
}

#[derive(Debug)]
pub(super) enum HistoryError {
    Storage(StorageError),
    Incident(IncidentError),
    Analysis(analysis::AnalysisError),
    Json(serde_json::Error),
    Io(io::Error),
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::Incident(error) => error.fmt(formatter),
            Self::Analysis(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
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

pub(super) fn run_probe(role: &str, target: &str, timeout: Option<&String>) {
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

pub(super) fn print_probe_result(event: Event) {
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
