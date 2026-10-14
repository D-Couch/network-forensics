//! Foreground observation rounds and interruptible interval waiting.

use crate::dns::{DnsName, DnsRunner, DnsTransport};
use crate::event::{Event, EventKind, ProbeRole};
use crate::interfaces::{self, Discovery, InterfaceState};
use crate::probes::{
    ProbeClock, ProbeConfig, ProbeRunner, ProbeTarget, ProbeTransport, SystemClock, SystemTransport,
};
use crate::storage::{EventStore, RetentionPolicy};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

pub const DEFAULT_INTERVAL_MILLIS: u64 = 10_000;
pub const MIN_INTERVAL_MILLIS: u64 = 1_000;
pub const MAX_INTERVAL_MILLIS: u64 = 3_600_000;
pub const MAX_TARGETS: usize = 8;
/// Leaves room in the 4096-event storage batch for up to four events per
/// interface, all configured probes, one DNS result, and bounded journal data.
pub const MAX_INTERFACES_PER_CYCLE: usize = 900;

#[derive(Debug, Default)]
pub struct InterfaceTracker {
    previous: HashMap<String, InterfaceState>,
}

impl InterfaceTracker {
    fn observe<F>(
        &mut self,
        discovery: &Discovery,
        at: DateTime<Utc>,
        next_id: &mut F,
    ) -> Vec<Event>
    where
        F: FnMut() -> String,
    {
        let mut events = Vec::with_capacity(discovery.interfaces.len() * 4);
        let mut current = HashMap::with_capacity(discovery.interfaces.len());
        for interface in &discovery.interfaces {
            events.push(Event {
                schema_version: crate::event::SCHEMA_VERSION,
                id: next_id(),
                recorded_at_utc: at,
                source: "interface".to_owned(),
                kind: EventKind::InterfaceSnapshot {
                    interface: interface.name.clone(),
                    operational_state: interface.operational_state.to_string(),
                    carrier: interface.carrier,
                    speed_mbps: interface.speed_mbps,
                    is_default_route: discovery
                        .default_route
                        .as_ref()
                        .map(|route| route == &interface.name),
                },
            });

            let previous = self.previous.get(&interface.name);
            if let Some(previous) = previous {
                if let (Some(previous_carrier), Some(carrier)) =
                    (previous.carrier, interface.carrier)
                {
                    if previous_carrier != carrier {
                        events.push(Event {
                            schema_version: crate::event::SCHEMA_VERSION,
                            id: next_id(),
                            recorded_at_utc: at,
                            source: "interface".to_owned(),
                            kind: EventKind::LinkStateChanged {
                                interface: interface.name.clone(),
                                carrier: Some(carrier),
                            },
                        });
                    }
                }
                if previous.speed_mbps != interface.speed_mbps
                    && (previous.speed_mbps.is_some() || interface.speed_mbps.is_some())
                {
                    events.push(Event {
                        schema_version: crate::event::SCHEMA_VERSION,
                        id: next_id(),
                        recorded_at_utc: at,
                        source: "interface".to_owned(),
                        kind: EventKind::LinkSpeedChanged {
                            interface: interface.name.clone(),
                            previous_speed_mbps: previous.speed_mbps,
                            speed_mbps: interface.speed_mbps,
                        },
                    });
                }
            }

            let previous_counters = previous.map(|sample| sample.counters).unwrap_or_default();
            let (rx_errors_delta, rx_errors_reset) =
                counter_delta(previous_counters.rx_errors, interface.counters.rx_errors);
            let (tx_errors_delta, tx_errors_reset) =
                counter_delta(previous_counters.tx_errors, interface.counters.tx_errors);
            let (rx_dropped_delta, rx_dropped_reset) =
                counter_delta(previous_counters.rx_dropped, interface.counters.rx_dropped);
            let (tx_dropped_delta, tx_dropped_reset) =
                counter_delta(previous_counters.tx_dropped, interface.counters.tx_dropped);
            events.push(Event {
                schema_version: crate::event::SCHEMA_VERSION,
                id: next_id(),
                recorded_at_utc: at,
                source: "interface".to_owned(),
                kind: EventKind::InterfaceCounterDelta {
                    interface: interface.name.clone(),
                    baseline_available: previous
                        .is_some_and(|sample| sample.counters.sample_available()),
                    sample_available: interface.counters.sample_available(),
                    counter_reset_detected: rx_errors_reset
                        || tx_errors_reset
                        || rx_dropped_reset
                        || tx_dropped_reset,
                    rx_errors_delta,
                    tx_errors_delta,
                    rx_dropped_delta,
                    tx_dropped_delta,
                },
            });
            current.insert(interface.name.clone(), interface.clone());
        }
        self.previous = current;
        events
    }
}

