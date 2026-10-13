//! Foreground observation rounds and interruptible interval waiting.

use crate::event::{Event, EventKind, ProbeRole};
use crate::interfaces::{self, Discovery};
use crate::probes::{
    ProbeClock, ProbeConfig, ProbeRunner, ProbeTarget, ProbeTransport, SystemClock, SystemTransport,
};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::io::{self, BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

pub const DEFAULT_INTERVAL_MILLIS: u64 = 10_000;
pub const MIN_INTERVAL_MILLIS: u64 = 1_000;
pub const MAX_INTERVAL_MILLIS: u64 = 3_600_000;
pub const MAX_TARGETS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorConfig {
    pub interval: Duration,
    pub probe: ProbeConfig,
    pub targets: Vec<ProbeTarget>,
}

impl MonitorConfig {
    pub fn new(
        interval_millis: u64,
        timeout_millis: u64,
        targets: Vec<ProbeTarget>,
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
        })
    }
}

/// Emits one newline-delimited JSON event for every interface snapshot and
/// configured probe. The stop flag is checked before each new probe.
pub fn emit_cycle<T, C, W, F>(
    discovery: &Discovery,
    config: &MonitorConfig,
    runner: &mut ProbeRunner<T, C>,
    stop: &AtomicBool,
    cycle_time: DateTime<Utc>,
    writer: &mut W,
    mut next_id: F,
) -> io::Result<()>
where
    T: ProbeTransport,
    C: ProbeClock,
    W: Write,
    F: FnMut() -> String,
{
    for interface in &discovery.interfaces {
        let event = Event {
            schema_version: crate::event::SCHEMA_VERSION,
            id: next_id(),
            recorded_at_utc: cycle_time,
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
        };
        write_event(writer, &event)?;
    }

    for target in &config.targets {
        if stop.load(Ordering::Acquire) {
            break;
        }
        let event = runner.run(next_id(), *target, config.probe);
        write_event(writer, &event)?;
    }
    Ok(())
}

fn write_event<W: Write>(writer: &mut W, event: &Event) -> io::Result<()> {
    let serialized = serde_json::to_string(event).map_err(io::Error::other)?;
    writeln!(writer, "{serialized}")
}

/// Runs until Ctrl+C. A currently active TCP connect is allowed to finish or
/// time out before shutdown; no new probes start after the signal is observed.
pub fn run_foreground(config: MonitorConfig) -> io::Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    let signal_stop = Arc::clone(&stop);
    ctrlc::set_handler(move || signal_stop.store(true, Ordering::Release))
        .map_err(io::Error::other)?;

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let mut runner = ProbeRunner::new(SystemTransport, SystemClock::new());

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
        emit_cycle(
            &discovery,
            &config,
            &mut runner,
            &stop,
            cycle_time,
            &mut writer,
            crate::next_event_id,
        )?;
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
        let mut output = Vec::new();
        emit_cycle(
            &discovery(),
            &MonitorConfig::new(
                DEFAULT_INTERVAL_MILLIS,
                crate::probes::DEFAULT_TIMEOUT_MILLIS,
                vec![target("192.0.2.1"), target("192.0.2.2")],
            )
            .unwrap(),
            &mut runner,
            &stop,
            "2026-10-13T00:00:00Z".parse().unwrap(),
            &mut output,
            || {
                ids += 1;
                format!("test-{ids}")
            },
        )
        .unwrap();
        let lines: Vec<_> = std::str::from_utf8(&output).unwrap().lines().collect();
        assert_eq!(lines.len(), 3);
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
            EventKind::ProbeResult {
                method: ProbeMethod::TcpConnect,
                outcome: ProbeOutcome::Success,
                ..
            }
        ));
        assert!(matches!(
            events[2].kind,
            EventKind::ProbeResult {
                outcome: ProbeOutcome::Success,
                ..
            }
        ));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn interrupted_interval_returns_without_waiting() {
        let stop = AtomicBool::new(true);
        wait_interval(Duration::from_secs(60), &stop);
    }
}
