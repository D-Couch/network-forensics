use chrono::{DateTime, Utc};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::net::SocketAddr;

pub const SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventKind {
    LinkStateChanged {
        interface: String,
        /// `None` means the carrier state was unavailable; `Some(false)` means carrier was down.
        carrier: Option<bool>,
    },
    ProbeResult {
        role: ProbeRole,
        target: SocketAddr,
        method: ProbeMethod,
        started_at_utc: DateTime<Utc>,
        elapsed_ms: u64,
        outcome: ProbeOutcome,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeRole {
    Gateway,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeMethod {
    TcpConnect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "category", rename_all = "snake_case")]
pub enum ProbeOutcome {
    Success,
    Timeout,
    Failure(ProbeFailureCategory),
    UnsupportedOrBlocked(ProbeLimitationCategory),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeFailureCategory {
    ConnectionRefused,
    NetworkUnreachable,
    HostUnreachable,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeLimitationCategory {
    PermissionDenied,
    MethodUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub schema_version: u16,
    /// Assigned by the caller; must be unique within the caller's event collection.
    pub id: String,
    pub recorded_at_utc: DateTime<Utc>,
    pub source: String,
    pub kind: EventKind,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventWire {
    schema_version: u16,
    id: String,
    recorded_at_utc: DateTime<Utc>,
    source: String,
    kind: EventKind,
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EventWire::deserialize(deserializer)?;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(de::Error::custom(format!(
                "unsupported schema_version {}; expected {}",
                wire.schema_version, SCHEMA_VERSION
            )));
        }
        Ok(Self {
            schema_version: wire.schema_version,
            id: wire.id,
            recorded_at_utc: wire.recorded_at_utc,
            source: wire.source,
            kind: wire.kind,
        })
    }
}

impl Event {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(carrier: Option<bool>) -> Event {
        Event {
            schema_version: SCHEMA_VERSION,
            id: "event-1".into(),
            recorded_at_utc: "2026-01-01T00:00:00Z".parse().unwrap(),
            source: "interface".into(),
            kind: EventKind::LinkStateChanged {
                interface: "eno1".into(),
                carrier,
            },
        }
    }

    #[test]
    fn round_trips_stable_json_shape() {
        let original = event(Some(false));
        let json = original.to_json().unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"type\": \"link_state_changed\""));
        assert_eq!(Event::from_json(&json).unwrap(), original);
    }

    #[test]
    fn rejects_incompatible_schema_version() {
        let json = event(Some(true))
            .to_json()
            .unwrap()
            .replace("\"schema_version\": 1", "\"schema_version\": 2");
        assert!(Event::from_json(&json)
            .unwrap_err()
            .to_string()
            .contains("unsupported schema_version 2"));
    }

    #[test]
    fn unavailable_carrier_is_distinct_from_down() {
        let down: serde_json::Value =
            serde_json::from_str(&event(Some(false)).to_json().unwrap()).unwrap();
        let unknown: serde_json::Value =
            serde_json::from_str(&event(None).to_json().unwrap()).unwrap();
        assert_eq!(down["kind"]["carrier"], false);
        assert!(unknown["kind"]["carrier"].is_null());
        assert_ne!(down, unknown);
    }

    #[test]
    fn rejects_unknown_fields() {
        let json = event(Some(true)).to_json().unwrap().replace(
            "\"source\": \"interface\"",
            "\"source\": \"interface\", \"extra\": true",
        );
        assert!(Event::from_json(&json).is_err());
    }

    #[test]
    fn round_trips_typed_probe_result() {
        let event = Event {
            schema_version: SCHEMA_VERSION,
            id: "probe-1".into(),
            recorded_at_utc: "2026-01-01T00:00:00Z".parse().unwrap(),
            source: "probe".into(),
            kind: EventKind::ProbeResult {
                role: ProbeRole::External,
                target: "203.0.113.8:443".parse().unwrap(),
                method: ProbeMethod::TcpConnect,
                started_at_utc: "2026-01-01T00:00:00Z".parse().unwrap(),
                elapsed_ms: 17,
                outcome: ProbeOutcome::Failure(ProbeFailureCategory::ConnectionRefused),
            },
        };

        let json = event.to_json().unwrap();
        assert!(json.contains("\"type\": \"probe_result\""));
        assert!(json.contains("\"role\": \"external\""));
        assert!(json.contains("\"target\": \"203.0.113.8:443\""));
        assert_eq!(Event::from_json(&json).unwrap(), event);
    }
}