fn counter_delta(previous: Option<u64>, current: Option<u64>) -> (Option<u64>, bool) {
    match (previous, current) {
        (Some(previous), Some(current)) if current >= previous => (Some(current - previous), false),
        (Some(_), Some(_)) => (None, true),
        _ => (None, false),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorConfig {
    pub interval: Duration,
    pub probe: ProbeConfig,
    pub targets: Vec<ProbeTarget>,
    pub retention: RetentionPolicy,
    pub dns_name: Option<DnsName>,
    pub kernel_journal: bool,
}

impl MonitorConfig {
    pub fn new(
        interval_millis: u64,
        timeout_millis: u64,
        targets: Vec<ProbeTarget>,
    ) -> Result<Self, &'static str> {
        Self::with_retention(
            interval_millis,
            timeout_millis,
            targets,
            RetentionPolicy::default(),
        )
    }

    pub fn with_retention(
        interval_millis: u64,
        timeout_millis: u64,
        targets: Vec<ProbeTarget>,
        retention: RetentionPolicy,
    ) -> Result<Self, &'static str> {
        if !(MIN_INTERVAL_MILLIS..=MAX_INTERVAL_MILLIS).contains(&interval_millis) {
            return Err("interval must be between 1000 and 3600000 milliseconds");
        }
        if targets.len() > MAX_TARGETS {
            return Err("no more than 8 probe targets may be configured");
        }
        let external_count = targets
            .iter()
            .filter(|target| target.role == ProbeRole::External)
            .count();
        if external_count < 2 {
            return Err("at least two external probe targets are required");
        }
        let mut external_addresses = HashSet::new();
        if targets
            .iter()
            .filter(|target| target.role == ProbeRole::External)
            .any(|target| !external_addresses.insert(target.address.ip()))
        {
            return Err("external probe targets must use distinct IP addresses");
        }
        if targets
            .iter()
            .filter(|target| target.role == ProbeRole::Gateway)
            .count()
            > 1
        {
            return Err("at most one gateway probe target may be configured");
        }
        let mut addresses = HashSet::new();
        if targets
            .iter()
            .any(|target| !addresses.insert(target.address))
        {
            return Err("probe targets must use distinct IP:port addresses");
        }

        Ok(Self {
            interval: Duration::from_millis(interval_millis),
            probe: ProbeConfig::from_millis(timeout_millis)?,
            targets,
            retention,
            dns_name: None,
            kernel_journal: false,
        })
    }

    pub fn with_dns_name(mut self, dns_name: Option<DnsName>) -> Self {
        self.dns_name = dns_name;
        self
    }

    pub fn with_kernel_journal(mut self, enabled: bool) -> Self {
        self.kernel_journal = enabled;
        self
    }
}

/// Collects one interface snapshot and each configured probe event. The stop
/// flag is checked before each new probe.
pub fn collect_cycle<T, C, F>(
    discovery: &Discovery,
    config: &MonitorConfig,
    tracker: &mut InterfaceTracker,
    runner: &mut ProbeRunner<T, C>,
    stop: &AtomicBool,
    cycle_time: DateTime<Utc>,
    mut next_id: F,
) -> io::Result<Vec<Event>>
where
    T: ProbeTransport,
    C: ProbeClock,
    F: FnMut() -> String,
{
    if discovery.interfaces.len() > MAX_INTERFACES_PER_CYCLE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "interface inventory exceeds the per-cycle limit of {MAX_INTERFACES_PER_CYCLE}; refusing to truncate observations"
            ),
        ));
    }
    let mut events = tracker.observe(discovery, cycle_time, &mut next_id);
    events.reserve(config.targets.len());

    for target in &config.targets {
        if stop.load(Ordering::Acquire) {
            break;
        }
        events.push(runner.run(next_id(), *target, config.probe));
    }
    Ok(events)
}

