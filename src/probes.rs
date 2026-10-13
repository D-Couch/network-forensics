//! One-shot, bounded TCP-connect probes with injectable transport and clocks.

use crate::event::{
    Event, EventKind, ProbeFailureCategory, ProbeLimitationCategory, ProbeMethod, ProbeOutcome,
    ProbeRole, SCHEMA_VERSION,
};
use chrono::{DateTime, Utc};
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant, SystemTime};

pub const DEFAULT_TIMEOUT_MILLIS: u64 = 3_000;
pub const MAX_TIMEOUT_MILLIS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeTarget {
    pub role: ProbeRole,
    pub address: SocketAddr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeConfig {
    timeout: Duration,
}

impl ProbeConfig {
    pub fn from_millis(timeout_millis: u64) -> Result<Self, &'static str> {
        if !(1..=MAX_TIMEOUT_MILLIS).contains(&timeout_millis) {
            return Err("timeout must be between 1 and 30000 milliseconds");
        }
        Ok(Self {
            timeout: Duration::from_millis(timeout_millis),
        })
    }

    pub fn timeout(self) -> Duration {
        self.timeout
    }
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MILLIS),
        }
    }
}

pub trait ProbeTransport {
    fn tcp_connect(&mut self, target: SocketAddr, timeout: Duration) -> io::Result<()>;
}

pub trait ProbeClock {
    fn utc_now(&self) -> DateTime<Utc>;
    fn monotonic_now(&self) -> Duration;
}

#[derive(Debug, Default)]
pub struct SystemTransport;

impl ProbeTransport for SystemTransport {
    fn tcp_connect(&mut self, target: SocketAddr, timeout: Duration) -> io::Result<()> {
        TcpStream::connect_timeout(&target, timeout).map(drop)
    }
}

#[derive(Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ProbeClock for SystemClock {
    fn utc_now(&self) -> DateTime<Utc> {
        DateTime::<Utc>::from(SystemTime::now())
    }

    fn monotonic_now(&self) -> Duration {
        self.origin.elapsed()
    }
}

pub struct ProbeRunner<T, C> {
    transport: T,
    clock: C,
}

impl<T, C> ProbeRunner<T, C>
where
    T: ProbeTransport,
    C: ProbeClock,
{
    pub fn new(transport: T, clock: C) -> Self {
        Self { transport, clock }
    }

    /// Runs one explicitly configured target and returns a versioned event.
    /// The caller supplies the event ID and owns its uniqueness semantics.
    pub fn run(&mut self, id: String, target: ProbeTarget, config: ProbeConfig) -> Event {
        let started_at_utc = self.clock.utc_now();
        let started_monotonic = self.clock.monotonic_now();
        let outcome = match self.transport.tcp_connect(target.address, config.timeout()) {
            Ok(()) => ProbeOutcome::Success,
            Err(error) => classify_error(error.kind()),
        };
        let elapsed_ms = self
            .clock
            .monotonic_now()
            .saturating_sub(started_monotonic)
            .as_millis()
            .min(u64::MAX as u128) as u64;

        Event {
            schema_version: SCHEMA_VERSION,
            id,
            recorded_at_utc: started_at_utc,
            source: "probe".to_owned(),
            kind: EventKind::ProbeResult {
                role: target.role,
                target: target.address,
                method: ProbeMethod::TcpConnect,
                started_at_utc,
                elapsed_ms,
                outcome,
            },
        }
    }
}

