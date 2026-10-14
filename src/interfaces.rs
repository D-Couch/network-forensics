//! Passive Linux interface and default-route discovery from sysfs and procfs.

use std::fs;
use std::io;
use std::path::Path;

const RTF_UP: u32 = 0x0001;
const RTF_REJECT: u32 = 0x0200;

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

/// Discovers all interfaces visible under `/sys/class/net`, preferring an
/// active IPv4 default route and falling back to IPv6 when needed.
pub fn discover() -> io::Result<Discovery> {
    discover_at(
        Path::new("/sys/class/net"),
        Path::new("/proc/net/route"),
        Path::new("/proc/net/ipv6_route"),
    )
}

fn discover_at(
    sys_class_net: &Path,
    route_file: &Path,
    route6_file: &Path,
) -> io::Result<Discovery> {
    let ipv4_default = fs::read_to_string(route_file)
        .ok()
        .and_then(|contents| parse_default_route(&contents));
    let default_route = ipv4_default.or_else(|| {
        fs::read_to_string(route6_file)
            .ok()
            .and_then(|contents| parse_ipv6_default_route(&contents))
    });
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
            let mask = fields.next()?;

            (destination == "00000000"
                && mask == "00000000"
                && flags & RTF_UP != 0
                && flags & RTF_REJECT == 0)
                .then_some((metric, interface))
        })
        .min_by(|(left_metric, left_iface), (right_metric, right_iface)| {
            left_metric
                .cmp(right_metric)
                .then_with(|| left_iface.cmp(right_iface))
        })
        .map(|(_, interface)| interface.to_owned())
}

fn parse_ipv6_default_route(contents: &str) -> Option<String> {
    contents
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let destination = fields.next()?;
            let destination_prefix = fields.next()?;
            let source = fields.next()?;
            let source_prefix = fields.next()?;
            let _next_hop = fields.next()?;
            let metric = u32::from_str_radix(fields.next()?, 16).ok()?;
            let _ref_count = fields.next()?;
            let _use_count = fields.next()?;
            let flags = u32::from_str_radix(fields.next()?, 16).ok()?;
            let interface = fields.next()?;

            (destination == "00000000000000000000000000000000"
                && destination_prefix == "00"
                && source == "00000000000000000000000000000000"
                && source_prefix == "00"
                && flags & RTF_UP != 0
                && flags & RTF_REJECT == 0)
                .then_some((metric, interface))
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
    fn ignores_reject_default_routes() {
        let routes = [
            "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT",
            "reject 00000000 00000000 0201 0 0 1 00000000 0 0 0",
        ]
        .join("\n");
        assert_eq!(parse_default_route(&routes), None);
    }

    #[test]
    fn selects_lowest_metric_active_ipv6_default_deterministically() {
        let routes = [
            "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000014 00000001 00000000 00000001 eth1",
            "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000005 00000001 00000000 00000001 eth0",
            "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000001 00000001 00000000 00000001 eth0",
        ]
        .join("\n");
        assert_eq!(parse_ipv6_default_route(&routes).as_deref(), Some("eth0"));
    }

    #[test]
    fn ignores_nondefault_source_specific_down_and_reject_ipv6_routes() {
        let zero = "00000000000000000000000000000000";
        let routes = [
            format!("{zero} 01 {zero} 00 {zero} 00000001 00000001 00000000 00000001 more-specific"),
            format!(
                "{zero} 00 {zero} 40 {zero} 00000001 00000001 00000000 00000001 source-specific"
            ),
            format!("{zero} 00 {zero} 00 {zero} 00000001 00000001 00000000 00000000 down"),
            format!("{zero} 00 {zero} 00 {zero} 00000001 00000001 00000000 00000201 reject"),
        ]
        .join("\n");
        assert_eq!(parse_ipv6_default_route(&routes), None);
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
    fn ignores_zero_destination_routes_with_nonzero_masks() {
        let routes = [
            "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT",
            "split-default 00000000 0100000A 0001 0 0 1 80000000 0 0 0",
            "default 00000000 0100000A 0001 0 0 20 00000000 0 0 0",
        ]
        .join("\n");
        assert_eq!(parse_default_route(&routes).as_deref(), Some("default"));

        let only_split_default = [
            "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT",
            "split-default 00000000 0100000A 0001 0 0 1 80000000 0 0 0",
        ]
        .join("\n");
        assert_eq!(parse_default_route(&only_split_default), None);
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

        let route6 = root.join("route6");
        let discovery = discover_at(&net, &route, &route6).unwrap();
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

        fs::write(
            &route6,
            "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000001 00000001 00000000 00000001 alpha\n",
        )
        .unwrap();
        let discovery = discover_at(&net, &route, &route6).unwrap();
        assert_eq!(discovery.default_route.as_deref(), Some("zeta"));

        fs::write(
            &route,
            [
                "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT",
                "reject 00000000 00000000 0201 0 0 1 00000000 0 0 0",
            ]
            .join("\n"),
        )
        .unwrap();
        fs::write(
            &route6,
            "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000005 00000001 00000000 00000001 zeta\n",
        )
        .unwrap();
        let discovery = discover_at(&net, &route, &route6).unwrap();
        assert_eq!(discovery.default_route.as_deref(), Some("zeta"));
        assert!(discovery.interfaces[1].is_default_route);

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