pub fn collect_cycle_with_dns<T, C, D, E, F>(
    config: &MonitorConfig,
    tracker: &mut InterfaceTracker,
    runner: &mut ProbeRunner<T, C>,
    dns_runner: &mut DnsRunner<D, E>,
    cycle: CycleInput<'_>,
    mut next_id: F,
) -> io::Result<Vec<Event>>
where
    T: ProbeTransport,
    C: ProbeClock,
    D: DnsTransport,
    E: ProbeClock,
    F: FnMut() -> String,
{
    let mut events = collect_cycle(
        cycle.discovery,
        config,
        tracker,
        runner,
        cycle.stop,
        cycle.recorded_at,
        &mut next_id,
    )?;
    if !cycle.stop.load(Ordering::Acquire) {
        if let Some(name) = &config.dns_name {
            events.push(dns_runner.run(next_id(), name, config.probe.timeout()));
        }
    }
    Ok(events)
}

pub struct CycleInput<'a> {
    pub discovery: &'a Discovery,
    pub stop: &'a AtomicBool,
    pub recorded_at: DateTime<Utc>,
}

fn append_journal_events<R, F>(
    events: &mut Vec<Event>,
    collector: &mut crate::journal::KernelJournalCollector<R>,
    recorded_at_utc: DateTime<Utc>,
    warning_reported: &mut bool,
    next_id: F,
) -> Option<crate::journal::JournalError>
where
    R: crate::journal::KernelJournalReader,
    F: FnMut() -> String,
{
    match collector.collect(recorded_at_utc, next_id) {
        Ok(mut journal_events) => {
            events.append(&mut journal_events);
            None
        }
        Err(error) if !*warning_reported => {
            *warning_reported = true;
            Some(error)
        }
        Err(_) => None,
    }
}

fn write_event<W: Write>(writer: &mut W, event: &Event) -> io::Result<()> {
    let serialized = serde_json::to_string(event).map_err(io::Error::other)?;
    writeln!(writer, "{serialized}")
}

fn persist_then_write<W: Write>(
    store: &mut EventStore,
    events: &[Event],
    writer: &mut W,
) -> io::Result<()> {
    store.store_events(events).map_err(io::Error::other)?;
    for event in events {
        write_event(writer, event)?;
    }
    Ok(())
}

/// Runs until Ctrl+C. A currently active TCP connect is allowed to finish or
/// time out before shutdown; no new probes start after the signal is observed.
pub fn run_foreground(config: MonitorConfig) -> io::Result<()> {
    let mut store = EventStore::open_default(config.retention).map_err(io::Error::other)?;
    let _monitor_guard = crate::monitor_state::MonitorGuard::acquire_default(&config)?;
    let stop = Arc::new(AtomicBool::new(false));
    let signal_stop = Arc::clone(&stop);
    ctrlc::set_handler(move || signal_stop.store(true, Ordering::Release))
        .map_err(io::Error::other)?;

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let mut runner = ProbeRunner::new(SystemTransport, SystemClock::new());
    let mut interface_tracker = InterfaceTracker::default();
    let mut dns_runner = match &config.dns_name {
        Some(_) => Some(DnsRunner::new(
            crate::dns::SystemDnsTransport::new(config.probe.timeout())?,
            SystemClock::new(),
        )),
        None => None,
    };
    let mut journal_collector = config.kernel_journal.then(|| {
        crate::journal::KernelJournalCollector::new(crate::journal::SystemKernelJournalReader)
    });
    let mut journal_warning_reported = false;

    while !stop.load(Ordering::Acquire) {
        let discovery = match interfaces::discover() {
            Ok(discovery) => discovery,
            Err(error) => {
                eprintln!("Interface observations unavailable this round: {error}");
                Discovery {
                    interfaces: Vec::new(),
                    default_route: None,
                }
            }
        };
        let cycle_time = DateTime::<Utc>::from(SystemTime::now());
        let mut events = if let Some(dns_runner) = &mut dns_runner {
            collect_cycle_with_dns(
                &config,
                &mut interface_tracker,
                &mut runner,
                dns_runner,
                CycleInput {
                    discovery: &discovery,
                    stop: &stop,
                    recorded_at: cycle_time,
                },
                crate::next_event_id,
            )?
        } else {
            collect_cycle(
                &discovery,
                &config,
                &mut interface_tracker,
                &mut runner,
                &stop,
                cycle_time,
                crate::next_event_id,
            )?
        };
        if let Some(collector) = &mut journal_collector {
            if let Some(error) = append_journal_events(
                &mut events,
                collector,
                cycle_time,
                &mut journal_warning_reported,
                crate::next_event_id,
            ) {
                eprintln!(
                    "Kernel journal observations unavailable ({error:?}); continuing without them."
                );
            }
        }
        persist_then_write(&mut store, &events, &mut writer)?;
        writer.flush()?;
        if !stop.load(Ordering::Acquire) {
            wait_interval(config.interval, &stop);
        }
    }
    Ok(())
}