fn classify_error(kind: io::ErrorKind) -> ProbeOutcome {
    match kind {
        io::ErrorKind::TimedOut => ProbeOutcome::Timeout,
        io::ErrorKind::PermissionDenied => {
            ProbeOutcome::UnsupportedOrBlocked(ProbeLimitationCategory::PermissionDenied)
        }
        io::ErrorKind::Unsupported => {
            ProbeOutcome::UnsupportedOrBlocked(ProbeLimitationCategory::MethodUnsupported)
        }
        io::ErrorKind::ConnectionRefused => {
            ProbeOutcome::Failure(ProbeFailureCategory::ConnectionRefused)
        }
        io::ErrorKind::NetworkUnreachable => {
            ProbeOutcome::Failure(ProbeFailureCategory::NetworkUnreachable)
        }
        io::ErrorKind::HostUnreachable => {
            ProbeOutcome::Failure(ProbeFailureCategory::HostUnreachable)
        }
        _ => ProbeOutcome::Failure(ProbeFailureCategory::Other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    struct FakeTransport {
        error: Option<io::ErrorKind>,
        calls: RefCell<Vec<(SocketAddr, Duration)>>,
    }

    impl ProbeTransport for FakeTransport {
        fn tcp_connect(&mut self, target: SocketAddr, timeout: Duration) -> io::Result<()> {
            self.calls.borrow_mut().push((target, timeout));
            match self.error {
                Some(kind) => Err(io::Error::from(kind)),
                None => Ok(()),
            }
        }
    }

    struct FakeClock {
        wall: DateTime<Utc>,
        monotonic_samples: RefCell<VecDeque<Duration>>,
        wall_reads: Cell<u8>,
    }

    impl ProbeClock for FakeClock {
        fn utc_now(&self) -> DateTime<Utc> {
            self.wall_reads.set(self.wall_reads.get() + 1);
            self.wall
        }

        fn monotonic_now(&self) -> Duration {
            self.monotonic_samples.borrow_mut().pop_front().unwrap()
        }
    }

    fn runner(error: Option<io::ErrorKind>) -> ProbeRunner<FakeTransport, FakeClock> {
        ProbeRunner::new(
            FakeTransport {
                error,
                calls: RefCell::new(Vec::new()),
            },
            FakeClock {
                wall: "2026-10-13T00:00:00Z".parse().unwrap(),
                monotonic_samples: RefCell::new(VecDeque::from([
                    Duration::from_millis(100),
                    Duration::from_millis(117),
                ])),
                wall_reads: Cell::new(0),
            },
        )
    }

    fn target() -> ProbeTarget {
        ProbeTarget {
            role: ProbeRole::External,
            address: "203.0.113.8:443".parse().unwrap(),
        }
    }

    #[test]
    fn records_success_with_utc_start_monotonic_elapsed_and_timeout() {
        let mut runner = runner(None);
        let config = ProbeConfig::from_millis(250).unwrap();
        let event = runner.run("probe-1".into(), target(), config);
        let EventKind::ProbeResult {
            target: address,
            started_at_utc,
            elapsed_ms,
            outcome,
            ..
        } = event.kind
        else {
            panic!("expected probe result event");
        };

        assert_eq!(address, target().address);
        assert_eq!(started_at_utc.to_rfc3339(), "2026-10-13T00:00:00+00:00");
        assert_eq!(elapsed_ms, 17);
        assert_eq!(outcome, ProbeOutcome::Success);
        assert_eq!(runner.clock.wall_reads.get(), 1);
        assert_eq!(
            runner.transport.calls.borrow().as_slice(),
            &[(target().address, Duration::from_millis(250))]
        );
    }

    #[test]
    fn classifies_timeout_rejection_and_blocked_probe_separately() {
        for (error, expected) in [
            (io::ErrorKind::TimedOut, ProbeOutcome::Timeout),
            (
                io::ErrorKind::ConnectionRefused,
                ProbeOutcome::Failure(ProbeFailureCategory::ConnectionRefused),
            ),
            (
                io::ErrorKind::PermissionDenied,
                ProbeOutcome::UnsupportedOrBlocked(ProbeLimitationCategory::PermissionDenied),
            ),
        ] {
            let mut runner = runner(Some(error));
            let event = runner.run("probe-1".into(), target(), ProbeConfig::default());
            let EventKind::ProbeResult { outcome, .. } = event.kind else {
                panic!("expected probe result event");
            };
            assert_eq!(outcome, expected);
        }
    }

    #[test]
    fn timeout_is_bounded_and_rejects_zero_or_over_limit() {
        assert!(ProbeConfig::from_millis(0).is_err());
        assert!(ProbeConfig::from_millis(MAX_TIMEOUT_MILLIS + 1).is_err());
        assert_eq!(
            ProbeConfig::from_millis(MAX_TIMEOUT_MILLIS)
                .unwrap()
                .timeout(),
            Duration::from_millis(MAX_TIMEOUT_MILLIS)
        );
    }
}
