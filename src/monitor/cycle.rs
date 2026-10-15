//! Interface transition tracking and observation collection for one monitor round.

use super::config::{MonitorConfig, MAX_INTERFACES_PER_CYCLE};
use crate::dns::{DnsRunner, DnsTransport};
use crate::event::{Event, EventKind};
use crate::interfaces::{Discovery, InterfaceState};
use crate::probes::{ProbeClock, ProbeRunner, ProbeTransport};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Default)]
pub struct InterfaceTracker {
    pub(super) previous: HashMap<String, InterfaceState>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;
    use crate::interfaces::{self, InterfaceState, OperationalState};

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
}