fn wait_interval(interval: Duration, stop: &AtomicBool) {
    let deadline = Instant::now() + interval;
    while !stop.load(Ordering::Acquire) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::{DnsLookupError, DnsName, DnsTransport};
    use crate::event::{EventKind, ProbeMethod, ProbeOutcome};
    use crate::interfaces::{InterfaceState, OperationalState};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::io;
    use std::rc::Rc;

    struct FakeTransport {
        calls: Rc<Cell<usize>>,
    }

    impl ProbeTransport for FakeTransport {
        fn tcp_connect(
            &mut self,
            _target: std::net::SocketAddr,
            _timeout: Duration,
        ) -> io::Result<()> {
            self.calls.set(self.calls.get() + 1);
            Ok(())
        }
    }

    struct FakeClock {
        monotonic: RefCell<VecDeque<Duration>>,
    }

    impl ProbeClock for FakeClock {
        fn utc_now(&self) -> DateTime<Utc> {
            "2026-10-13T00:00:00Z".parse().unwrap()
        }

        fn monotonic_now(&self) -> Duration {
            self.monotonic.borrow_mut().pop_front().unwrap()
        }
    }

    struct FakeDnsTransport(Rc<RefCell<Vec<Duration>>>);
    impl DnsTransport for FakeDnsTransport {
        fn lookup(
            &mut self,
            _name: &DnsName,
            timeout: Duration,
        ) -> Result<Vec<std::net::IpAddr>, DnsLookupError> {
            self.0.borrow_mut().push(timeout);
            Ok(vec!["192.0.2.53".parse().unwrap()])
        }
    }

    fn target(ip: &str) -> ProbeTarget {
        ProbeTarget {
            role: ProbeRole::External,
            address: format!("{ip}:443").parse().unwrap(),
        }
    }

    fn discovery() -> Discovery {
        Discovery {
            interfaces: vec![InterfaceState {
                name: "test0".into(),
                operational_state: OperationalState::Unknown,
                carrier: None,
                speed_mbps: None,
                is_default_route: false,
                counters: Default::default(),
            }],
            default_route: None,
        }
    }

    #[test]
    fn requires_two_distinct_external_targets_and_bounds_the_round() {
        assert!(MonitorConfig::new(1_000, 3_000, vec![target("192.0.2.1")]).is_err());
        assert!(
            MonitorConfig::new(999, 3_000, vec![target("192.0.2.1"), target("192.0.2.2")]).is_err()
        );
        assert!(
            MonitorConfig::new(1_000, 3_000, vec![target("192.0.2.1"), target("192.0.2.1")])
                .is_err()
        );
        assert!(MonitorConfig::new(
            1_000,
            3_000,
            vec![
                target("192.0.2.1"),
                ProbeTarget {
                    role: ProbeRole::External,
                    address: "192.0.2.1:8443".parse().unwrap(),
                }
            ]
        )
        .is_err());
        assert!(
            MonitorConfig::new(1_000, 3_000, vec![target("192.0.2.1"), target("192.0.2.2")])
                .is_ok()
        );
    }

    #[test]
    fn cycle_writes_structured_json_events_without_live_networking() {
        let calls = Rc::new(Cell::new(0));
        let mut runner = ProbeRunner::new(
            FakeTransport {
                calls: Rc::clone(&calls),
            },
            FakeClock {
                monotonic: RefCell::new(VecDeque::from([
                    Duration::from_millis(0),
                    Duration::from_millis(2),
                    Duration::from_millis(3),
                    Duration::from_millis(5),
                ])),
            },
        );
        let stop = AtomicBool::new(false);
        let mut ids = 0;
        let config = MonitorConfig::new(
            DEFAULT_INTERVAL_MILLIS,
            crate::probes::DEFAULT_TIMEOUT_MILLIS,
            vec![target("192.0.2.1"), target("192.0.2.2")],
        )
        .unwrap();
        let mut interface_tracker = InterfaceTracker::default();
        let events = collect_cycle(
            &discovery(),
            &config,
            &mut interface_tracker,
            &mut runner,
            &stop,
            "2026-10-13T00:00:00Z".parse().unwrap(),
            || {
                ids += 1;
                format!("test-{ids}")
            },
        )
        .unwrap();
        let mut output = Vec::new();
        for event in &events {
            write_event(&mut output, event).unwrap();
        }
        let lines: Vec<_> = std::str::from_utf8(&output).unwrap().lines().collect();
        assert_eq!(lines.len(), 4);
        let events: Vec<_> = lines
            .iter()
            .map(|line| Event::from_json(line).unwrap())
            .collect();
        assert!(matches!(
            events[0].kind,
            EventKind::InterfaceSnapshot { carrier: None, .. }
        ));
        assert!(matches!(
            events[1].kind,
            EventKind::InterfaceCounterDelta {
                baseline_available: false,
                sample_available: false,
                ..
            }
        ));
        assert!(matches!(
            events[2].kind,
            EventKind::ProbeResult {
                method: ProbeMethod::TcpConnect,
                outcome: ProbeOutcome::Success,
                ..
            }
        ));
        assert!(matches!(events[3].kind, EventKind::ProbeResult { .. }));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn configured_dns_event_follows_tcp_events_and_uses_same_timeout() {
        let mut runner = ProbeRunner::new(
            FakeTransport {
                calls: Rc::new(Cell::new(0)),
            },
            FakeClock {
                monotonic: RefCell::new(VecDeque::from([
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                ])),
            },
        );
        let dns_timeouts = Rc::new(RefCell::new(Vec::new()));
        let mut dns_runner = DnsRunner::new(
            FakeDnsTransport(Rc::clone(&dns_timeouts)),
            FakeClock {
                monotonic: RefCell::new(VecDeque::from([Duration::ZERO, Duration::from_millis(7)])),
            },
        );
        let config = MonitorConfig::new(
            DEFAULT_INTERVAL_MILLIS,
            250,
            vec![target("192.0.2.1"), target("192.0.2.2")],
        )
        .unwrap()
        .with_dns_name(Some(DnsName::parse("example.com.").unwrap()));
        let mut interface_tracker = InterfaceTracker::default();
        let events = collect_cycle_with_dns(
            &config,
            &mut interface_tracker,
            &mut runner,
            &mut dns_runner,
            CycleInput {
                discovery: &discovery(),
                stop: &AtomicBool::new(false),
                recorded_at: "2026-10-13T00:00:00Z".parse().unwrap(),
            },
            || "id".to_owned(),
        )
        .unwrap();
        assert!(matches!(
            events.last().unwrap().kind,
            EventKind::DnsResult { elapsed_ms: 7, .. }
        ));
        assert_eq!(
            dns_timeouts.borrow().as_slice(),
            &[Duration::from_millis(250)]
        );
        assert!(events[..events.len() - 1]
            .iter()
            .all(|event| !matches!(event.kind, EventKind::DnsResult { .. })));
    }

    #[test]
    fn maximum_interface_inventory_keeps_the_cycle_under_the_storage_batch_limit() {
        let mut tracker = InterfaceTracker::default();
        let mut runner = ProbeRunner::new(
            FakeTransport {
                calls: Rc::new(Cell::new(0)),
            },
            FakeClock {
                monotonic: RefCell::new(VecDeque::new()),
            },
        );
        let config = MonitorConfig::new(
            DEFAULT_INTERVAL_MILLIS,
            crate::probes::DEFAULT_TIMEOUT_MILLIS,
            vec![target("192.0.2.1"), target("192.0.2.2")],
        )
        .unwrap();
        let stop = AtomicBool::new(true);
        let at = "2026-10-13T00:00:00Z".parse().unwrap();
        let make_discovery = |carrier, speed_mbps| Discovery {
            interfaces: (0..MAX_INTERFACES_PER_CYCLE)
                .map(|index| InterfaceState {
                    name: format!("test{index}"),
                    operational_state: OperationalState::Unknown,
                    carrier: Some(carrier),
                    speed_mbps: Some(speed_mbps),
                    is_default_route: false,
                    counters: Default::default(),
                })
                .collect(),
            default_route: None,
        };
        let mut ids = 0;
        let baseline = collect_cycle(
            &make_discovery(false, 100),
            &config,
            &mut tracker,
            &mut runner,
            &stop,
            at,
            || {
                ids += 1;
                format!("test-{ids}")
            },
        )
        .unwrap();
        assert_eq!(baseline.len(), MAX_INTERFACES_PER_CYCLE * 2);

        let changed = collect_cycle(
            &make_discovery(true, 1000),
            &config,
            &mut tracker,
            &mut runner,
            &stop,
            at,
            || {
                ids += 1;
                format!("test-{ids}")
            },
        )
        .unwrap();
        assert_eq!(changed.len(), MAX_INTERFACES_PER_CYCLE * 4);
        assert!(
            changed.len() + MAX_TARGETS + 1 + crate::journal::MAX_ENTRIES
                <= crate::storage::MAX_WRITE_BATCH_SIZE
        );
    }

    #[test]
    fn oversized_interface_inventory_fails_before_creating_events_or_probing() {
        let calls = Rc::new(Cell::new(0));
        let mut runner = ProbeRunner::new(
            FakeTransport {
                calls: Rc::clone(&calls),
            },
            FakeClock {
                monotonic: RefCell::new(VecDeque::new()),
            },
        );
        let config = MonitorConfig::new(
            DEFAULT_INTERVAL_MILLIS,
            crate::probes::DEFAULT_TIMEOUT_MILLIS,
            vec![target("192.0.2.1"), target("192.0.2.2")],
        )
        .unwrap();
        let mut oversized = discovery();
        oversized.interfaces = (0..=MAX_INTERFACES_PER_CYCLE)
            .map(|index| InterfaceState {
                name: format!("test{index}"),
                operational_state: OperationalState::Unknown,
                carrier: None,
                speed_mbps: None,
                is_default_route: false,
                counters: Default::default(),
            })
            .collect();
        let mut tracker = InterfaceTracker::default();
        let mut generated_ids = 0;
        let error = collect_cycle(
            &oversized,
            &config,
            &mut tracker,
            &mut runner,
            &AtomicBool::new(false),
            "2026-10-13T00:00:00Z".parse().unwrap(),
            || {
                generated_ids += 1;
                format!("test-{generated_ids}")
            },
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error
            .to_string()
            .contains("refusing to truncate observations"));
        assert_eq!(generated_ids, 0);
        assert!(tracker.previous.is_empty());
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn unavailable_optional_journal_keeps_cycle_events_and_warns_only_once() {
        struct DeniedReader;
        impl crate::journal::KernelJournalReader for DeniedReader {
            fn read_since(
                &mut self,
                _cursor: Option<&str>,
            ) -> Result<Vec<u8>, crate::journal::JournalError> {
                Err(crate::journal::JournalError::Unavailable)
            }
        }

        let mut collector = crate::journal::KernelJournalCollector::new(DeniedReader);
        let mut events = vec![Event {
            schema_version: crate::event::SCHEMA_VERSION,
            id: "existing".into(),
            recorded_at_utc: "2026-10-13T00:00:00Z".parse().unwrap(),
            source: "interface".into(),
            kind: EventKind::LinkStateChanged {
                interface: "eno1".into(),
                carrier: Some(false),
            },
        }];
        let mut warning_reported = false;
        assert_eq!(
            append_journal_events(
                &mut events,
                &mut collector,
                "2026-10-13T00:00:00Z".parse().unwrap(),
                &mut warning_reported,
                || "journal".into(),
            ),
            Some(crate::journal::JournalError::Unavailable)
        );
        assert_eq!(events.len(), 1);
        assert!(warning_reported);
        assert_eq!(
            append_journal_events(
                &mut events,
                &mut collector,
                "2026-10-13T00:00:00Z".parse().unwrap(),
                &mut warning_reported,
                || "journal".into(),
            ),
            None
        );
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn normalized_journal_signal_is_persisted_in_the_same_cycle() {
        struct SignalReader;
        impl crate::journal::KernelJournalReader for SignalReader {
            fn read_since(
                &mut self,
                _cursor: Option<&str>,
            ) -> Result<Vec<u8>, crate::journal::JournalError> {
                Ok(br#"{"__CURSOR":"cursor","MESSAGE":"eno1: Link is Down","__REALTIME_TIMESTAMP":"1791849600000000"}"#.to_vec())
            }
        }

        let mut collector = crate::journal::KernelJournalCollector::new(SignalReader);
        let mut events = Vec::new();
        let mut warning_reported = false;
        assert_eq!(
            append_journal_events(
                &mut events,
                &mut collector,
                "2026-10-13T00:00:00Z".parse().unwrap(),
                &mut warning_reported,
                || "journal-event".into(),
            ),
            None
        );
        let mut store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
        let mut output = Vec::new();
        persist_then_write(&mut store, &events, &mut output).unwrap();
        let stored = store.read_events_after(0, 10).unwrap();
        assert_eq!(stored.len(), 1);
        assert!(matches!(
            stored[0].event.kind,
            EventKind::KernelLinkSignal {
                category: crate::event::KernelLinkSignalCategory::LinkDown,
                interface: Some(ref interface)
            } if interface == "eno1"
        ));
        assert_eq!(
            output
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .count(),
            1
        );
    }

    fn sample(
        carrier: Option<bool>,
        speed_mbps: Option<u64>,
        counters: interfaces::InterfaceCounters,
    ) -> Discovery {
        Discovery {
            interfaces: vec![InterfaceState {
                name: "test0".into(),
                operational_state: OperationalState::Up,
                carrier,
                speed_mbps,
                is_default_route: true,
                counters,
            }],
            default_route: Some("test0".into()),
        }
    }

    #[test]
    fn interface_tracker_emits_ordered_transitions_and_counter_deltas() {
        let mut tracker = InterfaceTracker::default();
        let first = sample(
            Some(true),
            Some(1000),
            interfaces::InterfaceCounters {
                rx_errors: Some(10),
                tx_errors: Some(20),
                rx_dropped: Some(3),
                tx_dropped: Some(4),
            },
        );
        let baseline =
            tracker.observe(&first, "2026-10-13T00:00:00Z".parse().unwrap(), &mut || {
                "first".to_owned()
            });
        assert_eq!(baseline.len(), 2);
        assert!(matches!(
            baseline[1].kind,
            EventKind::InterfaceCounterDelta {
                baseline_available: false,
                sample_available: true,
                ..
            }
        ));

        let second = sample(
            Some(false),
            Some(100),
            interfaces::InterfaceCounters {
                rx_errors: Some(14),
                tx_errors: Some(25),
                rx_dropped: Some(5),
                tx_dropped: Some(9),
            },
        );
        let changes = tracker.observe(
            &second,
            "2026-10-13T00:00:10Z".parse().unwrap(),
            &mut || "second".to_owned(),
        );
        assert!(matches!(
            changes[1].kind,
            EventKind::LinkStateChanged {
                carrier: Some(false),
                ..
            }
        ));
        assert!(matches!(
            changes[2].kind,
            EventKind::LinkSpeedChanged {
                previous_speed_mbps: Some(1000),
                speed_mbps: Some(100),
                ..
            }
        ));
        assert!(matches!(
            changes[3].kind,
            EventKind::InterfaceCounterDelta {
                baseline_available: true,
                rx_errors_delta: Some(4),
                tx_errors_delta: Some(5),
                rx_dropped_delta: Some(2),
                tx_dropped_delta: Some(5),
                counter_reset_detected: false,
                ..
            }
        ));

        let third = sample(
            Some(false),
            None,
            interfaces::InterfaceCounters {
                rx_errors: Some(2),
                tx_errors: None,
                rx_dropped: Some(8),
                tx_dropped: Some(10),
            },
        );
        let reset = tracker.observe(&third, "2026-10-13T00:00:20Z".parse().unwrap(), &mut || {
            "third".to_owned()
        });
        assert!(matches!(
            reset[1].kind,
            EventKind::LinkSpeedChanged {
                previous_speed_mbps: Some(100),
                speed_mbps: None,
                ..
            }
        ));
        assert!(matches!(
            reset[2].kind,
            EventKind::InterfaceCounterDelta {
                counter_reset_detected: true,
                rx_errors_delta: None,
                tx_errors_delta: None,
                rx_dropped_delta: Some(3),
                tx_dropped_delta: Some(1),
                ..
            }
        ));
    }

    #[test]
    fn disappeared_interface_restarts_its_counter_baseline() {
        let mut tracker = InterfaceTracker::default();
        let counters = interfaces::InterfaceCounters {
            rx_errors: Some(3),
            ..Default::default()
        };
        tracker.observe(
            &sample(Some(true), Some(100), counters),
            "2026-10-13T00:00:00Z".parse().unwrap(),
            &mut || "a".to_owned(),
        );
        tracker.observe(
            &Discovery {
                interfaces: Vec::new(),
                default_route: None,
            },
            "2026-10-13T00:00:01Z".parse().unwrap(),
            &mut || "b".to_owned(),
        );
        let returned = tracker.observe(
            &sample(
                Some(true),
                Some(100),
                interfaces::InterfaceCounters {
                    rx_errors: Some(200),
                    ..Default::default()
                },
            ),
            "2026-10-13T00:00:02Z".parse().unwrap(),
            &mut || "c".to_owned(),
        );
        assert!(matches!(
            returned[1].kind,
            EventKind::InterfaceCounterDelta {
                baseline_available: false,
                rx_errors_delta: None,
                ..
            }
        ));
    }

    #[test]
    fn cycle_is_not_emitted_when_storage_transaction_fails() {
        let mut store = EventStore::open_in_memory(RetentionPolicy::default()).unwrap();
        let existing = test_event("duplicate");
        store.store_events(&[existing.clone()]).unwrap();
        let events = [existing.clone(), existing];
        let mut output = Vec::new();

        assert!(persist_then_write(&mut store, &events, &mut output).is_err());
        assert!(output.is_empty());
        assert_eq!(store.event_count().unwrap(), 1);
    }

    fn test_event(id: &str) -> Event {
        Event {
            schema_version: crate::event::SCHEMA_VERSION,
            id: id.to_owned(),
            recorded_at_utc: "2026-10-13T00:00:00Z".parse().unwrap(),
            source: "interface".to_owned(),
            kind: EventKind::InterfaceSnapshot {
                interface: "test0".to_owned(),
                operational_state: "up".to_owned(),
                carrier: Some(true),
                speed_mbps: Some(1000),
                is_default_route: Some(true),
            },
        }
    }

    #[test]
    fn interrupted_interval_returns_without_waiting() {
        let stop = AtomicBool::new(true);
        wait_interval(Duration::from_secs(60), &stop);
    }
}
