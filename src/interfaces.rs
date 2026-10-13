//! Passive Linux interface and default-route discovery from sysfs and procfs.

use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationalState {
    Up,
    Down,
    Unknown,
    Dormant,
    NotPresent,
    LowerLayerDown,
    Other(String),
}

impl OperationalState {
    fn parse(value: &str) -> Self {
        match value.trim() {
            "up" => Self::Up,
            "down" => Self::Down,
            "unknown" => Self::Unknown,
            "dormant" => Self::Dormant,
            "notpresent" => Self::NotPresent,
            "lowerlayerdown" => Self::LowerLayerDown,
            other => Self::Other(other.to_owned()),
        }
    }

    fn display(&self) -> &str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Unknown => "unknown",
            Self::Dormant => "dormant",
            Self::NotPresent => "not present",
            Self::LowerLayerDown => "lower layer down",
            Self::Other(value) => value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceState {
    pub name: String,
    pub operational_state: OperationalState,
    /// `None` means sysfs could not provide a carrier value.
    pub carrier: Option<bool>,
    /// Link speed in Mbps, when the driver reports a positive value.
    pub speed_mbps: Option<u64>,
    pub is_default_route: bool,
    pub counters: InterfaceCounters,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InterfaceCounters {
    pub rx_errors: Option<u64>,
    pub tx_errors: Option<u64>,
    pub rx_dropped: Option<u64>,
    pub tx_dropped: Option<u64>,
}

impl InterfaceCounters {
    pub fn sample_available(self) -> bool {
        self.rx_errors.is_some()
            || self.tx_errors.is_some()
            || self.rx_dropped.is_some()
            || self.tx_dropped.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    pub interfaces: Vec<InterfaceState>,
    /// `None` means no default route was identified or procfs was unavailable.
    pub default_route: Option<String>,
}

/// Discovers all interfaces visible under `/sys/class/net` and the active
/// default route listed in `/proc/net/route`.
pub fn discover() -> io::Result<Discovery> {
    discover_at(Path::new("/sys/class/net"), Path::new("/proc/net/route"))
}

fn discover_at(sys_class_net: &Path, route_file: &Path) -> io::Result<Discovery> {
    let default_route = fs::read_to_string(route_file)
        .ok()
        .and_then(|contents| parse_default_route(&contents));
    let mut interfaces = Vec::new();

    for entry in fs::read_dir(sys_class_net)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let operational_state = read_trimmed(&path.join("operstate"))
            .map(|value| OperationalState::parse(&value))
            .unwrap_or(OperationalState::Unknown);
        let carrier = read_trimmed(&path.join("carrier")).and_then(|value| match value.as_str() {
            "0" => Some(false),
            "1" => Some(true),
            _ => None,
        });
        let speed_mbps = read_trimmed(&path.join("speed"))
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|speed| *speed > 0);
        let counters = InterfaceCounters {
            rx_errors: read_counter(&path.join("statistics/rx_errors")),
            tx_errors: read_counter(&path.join("statistics/tx_errors")),
            rx_dropped: read_counter(&path.join("statistics/rx_dropped")),
            tx_dropped: read_counter(&path.join("statistics/tx_dropped")),
        };

        interfaces.push(InterfaceState {
            is_default_route: default_route.as_deref() == Some(name.as_str()),
            name,
            operational_state,
            carrier,
            speed_mbps,
            counters,
        });
    }

    interfaces.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(Discovery {
        interfaces,
        default_route,
    })
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
}

fn read_counter(path: &Path) -> Option<u64> {
    read_trimmed(path).and_then(|value| value.parse::<u64>().ok())
}

fn parse_default_route(contents: &str) -> Option<String> {
    contents
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let interface = fields.next()?;
            let destination = fields.next()?;
            let _gateway = fields.next()?;
            let flags = u32::from_str_radix(fields.next()?, 16).ok()?;
            let _ref_count = fields.next()?;
            let _use_count = fields.next()?;
            let metric = fields.next()?.parse::<u32>().ok()?;

            (destination == "00000000" && flags & 1 != 0).then_some((metric, interface))
        })
        .min_by(|(left_metric, left_iface), (right_metric, right_iface)| {
            left_metric
                .cmp(right_metric)
                .then_with(|| left_iface.cmp(right_iface))
        })
        .map(|(_, interface)| interface.to_owned())
}

