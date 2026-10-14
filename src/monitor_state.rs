//! Private monitor configuration and kernel-lock-backed lifecycle status.

use crate::monitor::MonitorConfig;
#[cfg(not(target_os = "linux"))]
use std::io;

#[derive(Debug, Default)]
pub enum MonitorStatus {
    #[default]
    NoRecord,
    Running(Option<StoredMonitorConfig>),
    Stopped(Option<StoredMonitorConfig>),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredMonitorConfig {
    interval_ms: u64,
    timeout_ms: u64,
    targets: Vec<StoredTarget>,
    retention_days: u32,
    max_events: usize,
    dns_name: Option<String>,
    kernel_journal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredTarget {
    role: crate::event::ProbeRole,
    address: String,
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{MonitorConfig, MonitorStatus, StoredMonitorConfig, StoredTarget};
    use crate::dns::DnsName;
    use crate::event::ProbeRole;
    use crate::monitor::{MAX_INTERVAL_MILLIS, MAX_TARGETS, MIN_INTERVAL_MILLIS};
    use crate::storage::{self, RetentionPolicy, MAX_EVENT_COUNT, MAX_RETENTION_DAYS};
    use nix::errno::Errno;
    use nix::fcntl::{Flock, FlockArg};
    use std::env;
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Read, Seek, SeekFrom, Write};
    use std::net::SocketAddr;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    const STATE_FILE_NAME: &str = "monitor-state.json";
    const STATE_VERSION: u8 = 1;
    const MAX_STATE_BYTES: u64 = 8 * 1024;

    pub struct MonitorGuard {
        _lock: Flock<File>,
    }

    impl MonitorGuard {
        pub fn acquire_default(config: &MonitorConfig) -> io::Result<Self> {
            let path = default_state_path()?;
            let stored = StoredMonitorConfig::from_config(config);
            Self::acquire_at(&path, &stored)
        }

        fn acquire_at(path: &Path, stored: &StoredMonitorConfig) -> io::Result<Self> {
            stored.validate()?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .mode(0o600)
                .custom_flags(nix::libc::O_CLOEXEC | nix::libc::O_NOFOLLOW)
                .open(path)?;
            if !file.metadata()?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "monitor state path is not a regular file",
                ));
            }
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            let mut lock =
                Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|(file, error)| {
                    drop(file);
                    if error == Errno::EWOULDBLOCK || error == Errno::EAGAIN {
                        io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "another netforensics monitor is already running",
                        )
                    } else {
                        io::Error::from_raw_os_error(error as i32)
                    }
                })?;
            lock.set_len(0)?;
            lock.seek(SeekFrom::Start(0))?;
            serde_json::to_writer(
                &mut *lock,
                &StateRecord {
                    version: STATE_VERSION,
                    config: stored.clone(),
                },
            )
            .map_err(io::Error::other)?;
            lock.write_all(b"\n")?;
            lock.sync_all()?;
            Ok(Self { _lock: lock })
        }
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StateRecord {
        version: u8,
        config: StoredMonitorConfig,
    }

    impl StoredMonitorConfig {
        fn from_config(config: &MonitorConfig) -> Self {
            Self {
                interval_ms: config.interval.as_millis() as u64,
                timeout_ms: config.probe.timeout().as_millis() as u64,
                targets: config
                    .targets
                    .iter()
                    .map(|target| StoredTarget {
                        role: target.role,
                        address: target.address.to_string(),
                    })
                    .collect(),
                retention_days: config.retention.retention_days(),
                max_events: config.retention.max_events(),
                dns_name: config
                    .dns_name
                    .as_ref()
                    .map(|name| name.as_str().to_owned()),
                kernel_journal: config.kernel_journal,
            }
        }

        fn validate(&self) -> io::Result<()> {
            if !(MIN_INTERVAL_MILLIS..=MAX_INTERVAL_MILLIS).contains(&self.interval_ms)
                || !(1..=30_000).contains(&self.timeout_ms)
                || !(1..=MAX_RETENTION_DAYS).contains(&self.retention_days)
                || !(1..=MAX_EVENT_COUNT).contains(&self.max_events)
                || self.targets.len() < 2
                || self.targets.len() > MAX_TARGETS
            {
                return Err(invalid_record());
            }
            RetentionPolicy::new(self.retention_days, self.max_events)
                .map_err(|_| invalid_record())?;
            if self
                .dns_name
                .as_deref()
                .is_some_and(|name| DnsName::parse(name).is_err())
            {
                return Err(invalid_record());
            }
            let mut addresses = std::collections::HashSet::new();
            let mut external_ips = std::collections::HashSet::new();
            let mut external_count = 0;
            let mut gateway_count = 0;
            for target in &self.targets {
                let address = target
                    .address
                    .parse::<SocketAddr>()
                    .map_err(|_| invalid_record())?;
                if !addresses.insert(address) {
                    return Err(invalid_record());
                }
                match target.role {
                    ProbeRole::External => {
                        external_count += 1;
                        if !external_ips.insert(address.ip()) {
                            return Err(invalid_record());
                        }
                    }
                    ProbeRole::Gateway => gateway_count += 1,
                }
            }
            if external_count < 2 || gateway_count > 1 {
                return Err(invalid_record());
            }
            Ok(())
        }
    }

    fn invalid_record() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, "invalid monitor state record")
    }

    fn default_state_path() -> io::Result<PathBuf> {
        let data_home = env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let home = env::var_os("HOME").map(PathBuf::from);
        let database = storage::resolve_database_path(data_home.as_deref(), home.as_deref())
            .map_err(io::Error::other)?;
        let directory = database
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid data path"))?;
        Ok(directory.join(STATE_FILE_NAME))
    }

    fn read_status(path: &Path) -> MonitorStatus {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return MonitorStatus::Unavailable
            }
            Ok(metadata) if metadata.len() > MAX_STATE_BYTES => return MonitorStatus::Unavailable,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return MonitorStatus::NoRecord
            }
            Err(_) => return MonitorStatus::Unavailable,
        }
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_CLOEXEC | nix::libc::O_NOFOLLOW)
            .open(path)
        {
            Ok(file) => file,
            Err(_) => return MonitorStatus::Unavailable,
        };
        let (bytes, running) = match Flock::lock(file, FlockArg::LockSharedNonblock) {
            Ok(mut lock) => (read_payload(&mut *lock), false),
            Err((file, error)) if error == Errno::EWOULDBLOCK || error == Errno::EAGAIN => {
                (read_payload(file), true)
            }
            Err(_) => return MonitorStatus::Unavailable,
        };
        let Some(bytes) = bytes else {
            return if running {
                MonitorStatus::Running(None)
            } else {
                MonitorStatus::Unavailable
            };
        };
        let record = serde_json::from_slice::<StateRecord>(&bytes)
            .ok()
            .filter(|record| record.version == STATE_VERSION)
            .map(|record| record.config)
            .filter(|config| config.validate().is_ok());
        match (running, record) {
            (true, record) => MonitorStatus::Running(record),
            (false, Some(config)) => MonitorStatus::Stopped(Some(config)),
            (false, None) => MonitorStatus::Unavailable,
        }
    }

    fn read_payload(reader: impl Read) -> Option<Vec<u8>> {
        let mut bytes = Vec::new();
        reader
            .take(MAX_STATE_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        (bytes.len() as u64 <= MAX_STATE_BYTES).then_some(bytes)
    }

    pub fn read_default_status() -> MonitorStatus {
        match default_state_path() {
            Ok(path) => read_status(&path),
            Err(_) => MonitorStatus::Unavailable,
        }
    }

    pub fn render_status(output: &mut String, status: &MonitorStatus) {
        match status {
            MonitorStatus::NoRecord => {
                output.push_str("Monitor process: not running (no active lock detected).\n");
                output.push_str("Monitor configuration: no local monitor record.\n");
            }
            MonitorStatus::Unavailable => {
                output.push_str("Monitor process: unknown (monitor state is unavailable).\n");
                output.push_str("Monitor configuration: unavailable.\n");
            }
            MonitorStatus::Running(config) => {
                output.push_str(
                    "Monitor process: running (foreground process holds the local monitor lock).\n",
                );
                render_config(output, config, "Current monitor configuration");
            }
            MonitorStatus::Stopped(config) => {
                output.push_str(
                    "Monitor process: not running (the previous monitor lock is released).\n",
                );
                render_config(output, config, "Last monitor configuration (may be stale)");
            }
        }
    }

    fn render_config(output: &mut String, config: &Option<StoredMonitorConfig>, title: &str) {
        use std::fmt::Write as _;
        let Some(config) = config else {
            output.push_str("Monitor configuration: unavailable or invalid.\n");
            return;
        };
        writeln!(output, "{title}:").unwrap();
        writeln!(output, "  Interval: {} ms", config.interval_ms).unwrap();
        writeln!(output, "  Probe timeout: {} ms", config.timeout_ms).unwrap();
        writeln!(
            output,
            "  Retention: {} days or {} events",
            config.retention_days, config.max_events
        )
        .unwrap();
        writeln!(
            output,
            "  DNS query: {} (name omitted)",
            if config.dns_name.is_some() {
                "configured"
            } else {
                "not configured"
            }
        )
        .unwrap();
        writeln!(
            output,
            "  Kernel journal: {}",
            if config.kernel_journal {
                "enabled"
            } else {
                "disabled"
            }
        )
        .unwrap();
        output.push_str("  Probe targets:\n");
        for target in &config.targets {
            writeln!(output, "    {:?}: {}", target.role, target.address).unwrap();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::probes::ProbeTarget;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NEXT: AtomicUsize = AtomicUsize::new(0);

        fn temp_path() -> (PathBuf, PathBuf) {
            let root = env::temp_dir().join(format!(
                "netforensics-state-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            (root.clone(), root.join(STATE_FILE_NAME))
        }

        fn config() -> MonitorConfig {
            let mut config = MonitorConfig::new(
                10_000,
                3_000,
                vec![
                    ProbeTarget {
                        role: ProbeRole::External,
                        address: "192.0.2.1:443".parse().unwrap(),
                    },
                    ProbeTarget {
                        role: ProbeRole::External,
                        address: "198.51.100.2:443".parse().unwrap(),
                    },
                ],
            )
            .unwrap();
            config = config.with_dns_name(Some(DnsName::parse("private.example.").unwrap()));
            config.with_kernel_journal(true)
        }

        #[test]
        fn reports_active_config_then_retained_last_config_after_shutdown() {
            use std::os::unix::fs::PermissionsExt;

            let (root, path) = temp_path();
            let stored = StoredMonitorConfig::from_config(&config());
            stored.validate().unwrap();
            let guard = MonitorGuard::acquire_at(&path, &stored).unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            match read_status(&path) {
                MonitorStatus::Running(Some(actual)) => assert_eq!(actual, stored),
                other => panic!("unexpected status: {other:?}"),
            }
            assert!(matches!(
                MonitorGuard::acquire_at(&path, &stored),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists
            ));
            drop(guard);
            match read_status(&path) {
                MonitorStatus::Stopped(Some(actual)) => assert_eq!(actual, stored),
                other => panic!("unexpected status: {other:?}"),
            }
            fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn rejects_malformed_and_symlink_state_without_following_it() {
            use std::os::unix::fs::symlink;
            let (root, path) = temp_path();
            fs::write(&path, b"{broken").unwrap();
            assert!(matches!(read_status(&path), MonitorStatus::Unavailable));
            let target = root.join("target");
            fs::write(&target, b"{}").unwrap();
            fs::remove_file(&path).unwrap();
            symlink(&target, &path).unwrap();
            assert!(matches!(read_status(&path), MonitorStatus::Unavailable));
            fs::remove_dir_all(root).unwrap();
        }

        #[test]
        fn validates_monitor_configuration_and_hides_dns_name_in_status() {
            let config = StoredMonitorConfig::from_config(&config());
            config.validate().unwrap();
            let mut output = String::new();
            render_status(&mut output, &MonitorStatus::Running(Some(config)));
            assert!(output.contains("DNS query: configured (name omitted)"));
            assert!(output.contains("External: 192.0.2.1:443"));
            assert!(!output.contains("private.example."));
        }
    }

    pub use MonitorGuard as Guard;
}

#[cfg(target_os = "linux")]
pub use linux::{read_default_status, render_status, Guard as MonitorGuard};

#[cfg(not(target_os = "linux"))]
pub struct MonitorGuard;

#[cfg(not(target_os = "linux"))]
impl MonitorGuard {
    pub fn acquire_default(_config: &MonitorConfig) -> io::Result<Self> {
        Ok(Self)
    }
}

#[cfg(not(target_os = "linux"))]
pub fn read_default_status() -> MonitorStatus {
    MonitorStatus::Unavailable
}

#[cfg(not(target_os = "linux"))]
pub fn render_status(output: &mut String, _status: &MonitorStatus) {
    output.push_str("Monitor process: unknown (active monitor status is supported on Linux).\n");
    output.push_str("Monitor configuration: unavailable on this platform.\n");
}
