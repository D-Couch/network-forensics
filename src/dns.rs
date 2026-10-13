//! Explicit, bounded DNS observations with an injectable lookup boundary.

use crate::event::{
    DnsFailureCategory, DnsLimitationCategory, DnsOutcome, Event, EventKind, SCHEMA_VERSION,
};
use crate::probes::ProbeClock;
use hickory_resolver::TokioResolver;
use std::net::IpAddr;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsName(String);

impl DnsName {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        if value.len() > 254 || !value.is_ascii() || !value.ends_with('.') {
            return Err("DNS name must be an ASCII fully qualified name ending in a dot");
        }
        let body = &value[..value.len() - 1];
        if body.is_empty()
            || body.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
        {
            return Err("DNS name contains an invalid label");
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub trait DnsTransport {
    fn lookup(&mut self, name: &DnsName, timeout: Duration) -> Result<Vec<IpAddr>, DnsLookupError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsLookupError {
    Timeout,
    NameNotFound,
    NoAddressRecords,
    ResolverFailure,
    ResolverUnavailable,
}

#[derive(Debug)]
pub struct SystemDnsTransport {
    resolver: Option<TokioResolver>,
    _runtime: tokio::runtime::Runtime,
}

impl SystemDnsTransport {
    pub fn new(timeout: Duration) -> Result<Self, std::io::Error> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let resolver = runtime.block_on(async {
            let mut builder = TokioResolver::builder_tokio().ok()?;
            let options = builder.options_mut();
            options.timeout = timeout;
            options.attempts = 1;
            options.use_hosts_file = hickory_resolver::config::ResolveHosts::Never;
            builder.build().ok()
        });
        Ok(Self {
            resolver,
            _runtime: runtime,
        })
    }
}

impl DnsTransport for SystemDnsTransport {
    fn lookup(&mut self, name: &DnsName, timeout: Duration) -> Result<Vec<IpAddr>, DnsLookupError> {
        let Some(resolver) = self.resolver.as_ref() else {
            return Err(DnsLookupError::ResolverUnavailable);
        };
        let result = self._runtime.block_on(async {
            tokio::time::timeout(timeout, resolver.lookup_ip(name.as_str())).await
        });
        match result {
            Err(_) => Err(DnsLookupError::Timeout),
            Ok(Err(hickory_resolver::net::NetError::Timeout)) => Err(DnsLookupError::Timeout),
            Ok(Err(error)) if error.is_nx_domain() => Err(DnsLookupError::NameNotFound),
            Ok(Err(error)) if error.is_no_records_found() => Err(DnsLookupError::NoAddressRecords),
            Ok(Err(_)) => Err(DnsLookupError::ResolverFailure),
            Ok(Ok(lookup)) => {
                let addresses = lookup.iter().collect::<Vec<_>>();
                if addresses.is_empty() {
                    Err(DnsLookupError::NoAddressRecords)
                } else {
                    Ok(addresses)
                }
            }
        }
    }
}

pub struct DnsRunner<T, C> {
    transport: T,
    clock: C,
}

impl<T: DnsTransport, C: ProbeClock> DnsRunner<T, C> {
    pub fn new(transport: T, clock: C) -> Self {
        Self { transport, clock }
    }

    pub fn run(&mut self, id: String, name: &DnsName, timeout: Duration) -> Event {
        let started_at_utc = self.clock.utc_now();
        let started = self.clock.monotonic_now();
        let outcome = match self.transport.lookup(name, timeout) {
            Ok(addresses) => DnsOutcome::Success {
                answer_count: addresses.len().min(u16::MAX as usize) as u16,
            },
            Err(DnsLookupError::Timeout) => DnsOutcome::Timeout,
            Err(DnsLookupError::NameNotFound) => {
                DnsOutcome::Failure(DnsFailureCategory::NameNotFound)
            }
            Err(DnsLookupError::NoAddressRecords) => {
                DnsOutcome::Failure(DnsFailureCategory::NoAddressRecords)
            }
            Err(DnsLookupError::ResolverFailure) => {
                DnsOutcome::Failure(DnsFailureCategory::ResolverFailure)
            }
            Err(DnsLookupError::ResolverUnavailable) => {
                DnsOutcome::UnsupportedOrBlocked(DnsLimitationCategory::ResolverUnavailable)
            }
        };
        let elapsed_ms = self
            .clock
            .monotonic_now()
            .saturating_sub(started)
            .as_millis()
            .min(u64::MAX as u128) as u64;
        Event {
            schema_version: SCHEMA_VERSION,
            id,
            recorded_at_utc: started_at_utc,
            source: "dns".to_owned(),
            kind: EventKind::DnsResult {
                query_name: name.0.clone(),
                started_at_utc,
                elapsed_ms,
                outcome,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct FakeTransport(Result<Vec<IpAddr>, DnsLookupError>, Vec<Duration>);
    impl DnsTransport for FakeTransport {
        fn lookup(
            &mut self,
            _name: &DnsName,
            timeout: Duration,
        ) -> Result<Vec<IpAddr>, DnsLookupError> {
            self.1.push(timeout);
            self.0.clone()
        }
    }
    struct FakeClock(RefCell<VecDeque<Duration>>);
    impl ProbeClock for FakeClock {
        fn utc_now(&self) -> DateTime<Utc> {
            "2026-10-13T00:00:00Z".parse().unwrap()
        }
        fn monotonic_now(&self) -> Duration {
            self.0.borrow_mut().pop_front().unwrap()
        }
    }

    #[test]
    fn validates_fully_qualified_ascii_dns_names() {
        assert_eq!(
            DnsName::parse("Example.COM.").unwrap().as_str(),
            "example.com."
        );
        for invalid in [
            "example.com",
            ".",
            "a..b.",
            "-a.example.",
            "a-.example.",
            "tést.example.",
        ] {
            assert!(DnsName::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn runner_records_count_without_storing_answer_addresses() {
        let mut runner = DnsRunner::new(
            FakeTransport(
                Ok(vec![
                    "192.0.2.1".parse().unwrap(),
                    "2001:db8::1".parse().unwrap(),
                ]),
                Vec::new(),
            ),
            FakeClock(RefCell::new(VecDeque::from([
                Duration::from_millis(10),
                Duration::from_millis(35),
            ]))),
        );
        let event = runner.run(
            "dns-1".into(),
            &DnsName::parse("example.com.").unwrap(),
            Duration::from_secs(2),
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("example.com."));
        assert!(json.contains("answer_count"));
        assert!(!json.contains("192.0.2.1"));
        assert!(!json.contains("2001:db8::1"));
        let EventKind::DnsResult {
            elapsed_ms,
            outcome,
            ..
        } = event.kind
        else {
            panic!("DNS event expected")
        };
        assert_eq!(elapsed_ms, 25);
        assert_eq!(outcome, DnsOutcome::Success { answer_count: 2 });
    }

    #[test]
    fn runner_keeps_failure_kinds_distinct() {
        for (error, expected) in [
            (DnsLookupError::Timeout, DnsOutcome::Timeout),
            (
                DnsLookupError::NameNotFound,
                DnsOutcome::Failure(DnsFailureCategory::NameNotFound),
            ),
            (
                DnsLookupError::ResolverUnavailable,
                DnsOutcome::UnsupportedOrBlocked(DnsLimitationCategory::ResolverUnavailable),
            ),
        ] {
            let mut runner = DnsRunner::new(
                FakeTransport(Err(error), Vec::new()),
                FakeClock(RefCell::new(VecDeque::from([
                    Duration::ZERO,
                    Duration::ZERO,
                ]))),
            );
            let event = runner.run(
                "dns-1".into(),
                &DnsName::parse("example.com.").unwrap(),
                Duration::from_secs(1),
            );
            let EventKind::DnsResult { outcome, .. } = event.kind else {
                panic!("DNS event expected")
            };
            assert_eq!(outcome, expected);
        }
    }
}
