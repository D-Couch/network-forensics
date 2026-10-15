# Technical Architecture

**Status:** Describes the implemented single-package CLI and monitoring architecture through Tasks 0001–0045. Sections labeled “Implemented architecture” describe current behavior; “Design constraints and future direction” records boundaries and guidance that may extend beyond current implementation. The roadmap is the planning inventory.

## Implemented architecture

### 1. Logical flow

```text
monitor command:
[interface discovery/tracking] --\
[explicit TCP probes] ------------+--> [ordered cycle events] --> [SQLite transaction]
[opt-in DNS and journal signals] -/                                  |
                                                        [JSON Lines stdout]

database-backed commands:
[SQLite event history] --> [incident lifecycle] --> [analysis] --> [CLI output]
                                                                  \--> [optional report file]
[local replay fixture] ---------------------------> [same incident and analysis APIs]
```

The monitor collects configured observations sequentially, commits each complete cycle, and then writes its events to stdout. The collectors emit *observations*, not conclusions. The incident detector derives incident boundaries from stored events; the separate rule engine evaluates possible fault domains. Replay uses the same incident and analysis APIs without the database or network.

### 2. Modules

- `main.rs`: crate root and thin executable entry point. It declares the public application modules and delegates execution to `cli`; the crate-root event-ID helper remains re-exported for the monitor. `cli`: command dispatch and help text in `cli/mod.rs`, pure option parsing in `cli/args.rs`, command orchestration and local I/O in `cli/commands.rs`, user-facing output construction in `cli/render.rs`, and shared status data in `cli/types.rs`. The boundaries keep argument parsing and rendering separately reviewable from database, filesystem, and network actions.
- `monitor`: coordinates foreground round scheduling, cancellation, collector ordering, cycle persistence, and JSON Lines output. `monitor::config` validates settings and owns monitor limits; `monitor::cycle` tracks interface transitions and collects per-round interface, TCP, and optional DNS events; `monitor::gap` detects elapsed gaps using the boot-time clock. The public configuration and collection APIs remain re-exported from `monitor`.
- `interfaces`: reads Linux interface state and default-route information from local system interfaces.
- `probes` and `dns`: perform explicit TCP connection attempts and opt-in system-configured DNS lookups.
- `journal`: optionally reads and normalizes selected kernel link messages; it does not persist raw journal messages.
- `event`: typed, versioned observation-event definitions and serialization.
- `storage`: local SQLite schema, migrations, transactional cycle writes, bounded retention, and ordered event reads.
- `monitor_state`: Linux advisory-lock lifecycle detection and a bounded, validated private monitor-configuration record.
- `incidents`: deterministic probe-target incident lifecycle over ordered stored events; the CLI derives and filters incidents without persisting them.
- `analysis`: pure evidence-citing rules over events and incidents, shared by stored-history and replay flows.
- `report`: pure versioned text/JSON report construction with target redaction by default.
- `report_output`: bounded local report-file creation with private permissions and no-overwrite behavior.
- `replay`: strict, bounded JSON fixture parsing and invocation of the incident and analysis APIs.

These are Rust modules in one Cargo package, not separate crates. The module map describes current source responsibilities; proposed organization work is listed in the roadmap.

### Current read-only history commands

`analyze --latest` reads retained events by insertion sequence, derives incidents,
and renders their findings. `report --latest` performs the same analysis and
renders incident timelines plus findings as text by default or versioned JSON
with `--format json`. Both commands open the existing database read-only and
perform no network activity. Reports map each distinct target consistently to
a role-scoped label; `--include-identifiers` includes raw target addresses and
prints a sharing warning to stderr. Report bodies contain event IDs and
timestamps for evidence review but omit DNS query names and raw event payloads.
`report --latest --output FILE` optionally writes the same text or JSON report
to a new local file. The writer caps output at 16 MiB, stages the complete
report in a private temporary file, and links it into place without replacing
an existing destination. It sets `0600` permissions on Unix and does not create
parent directories. Derived incidents continue to be recomputed from retained
observations rather than persisted separately.

`status` pages the existing database in insertion order and summarizes its
latest event, probe, interface snapshot/counter sample, and optional DNS and
normalized kernel-link events. Missing optional events are described as
absent from retained history; the command does not infer whether collection was
disabled or unavailable. On Linux, a foreground monitor holds an exclusive
advisory lock on a `0600` versioned configuration record in the `0700` data
directory. `status` attempts a nonblocking shared lock to distinguish an active
monitor from a stopped one. It validates and displays active configuration, or
labels an unlocked record as the last configuration, which may be stale. The
record includes the configured DNS name, but status omits the name from output.
Status performs no network requests, process-table inspection, or writes.

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
allows the current bounded connect attempt to finish. A Linux advisory lock
prevents a second monitor from starting and is released by the kernel on exit.
The `ctrlc` dependency provides portable signal handling; the monitor creates
no background service. Before each round, it compares Linux `CLOCK_BOOTTIME`
with the preceding round's completion; if the measured inter-round time exceeds
the configured pause by more than one second, it puts a `monitoring_gap` event
before the current round's observations. The event is persisted and written
with that cycle. Unavailable or backwards elapsed readings suppress detection.

### 3. Event envelope v5 (reads v1 through v4)

New events use schema v5; deserialization continues to accept schema-v1 through
schema-v4 events. The envelope uses a serde-compatible Rust structure with
fields equivalent to:

