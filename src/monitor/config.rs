//! Validated settings and limits shared by monitor collection and orchestration.

use crate::dns::DnsName;
use crate::event::ProbeRole;
use crate::probes::{ProbeConfig, ProbeTarget};
use crate::storage::RetentionPolicy;
use std::collections::HashSet;
use std::time::Duration;

pub const DEFAULT_INTERVAL_MILLIS: u64 = 10_000;
pub const MIN_INTERVAL_MILLIS: u64 = 1_000;
pub const MAX_INTERVAL_MILLIS: u64 = 3_600_000;
pub const MAX_TARGETS: usize = 8;
/// Leaves room in the 4096-event storage batch for up to four events per
/// interface, all configured probes, one DNS result, and bounded journal data.
pub const MAX_INTERFACES_PER_CYCLE: usize = 900;

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
