# Technical Architecture

**Status:** Describes the implemented M1 CLI, Tasks 0005–0014, and intended future architecture. Implemented behavior is labeled below; other sections describe targets, not existing components.

## 1. Logical flow

```text
Current:
[Linux interface observer] --\
                              +--> [cycle events] --> [SQLite] --> [JSON Lines stdout]
[Explicit TCP and opt-in DNS probe runners] -/

Target:
[Observation events] --> [local event store] --> [incident detector]
                                                   --> [diagnostic rules]
                                                   --> [report/export CLI]
[Versioned replay fixtures] ----------------------> [same analysis pipeline]
```

The collectors emit *observations*, not conclusions. The incident detector correlates events and produces incident boundaries; the separate rule engine evaluates possible fault domains. The storage layer should not dictate diagnostic rules.

## 2. Modules (logical, not mandatory crates)

- `main.rs`: current command parsing and user-facing output, including read-only `status` and interactive target `setup`; setup prints a command but does not start monitoring.
- `event`: typed, versioned observation events (implemented).
- `interfaces`: Linux interface state and default-route discovery (implemented).
- `probes` and `dns`: explicit TCP checks and opt-in system-configured DNS lookups (implemented).
- `monitor`: sequential round scheduling and cancellation (implemented); see the roadmap for planned reliability work.
- `storage`: SQLite persistence and migration (implemented in M2).
- `incidents`: deterministic probe-target lifecycle over stored events (Task 0006) and read-only history CLI (Task 0007); see the roadmap for unimplemented capability status.
- `analysis`: pure evidence-citing rules over stored event types (Tasks 0008/0010/0014), exposed by read-only `analyze --latest`.
- `report`: pure versioned text/JSON reports, redacted by default (Task 0011); see the roadmap for unimplemented capability status.
- `replay`: strict bounded JSON fixture replay through incident and analysis APIs (Task 0009), exposed through `replay <FIXTURE.json>` (Task 0019).

### Current read-only history commands

`analyze --latest` reads retained events by insertion sequence, derives incidents,
and renders their findings. `report --latest` performs the same analysis and
renders incident timelines plus findings as text by default or versioned JSON
with `--format json`. Both commands open the existing database read-only and
perform no network activity. Reports map each distinct target consistently to
a role-scoped label; `--include-identifiers` includes raw target addresses and
prints a sharing warning to stderr. Report bodies contain event IDs and
timestamps for evidence review but omit DNS query names and raw event payloads.

`status` pages the existing database in insertion order and summarizes its
latest event, probe, interface snapshot/counter sample, and optional DNS and
normalized kernel-link events. Missing optional events are described as
absent from retained history; the command does not infer whether collection was
disabled or unavailable. It performs no network, process, or write operations.

**Implementation rule:** Keep one Cargo package and Rust modules. Extract crates only if justified by dependency separation or actual reuse. Do not scaffold unused modules.

### Guided target setup

`setup` prompts for two external numeric IP:port endpoints on distinct IP
addresses and an optional gateway-role endpoint. It validates the same target
syntax used by monitoring, explains that external endpoints should be expected to accept TCP,
and states that the gateway role is only a caller-supplied label. It prints a
shell-quoted monitor command for review but performs no probes, database access,
or system/network discovery. Monitoring begins only when the user runs that
command.

### Current interface discovery

The Linux interface inventory reads operational state, carrier, reported link
speed, and available RX/TX error/drop counters from `/sys/class/net`. It selects
the active IPv4 `0.0.0.0/0` route with the lowest metric from
`/proc/net/route`, falling back to an active IPv6 `::/0` route from
`/proc/net/ipv6_route` when no usable IPv4 default exists. IPv6 routes must be
unrestricted by source prefix; down and reject routes are ignored in both
tables. When both families have usable defaults on different interfaces, IPv4
is preferred. The discovery does no active probes and requires no elevated
privileges. Missing or unsupported sysfs values remain unknown; the CLI reports
an unavailable default route when it cannot identify one.

Foreground monitoring compares each interface with its preceding in-memory
sample. It emits carrier and speed transitions plus per-round counter deltas;
cumulative counter values are not persisted. Missing values remain unknown,
and a decrease marks a possible counter reset while leaving that delta unknown.
These reads are passive and require no elevated privileges.

### Current probe runner

The one-shot CLI probe accepts a role and numeric IP:port explicitly, then makes
one TCP connection attempt with a 3000 ms default timeout capped at 30000 ms.
It does not perform DNS resolution, retry, or select public targets. The runner
records a UTC start timestamp and monotonic elapsed milliseconds in a typed
`probe_result` event. Transport and clock traits allow deterministic tests
without network access. TCP success or failure applies only to the configured
endpoint and port; it is not a general Internet reachability conclusion. The
gateway/external role is caller-provided metadata, not an independently
verified network location.

