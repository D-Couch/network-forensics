# Network Forensics

**Status:** Repository foundation, passive interface inventory, explicit one-shot TCP probes, foreground monitoring, bounded local event persistence, a deterministic probe-incident lifecycle core, and read-only incident history are implemented.

Network Forensics is a Linux-first, privacy-conscious network diagnostic tool in early development. The current CLI inventories local interfaces, performs explicitly configured TCP connection probes, emits foreground observations, and displays incident history derived from stored probe events. Diagnostic explanations remain planned.

Unlike a ping-only monitor, it aims to distinguish observed facts from inferred fault domains. The intended initial executable is `netforensics`.

## Target users

Linux desktop users, developers, homelab operators, and support engineers investigating intermittent outages, unexpected link negotiation, DNS trouble, or routing failures.

## Currently implemented

- `netforensics --help` and `netforensics --version`.
- `netforensics interfaces` lists Linux interfaces, operational state, carrier,
  reported speed, and the interface on the lowest-metric active default route
  when available. Missing sysfs/procfs values appear as unavailable. The
  command reads `/sys/class/net` and `/proc/net/route` only.
- `netforensics probe <gateway|external> <IP:PORT> [TIMEOUT_MS]` attempts one
  TCP connection to an explicitly supplied numeric address. It does not resolve
  names or use default endpoints. The default timeout is 3000 ms, with a
  validated range of 1–30000 ms. A successful connection only describes that
  address and port; it does not establish general Internet connectivity. The
  gateway/external role is a caller-supplied label and is not independently
  validated against routing state.
- `netforensics monitor --external <IP:PORT> --external <IP:PORT> [--gateway <IP:PORT>] [--interval-ms MS] [--timeout-ms MS] [--retention-days DAYS] [--max-events COUNT]` emits newline-delimited JSON interface snapshots and probe results until Ctrl+C, and stores each cycle locally in SQLite. It requires two distinct external targets and accepts one optional gateway target. Probes run sequentially with no retries or overlapping rounds. The interval is a pause after each round completes: it defaults to 10000 ms and is bounded to 1000–3600000 ms. A Ctrl+C waits for the in-progress probe to finish or time out (at most 30000 ms).
- `netforensics incidents` reads the existing local database without creating or changing it, then lists incidents derived from stored probe events. It reports target, status, event sequence/times, evidence IDs, and gap/clock-adjustment uncertainty. It makes no network connections and does not store derived incidents. If there is no event database, the command reports that history is not available yet.

The incident lifecycle API analyzes stored/synthetic events without I/O. By default, two consecutive timeouts or network/host-unreachable results for the same role and IP:port within 30 seconds open an incident; two consecutive successful probes recover it. Refused, ambiguous, unsupported, or blocked outcomes do not qualify. Gaps over 60 seconds and backward timestamps reset streaks and are retained as uncertainty markers. Derived incidents are not separately persisted; each history query recomputes them from retained events.

The database is `$XDG_DATA_HOME/netforensics/events.sqlite3`, or
`$HOME/.local/share/netforensics/events.sqlite3` when XDG_DATA_HOME is unset.
On Unix, the application directory is owner-only (`0700`) and the database is
owner read/write (`0600`). Retention defaults to 30 days or 100000 events,
whichever limit is reached first; the database is capped at 256 MiB. The monitor
accepts `--retention-days` from 1 to 3650 and `--max-events` from 1 to 1000000.
Each cycle is committed transactionally before its JSON Lines output is
written. A storage error stops monitoring with an error. Stop the monitor
before deleting the database file to clear history.

## Planned capabilities

- DNS-specific probes and interface change tracking with selected counters.
- Explainable, conservative rule-based incident analysis and human/JSON reports.
- Replayable synthetic incidents for deterministic diagnosis tests.

These are **planned features**, not currently implemented. Derived incident persistence and diagnostic explanations remain planned.

## Safety and privacy

Read-only monitoring by default; no packet payload collection, network configuration changes, or runtime telemetry. The current monitor makes only the TCP connections explicitly configured by the user. Local event history can contain IP addresses and interface names; it is stored in a per-user database with restrictive Unix permissions and is never synced by the application. Shared diagnostic reports must redact identifiable network details by default. Additional privileges and log collection will be opt-in and documented.

## Project documents

- [`docs/PRD.md`](docs/PRD.md): goals, user stories, requirements, boundaries, acceptance criteria.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): components, event schema, data flow, concurrency, failure semantics, and design decisions.
- [`docs/ROADMAP.md`](docs/ROADMAP.md): staged implementation with exit criteria.
- [`docs/TESTING.md`](docs/TESTING.md): verification and fixture strategy.
- [`docs/SECURITY_PRIVACY.md`](docs/SECURITY_PRIVACY.md): threat model and data-handling policy.
- [`AGENTS.md`](AGENTS.md): persistent instructions for Codex.
- [`tasks/0001-repository-foundation.md`](tasks/0001-repository-foundation.md): first narrowly scoped implementation task.

## Getting started with Codex

1. Open this repository directory as a project in Codex.
2. Ask Codex to read `AGENTS.md`, the next assigned task, and its referenced documentation.
3. Review changes and test results at each milestone. Task 0001 is complete; the current implementation and remaining work are summarized in this README and `docs/ROADMAP.md`.

The Cargo package includes `--help`, `--version`, passive interface inventory,
explicit TCP probes, foreground monitoring with local persistence, and a
versioned event model. With Rust and Cargo installed, build and verify it with:

```sh
cargo build
cargo run -- --help
cargo run -- --version
cargo run -- interfaces
cargo run -- probe --help
cargo run -- monitor --help
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

The current event JSON representation is stable for schema version 1:

```json
{
  "schema_version": 1,
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

The caller assigns `id` and is responsible for uniqueness in its collection.
`carrier: null` represents unavailable carrier state, while `false` means the
carrier was observed down. Deserialization rejects schema versions other than
1 and rejects unknown fields. The event model also represents TCP probe results
and interface snapshots. Event IDs are supplied by the caller and should be
unique within its event collection. The SQLite storage schema is separately
versioned and preserves each serialized event along with indexed envelope
metadata.

## Contribution and licensing

The GitHub repository is public, but this project has no license file. Public visibility does not itself grant permission to use, modify, or redistribute the code. Choose and add a license before inviting reuse or accepting contributions; also establish contribution and release policies before a release.
