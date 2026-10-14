//! Opt-in, privacy-minimized kernel journal sampling.

use crate::event::{Event, EventKind, KernelLinkSignalCategory};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const MAX_ENTRIES: usize = 100;
const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct JournalEntry {
    #[serde(rename = "__CURSOR")]
    cursor: Option<String>,
    #[serde(rename = "MESSAGE")]
    message: Option<String>,
    #[serde(rename = "__REALTIME_TIMESTAMP")]
    realtime_timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelSignal {
    pub category: KernelLinkSignalCategory,
    pub interface: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalBatch {
    pub events: Vec<Event>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalError {
    Unavailable,
    TimedOut,
    OutputLimit,
    InvalidOutput,
}

pub trait KernelJournalReader {
    fn read_since(&mut self, cursor: Option<&str>) -> Result<Vec<u8>, JournalError>;
}

#[derive(Debug, Default)]
pub struct SystemKernelJournalReader;

impl KernelJournalReader for SystemKernelJournalReader {
    fn read_since(&mut self, cursor: Option<&str>) -> Result<Vec<u8>, JournalError> {
        let mut command = Command::new("journalctl");
        command
            .args([
                "--kernel",
                "--output=json",
                "--no-pager",
                "--quiet",
                "--lines=100",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(cursor) = cursor {
            command.arg(format!("--after-cursor={cursor}"));
        } else {
            command.arg("--since=now");
        }

        let mut child = command.spawn().map_err(|_| JournalError::Unavailable)?;
        let mut stdout = child.stdout.take().ok_or(JournalError::Unavailable)?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let reader = thread::spawn(move || {
            let mut output = Vec::new();
            let result = stdout
                .by_ref()
                .take(MAX_OUTPUT_BYTES + 1)
                .read_to_end(&mut output)
                .map(|_| output);
            let _ = sender.send(result);
        });

        let deadline = Instant::now() + COMMAND_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status.success()),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Some(false);
                }
            }
        };
        let output = receiver.recv().map_err(|_| JournalError::Unavailable)?;
        let _ = reader.join();
        let output = output.map_err(|_| JournalError::Unavailable)?;
        if status.is_none() {
            return Err(JournalError::TimedOut);
        }
        if output.len() as u64 > MAX_OUTPUT_BYTES {
            return Err(JournalError::OutputLimit);
        }
        if !status.unwrap_or(false) {
            return Err(JournalError::Unavailable);
        }
        Ok(output)
    }
}

#[derive(Debug)]
pub struct KernelJournalCollector<R> {
    reader: R,
    cursor: Option<String>,
}

impl<R: KernelJournalReader> KernelJournalCollector<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            cursor: None,
        }
    }

    pub fn collect<F>(
        &mut self,
        recorded_at_utc: DateTime<Utc>,
        mut next_id: F,
    ) -> Result<Vec<Event>, JournalError>
    where
        F: FnMut() -> String,
    {
        let output = self.reader.read_since(self.cursor.as_deref())?;
        let batch = parse_output(&output, recorded_at_utc, &mut next_id)?;
        if batch.cursor.is_some() {
            self.cursor = batch.cursor;
        }
        Ok(batch.events)
    }
}

pub fn parse_output<F>(
    output: &[u8],
    recorded_at_utc: DateTime<Utc>,
    next_id: &mut F,
) -> Result<JournalBatch, JournalError>
where
    F: FnMut() -> String,
{
    if output.len() as u64 > MAX_OUTPUT_BYTES {
        return Err(JournalError::OutputLimit);
    }
    let mut events = Vec::new();
    let mut cursor = None;
    let mut entry_count = 0;
    for line in output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        entry_count += 1;
        if entry_count > MAX_ENTRIES {
            return Err(JournalError::OutputLimit);
        }
        let entry: JournalEntry =
            serde_json::from_slice(line).map_err(|_| JournalError::InvalidOutput)?;
        if let Some(value) = entry
            .cursor
            .filter(|value| !value.is_empty() && value.len() <= 1024)
        {
            cursor = Some(value);
        }
        if let Some(message) = entry.message.as_deref() {
            if let Some(signal) = parse_message(message) {
                events.push(Event {
                    schema_version: crate::event::SCHEMA_VERSION,
                    id: next_id(),
                    recorded_at_utc: entry
                        .realtime_timestamp
                        .as_deref()
                        .and_then(parse_realtime_timestamp)
                        .unwrap_or(recorded_at_utc),
                    source: "kernel_journal".to_owned(),
                    kind: EventKind::KernelLinkSignal {
                        category: signal.category,
                        interface: signal.interface,
                    },
                });
            }
        }
    }
    Ok(JournalBatch { events, cursor })
}

fn parse_realtime_timestamp(value: &str) -> Option<DateTime<Utc>> {
    let micros = value.parse::<u64>().ok()?;
    DateTime::from_timestamp(
        i64::try_from(micros / 1_000_000).ok()?,
        u32::try_from((micros % 1_000_000) * 1_000).ok()?,
    )
}