```json
{
  "schema_version": 5,
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
`interface_counter_delta`, `kernel_link_signal`, and `monitoring_gap`; these variants are implemented and tested. DNS
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
monitoring continues. The `monitoring_gap` variant records the wall-clock end
of the preceding cycle and the elapsed gap length when it is at most seven
days. The foreground monitor compares Linux `CLOCK_BOOTTIME` readings around
rounds and emits a gap when the time since the prior cycle completion exceeds
the configured pause by more than one second. This clock includes suspend time
and is independent of wall-clock adjustments. If clock readings are missing,
inconsistent, or move backwards, no gap is inferred; an emitted gap never
establishes suspend or a network failure as its cause.

### 5. Incidents and temporal rules

Task 0006 implements `incidents::analyze_events`, a pure Rust API over stored events. It groups by exact `(probe role, numeric IP:port)` and opens an incident after two consecutive timeouts, network-unreachable, or host-unreachable outcomes within 30 seconds. Two consecutive successful TCP connects recover it. Refused, ambiguous, unsupported, or blocked outcomes break streaks and do not qualify. Counts, failure window, and maximum observation gap are configurable with validation; defaults are two failures, two successes, 30 seconds, and 60 seconds. A stored `monitoring_gap` event clears pending failure/recovery streaks and marks every open incident as containing an observation gap, regardless of whether its duration is known.

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

### 6. Current event storage

Task 0005 implements a local SQLite event store. Its database is
`$XDG_DATA_HOME/netforensics/events.sqlite3`, or
`$HOME/.local/share/netforensics/events.sqlite3` when XDG_DATA_HOME is unset.
The schema is tracked with `PRAGMA user_version`; migration to schema v1 is
transactional, and unknown versions are rejected. Existing files are checked
through a read-only connection before private permissions are applied, so an
unsupported schema is rejected without changing the file. Each row stores insertion
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
failures stop the monitor before that cycle is emitted to stdout. Configured
probes are capped, and monitor collection rejects inventories above 900
interfaces before generating cycle events. At most four events per interface,
eight probes, one DNS event, and 100 journal events fit below the 4096-event
storage batch limit. The monitor fails that cycle with a clear error rather
than silently truncating interface observations. Database writes are
synchronous in the foreground monitor, so slow storage can delay later
sampling; there is no asynchronous queue or backlog.

See [ADR 0001](decisions/0001-local-event-storage.md) for the payload/index
trade-off and dependency choice, and [ADR 0002](decisions/0002-probe-incident-lifecycle.md)
for incident semantics. The history CLI reads stored events without writing to
the database. Derived incident records are not persisted.

## Design constraints and future direction

The following guidance defines project boundaries and areas that may need
additional design as capabilities evolve. It is not a claim that every listed
mechanism or capability is implemented. Planned work, when present, is tracked
in the [roadmap](ROADMAP.md).

### Module organization

Keep one Cargo package and Rust modules. Extract crates only if justified by
dependency separation or actual reuse; do not scaffold unused modules.

### Event and evidence principles

- Every event identifies provenance/source and observation time.
- IDs allow reports to cite specific evidence. Stable unique IDs may be generated by the ingestion layer, not each collector.
- Prefer source-specific structured fields over parsing prose in the rules engine.
- **Unknown is distinct from zero, false, and failure.** Optionals and explicit enums should preserve uncertainty.
- Require an event schema version; reject unsupported future versions clearly or handle them through explicit migration.
- Use wall-clock time for presentation and log correlation; use monotonic time for elapsed intervals and debounce in live sessions. Replay must define event order deterministically when timestamps tie or jump.

### Probe and classification constraints

- Distinct targets and probe methods can fail independently; no single probe determines “Internet up/down.”
- Gateways may not reply to ICMP or accept TCP ports; probeability needs explicit state, and gateway reachability should never be inferred solely from a failed arbitrary TCP port.
- Keep network errors, timeouts, active rejections, lookup problems, and unsupported/blocked methods distinguishable in probe results.
- Record configuration changes and network route/interface changes when they are collected and relevant to interpretation.
- Conservative analysis uses correlated evidence and provides alternatives. Do not calculate a numerical confidence score without calibration and evidence.

### 7. Permissions and deployment

Foreground process by default. Favor netlink/sysfs and other unprivileged Linux APIs where possible; do not require blanket root. Permissions for ICMP sockets, journal access, privileged counters, or eBPF vary by distribution; optional collectors can degrade gracefully. Avoid eBPF in the MVP unless justified by an unaddressed requirement.

### 8. Cancellation, reliability, and observability

The current foreground monitor responds to Ctrl+C by stopping new probe work
and allowing the current bounded connection attempt to finish. It writes each
cycle synchronously before printing that cycle's events, so there is no pending
asynchronous write queue. It records detected inter-round gaps as described in
the implemented monitor and event sections above.

Future changes to asynchronous collection or persistence would need an explicit
bounded-queue, backpressure, and dropped-observation policy. Diagnostic output
must not expose secrets, and elapsed-time conclusions must not rely on wall-clock
changes alone.

### 9. Architectural decision records

Use a lightweight `docs/decisions/NNNN-title.md` only when a meaningful trade-off arises; record context, decision, consequences, and alternatives. No decision record is necessary for every file or library.

### Initial decisions

- Linux-first, Rust-first, CLI-first.
- Local-only, read-only, unprivileged-first.
- Normalize collection events before persistence and diagnosis.
- Replay is a first-class interface to incident detection and analysis.
- Distinguish evidence from hypothesis; safe unknown outcomes.
- Prefer incremental single-package implementation to speculative workspace complexity.
