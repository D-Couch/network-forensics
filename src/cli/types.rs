use crate::{monitor_state, storage};

#[derive(Default)]
pub(super) struct StatusSummary {
    pub(super) event_count: usize,
    pub(super) latest_event: Option<(i64, chrono::DateTime<chrono::Utc>)>,
    pub(super) latest_probe: Option<storage::StoredEvent>,
    pub(super) latest_interface: Option<storage::StoredEvent>,
    pub(super) latest_counters: Option<storage::StoredEvent>,
    pub(super) latest_dns: Option<storage::StoredEvent>,
    pub(super) latest_kernel_signal: Option<storage::StoredEvent>,
    pub(super) monitor: monitor_state::MonitorStatus,
}