fn parse_message(message: &str) -> Option<KernelSignal> {
    let lower = message.to_ascii_lowercase();
    let category = if lower.contains("netdev watchdog") {
        KernelLinkSignalCategory::NetdevWatchdog
    } else if lower.contains("link is down") || lower.contains("nic link is down") {
        KernelLinkSignalCategory::LinkDown
    } else if lower.contains("link is up") || lower.contains("link becomes ready") {
        KernelLinkSignalCategory::LinkUp
    } else {
        return None;
    };

    let marker = [
        "netdev watchdog",
        "link is down",
        "link is up",
        "link becomes ready",
    ]
    .iter()
    .filter_map(|marker| lower.find(marker))
    .min();
    let interface = marker
        .and_then(|index| message.get(..index))
        .and_then(|prefix| {
            let mut names = prefix.split_whitespace().rev();
            let last = names.next()?;
            if last.eq_ignore_ascii_case("nic") {
                names.next()
            } else {
                Some(last)
            }
        })
        .map(|name| name.trim_end_matches(':'))
        .filter(|name| valid_interface_name(name))
        .map(str::to_owned);
    Some(KernelSignal {
        category,
        interface,
    })
}

fn valid_interface_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 15
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn sample_time() -> DateTime<Utc> {
        "2026-10-13T00:00:00Z".parse().unwrap()
    }

    #[derive(Clone)]
    struct FakeReader {
        batches: Rc<RefCell<Vec<Result<Vec<u8>, JournalError>>>>,
        cursors: Rc<RefCell<Vec<Option<String>>>>,
    }

    impl KernelJournalReader for FakeReader {
        fn read_since(&mut self, cursor: Option<&str>) -> Result<Vec<u8>, JournalError> {
            self.cursors.borrow_mut().push(cursor.map(str::to_owned));
            self.batches.borrow_mut().remove(0)
        }
    }

    #[test]
    fn parses_only_normalized_link_signals_and_discards_raw_text() {
        let output = br#"{"__CURSOR":"cursor-a","__REALTIME_TIMESTAMP":"1791849600000000","MESSAGE":"eno1: NIC Link is Down"}
{"__CURSOR":"cursor-b","MESSAGE":"NETDEV WATCHDOG: transmit queue timed out"}
{"__CURSOR":"cursor-c","MESSAGE":"unrelated kernel detail with secret"}
"#;
        let batch = parse_output(output, "2026-10-13T00:00:00Z".parse().unwrap(), &mut || {
            "id".into()
        })
        .unwrap();
        assert_eq!(batch.cursor.as_deref(), Some("cursor-c"));
        assert_eq!(batch.events.len(), 2);
        assert_eq!(
            batch.events[0].recorded_at_utc.to_rfc3339(),
            "2026-10-13T00:00:00+00:00"
        );
        assert!(
            matches!(batch.events[0].kind, EventKind::KernelLinkSignal { category: KernelLinkSignalCategory::LinkDown, interface: Some(ref name) } if name == "eno1")
        );
        assert!(matches!(
            batch.events[1].kind,
            EventKind::KernelLinkSignal {
                category: KernelLinkSignalCategory::NetdevWatchdog,
                interface: None
            }
        ));
        let json = batch.events[0].to_json().unwrap();
        assert!(!json.contains("NIC Link"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn ignores_invalid_interface_names_and_unrecognized_or_missing_messages() {
        assert!(parse_message("bad/name: Link is Down")
            .unwrap()
            .interface
            .is_none());
        assert!(parse_message("driver detail").is_none());
        assert_eq!(
            parse_message("e1000e 0000:03:00.0 eno1: NIC Link is Down")
                .unwrap()
                .interface
                .as_deref(),
            Some("eno1")
        );
        let result =
            parse_output(br#"{"__CURSOR":"x"}"#, sample_time(), &mut || "id".into()).unwrap();
        assert!(result.events.is_empty());
        assert_eq!(result.cursor.as_deref(), Some("x"));
    }

    #[test]
    fn rejects_malformed_entries_and_enforces_output_limit() {
        assert_eq!(
            parse_output(b"not-json", sample_time(), &mut || "id".into()).unwrap_err(),
            JournalError::InvalidOutput
        );
        assert_eq!(
            parse_output(
                &vec![b'x'; MAX_OUTPUT_BYTES as usize + 1],
                sample_time(),
                &mut || "id".into()
            )
            .unwrap_err(),
            JournalError::OutputLimit
        );
    }

    #[test]
    fn collector_keeps_cursor_in_memory_and_propagates_unavailability() {
        let cursors = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            batches: Rc::new(RefCell::new(vec![
                Ok(br#"{"__CURSOR":"only-in-memory","MESSAGE":"eth0: Link is Up"}"#.to_vec()),
                Err(JournalError::Unavailable),
            ])),
            cursors: Rc::clone(&cursors),
        };
        let mut collector = KernelJournalCollector::new(reader);
        assert_eq!(
            collector
                .collect(sample_time(), || "1".into())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            collector.collect(sample_time(), || "2".into()).unwrap_err(),
            JournalError::Unavailable
        );
        assert_eq!(
            cursors.borrow().as_slice(),
            &[None, Some("only-in-memory".to_owned())]
        );
    }
}