Foreground monitoring optionally accepts one ASCII FQDN ending in a dot via
`--dns-name`. The DNS collector runs after configured TCP probes, only when
explicitly enabled, and uses Hickory's system Tokio resolver builder (Unix
`/etc/resolv.conf`, platform system configuration where supported). It bypasses
hosts-file lookup, uses one resolver attempt and the configured timeout, and
issues A/AAAA lookups. It stores the query name and answer count but not address
answers. Hickory does not implement every host NSS/resolver integration; see
[ADR 0004](decisions/0004-system-configured-dns-observation.md).

### Current foreground monitor

The monitor requires two distinct, explicit external IP:port targets and
accepts one optional gateway target. It emits JSON Lines interface snapshots,
probe events, and optional normalized kernel link signals to stdout, then commits each complete event cycle to the local
SQLite store before printing it. Targets run sequentially; a
completed round is followed by a configurable pause (10 seconds by default,
bounded to 1 second–1 hour). Ctrl+C prevents starting additional probes and
allows the current bounded connect attempt to finish. The `ctrlc` dependency
provides portable signal handling; the monitor creates no background service.

## 3. Event envelope v4 (reads v1 through v3)

New events use schema v4; deserialization continues to accept schema-v1 through
schema-v3 events. The envelope uses a serde-compatible Rust structure with
fields equivalent to:

```json
{
  "schema_version": 4,
  "id": "event-1",
  "recorded_at_utc": "2026-01-01T00:00:00Z",
  "source": "interface",
  "kind": {
    "type": "link_state_changed",
    "interface": "eno1",
    "carrier": false
  }
}
```

This JSON illustrates the `link_state_changed` shape. The event enum also
includes `probe_result`, `interface_snapshot`, `dns_result`, `link_speed_changed`,
`interface_counter_delta`, and `kernel_link_signal`; these variants are implemented and tested. DNS
events persist a configured query name and answer count, never returned
addresses. IDs are caller-assigned and must be unique within the
caller's event collection. Deserialization rejects unsupported schema versions
and unknown fields.

The `kernel_link_signal` variant stores the journal entry's realtime timestamp
when present, plus only normalized `link_up`, `link_down`,
or `netdev_watchdog` categories and an optional validated interface name. Its
collector is opt-in, reads kernel entries through `journalctl --output=json`,
and keeps the journal cursor in memory. Raw messages and cursors are not stored.
Missing `journalctl` or journal permission is reported as reduced observability;
monitoring continues. A separate `monitoring_gap` variant is not implemented; see the roadmap for planning status.

- Every event identifies provenance/source and observation time.
- IDs allow reports to cite specific evidence. Stable unique IDs may be generated by the ingestion layer, not each collector.
- Source-specific structured fields are preferred to parsing prose in the rules engine.
- **Unknown is distinct from zero, false, and failure.** Optionals and explicit enums should preserve uncertainty.
- Schema version is required; unknown future versions must be rejected clearly or handled by explicit migration.
- Wall clock is for presentation/correlation with logs; monotonic time should govern elapsed intervals and debounce in live sessions. Replay must define event order deterministically even if timestamps tie or jump.

## 4. Probes and classification constraints

- Distinct targets and probe methods can fail independently; no single probe determines “Internet up/down.”
- Gateways may not reply to ICMP or accept TCP ports; probeability needs explicit state, and gateway reachability should never be inferred solely from a failed arbitrary TCP port.
- Probe result should separate network errors, timeouts, active rejections, lookup problems, and unsupported/blocked methods.
- Record configuration changes and network route/interface changes that affect interpretation.
- Conservative analysis uses correlated evidence and provides alternatives. Do not calculate a numerical confidence score without calibration and evidence.

## 5. Incidents and temporal rules

Task 0006 implements `incidents::analyze_events`, a pure Rust API over stored events. It groups by exact `(probe role, numeric IP:port)` and opens an incident after two consecutive timeouts, network-unreachable, or host-unreachable outcomes within 30 seconds. Two consecutive successful TCP connects recover it. Refused, ambiguous, unsupported, or blocked outcomes break streaks and do not qualify. Counts, failure window, and maximum observation gap are configurable with validation; defaults are two failures, two successes, 30 seconds, and 60 seconds.

Stored insertion sequence defines order. A forward timestamp gap beyond the configured maximum or a backward timestamp resets pending streaks and marks an already-open incident with the corresponding uncertainty flag. Neither is evidence of continuous downtime. Incident IDs derive from the first stored sequence, and results retain failure/recovery event IDs. The `incidents` CLI reads event pages in sequence order from the existing database and passes the full retained history through the pure engine before applying optional `--since`, `--until`, and `--target` display filters. Its `--compare` view shows same-target probe outcomes before an incident and after recovery when those observations are retained; it does not infer a cause. The command opens the database read-only and performs no network activity. Derived records remain unpersisted and are recomputed for each query.