impl std::fmt::Display for OperationalState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "netforensics-test-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn picks_lowest_metric_active_default_route_deterministically() {
        let routes = "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\n\
            wlan0 00000000 0100000A 0003 0 0 50 00000000 0 0 0\n\
            eth0 00000000 0100000A 0001 0 0 10 00000000 0 0 0\n\
            eth1 00000000 0100000A 0001 0 0 10 00000000 0 0 0\n\
            inactive 00000000 0100000A 0000 0 0 1 00000000 0 0 0\n";
        assert_eq!(parse_default_route(routes).as_deref(), Some("eth0"));
    }

    #[test]
    fn malformed_or_missing_default_route_is_unavailable() {
        assert_eq!(
            parse_default_route("Iface Destination Gateway Flags\n"),
            None
        );
        assert_eq!(
            parse_default_route("Iface Destination Gateway Flags\neth0 broken"),
            None
        );
    }

    #[test]
    fn discovery_preserves_unavailable_values_and_orders_interfaces() {
        let root = temp_dir();
        let net = root.join("net");
        fs::create_dir(&net).unwrap();
        let zeta = net.join("zeta");
        fs::create_dir(&zeta).unwrap();
        fs::write(zeta.join("operstate"), "up\n").unwrap();
        fs::write(zeta.join("carrier"), "1\n").unwrap();
        fs::write(zeta.join("speed"), "1000\n").unwrap();
        let stats = zeta.join("statistics");
        fs::create_dir(&stats).unwrap();
        fs::write(stats.join("rx_errors"), "3\n").unwrap();
        fs::write(stats.join("tx_dropped"), "bad\n").unwrap();
        let alpha = net.join("alpha");
        fs::create_dir(&alpha).unwrap();
        fs::write(alpha.join("operstate"), "unknown\n").unwrap();

        let route = root.join("route");
        fs::write(
            &route,
            "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\n\
             zeta 00000000 0100000A 0003 0 0 5 00000000 0 0 0\n",
        )
        .unwrap();

        let discovery = discover_at(&net, &route).unwrap();
        let states = discovery.interfaces;
        assert_eq!(discovery.default_route.as_deref(), Some("zeta"));
        assert_eq!(states[0].name, "alpha");
        assert_eq!(states[0].operational_state, OperationalState::Unknown);
        assert_eq!(states[0].carrier, None);
        assert_eq!(states[0].speed_mbps, None);
        assert!(!states[0].is_default_route);
        assert_eq!(states[1].carrier, Some(true));
        assert_eq!(states[1].speed_mbps, Some(1000));
        assert_eq!(states[1].counters.rx_errors, Some(3));
        assert_eq!(states[1].counters.tx_errors, None);
        assert_eq!(states[1].counters.tx_dropped, None);
        assert!(states[1].is_default_route);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zero_or_negative_speed_is_unavailable() {
        assert_eq!("0".parse::<u64>().ok().filter(|speed| *speed > 0), None);
        assert_eq!("-1".parse::<u64>().ok().filter(|speed| *speed > 0), None);
    }

    #[test]
    fn counter_parser_preserves_missing_invalid_and_zero_values() {
        let root = temp_dir();
        let path = root.join("counter");
        fs::write(&path, "0\n").unwrap();
        assert_eq!(read_counter(&path), Some(0));
        fs::write(&path, "-1\n").unwrap();
        assert_eq!(read_counter(&path), None);
        assert_eq!(read_counter(&root.join("missing")), None);
        fs::remove_dir_all(root).unwrap();
    }
}