### Conservative analysis API

Tasks 0008, 0010, and 0014 add `analysis::analyze_incidents`, a pure function over ordered
stored events and lifecycle incidents. It returns one finding per incident with
a classification, explanation, evidence IDs, limitations, and follow-up check.
Its validated correlation window defaults to 60 seconds. Carrier-down,
normalized kernel link-down, and watchdog evidence may support local-link-related
findings without proving cause or claiming that the interface carried the probe
path. Journal link-up and unrelated kernel messages are neutral. Caller-provided
gateway roles remain labels. Multiple external
target failures do not prove a shared cause; endpoint-specific evidence requires
a distinct external target to succeed without a qualifying failure in the same
window. A backward timestamp anywhere in stored sequence disables timestamp
correlations. A failed DNS query with independent external TCP success can
support only concurrent DNS-query failure evidence; it does not prove DNS
caused an incident. The API adds no CLI, I/O, or diagnosis certainty.

### Versioned fixture replay

Task 0009's `replay::replay_json` accepts a strict envelope with
`fixture_schema_version: 1` and an `events` array. Each item has a stored
`sequence` and an event-v1 object. Sequence values must be strictly increasing;
the replay parser does not sort or repair them. Fixture versioning is
independent from event schema versioning. Parsing is bounded to 16 MiB and
100000 events, rejects duplicate event IDs and unknown fields, then invokes the
same incident and analysis APIs used elsewhere. The function accepts an
in-memory string and performs no file, database, network, or privileged I/O.

The `replay <FIXTURE.json>` CLI reads one supplied local file with a bounded
16 MiB read, passes its UTF-8 contents to this API, and prints derived incidents
and findings. It does not access the event database, network, or privileged
interfaces, and it does not write output files.

## 6. Current event storage

Task 0005 implements a local SQLite event store. Its database is
`$XDG_DATA_HOME/netforensics/events.sqlite3`, or
`$HOME/.local/share/netforensics/events.sqlite3` when XDG_DATA_HOME is unset.
The schema is tracked with `PRAGMA user_version`; migration to schema v1 is
transactional, and unknown versions are rejected. Each row stores insertion
sequence, event ID, event schema version, UTC timestamp and Unix milliseconds,
source, kind, and the full serialized versioned event. Timestamp and kind/time
indexes support later filtering; sequence is the stable pagination order.

Each monitor cycle is committed in one immediate transaction. Age and count
retention run in the same transaction. Defaults are 30 days and 100000 events;
new events older than the age cutoff are discarded in the same write, while an
event exactly at the cutoff remains eligible for storage.
the database limit is 256 MiB. Configuration ranges and limits are described
in the README and enforced by the CLI/storage API. Reads are paged by sequence,
up to 1000 events per request. Write batches are capped at 4096 events. Storage
failures stop the monitor before that cycle is emitted to stdout; the
in-memory cycle is bounded to the visible interfaces and configured probes.
Database writes are synchronous in the foreground monitor, so slow storage can
delay later sampling; the roadmap tracks possible reliability improvements.

See [ADR 0001](decisions/0001-local-event-storage.md) for the payload/index
trade-off and dependency choice, and [ADR 0002](decisions/0002-probe-incident-lifecycle.md)
for incident semantics. The history CLI reads stored events without writing to
the database. Derived incident records are not persisted.

## 7. Permissions and deployment

Foreground process by default. Favor netlink/sysfs and other unprivileged Linux APIs where possible; do not require blanket root. Permissions for ICMP sockets, journal access, privileged counters, or eBPF vary by distribution; optional collectors can degrade gracefully. Avoid eBPF in the MVP unless justified by an unaddressed requirement.

## 8. Cancellation, reliability, and observability

Cancel scheduling and collection cleanly on Ctrl+C; complete or explicitly abandon pending writes. Timeouts per probe; bounded queue and backpressure policy; record dropped observations and suspend/resume sampling gaps. Log diagnostic metadata to stderr with explicit verbosity and no secrets. Do not mistake system clock jumps for elapsed outages.

## 9. Architectural decision records

Use a lightweight `docs/decisions/NNNN-title.md` only when a meaningful trade-off arises; record context, decision, consequences, and alternatives. No decision record is necessary for every file or library.

### Initial decisions

- Linux-first, Rust-first, CLI-first.
- Local-only, read-only, unprivileged-first.
- Normalize collection events before persistence and diagnosis.
- Replay is a first-class interface to incident detection and analysis.
- Distinguish evidence from hypothesis; safe unknown outcomes.
- Prefer incremental single-package implementation to speculative workspace complexity.
