# Network Forensics

**Status:** Repository foundation, passive interface inventory, explicit TCP and opt-in DNS probes, foreground monitoring, bounded local event persistence, deterministic incident lifecycle and analysis APIs, read-only incident history, and offline fixture replay are implemented.

Network Forensics is a Linux-first, privacy-conscious network diagnostic tool in early development. The current CLI inventories local interfaces, performs explicitly configured TCP connection probes, emits foreground observations, and displays incident history derived from stored probe events. Diagnostic findings and reports are available through read-only commands.

Unlike a ping-only monitor, it aims to distinguish observed facts from inferred fault domains. The intended initial executable is `netforensics`.

## Target users

Linux desktop users, developers, homelab operators, and support engineers investigating intermittent outages, unexpected link negotiation, DNS trouble, or routing failures.

## Currently implemented

- `netforensics --help` and `netforensics --version`.
- `netforensics status` summarizes the latest retained observations, including
  explicitly unknown interface values and the latest probe, DNS, and
  normalized kernel-link observations. It reads the database without changing
  it. Because monitoring runs in the foreground and its complete options are
  not stored, status cannot say whether a monitor is currently running or
  reconstruct every configured target. Missing DNS or kernel events do not
  distinguish disabled collection from unavailable/no matching observations.
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
- `netforensics monitor --external <IP:PORT> --external <IP:PORT> [--gateway <IP:PORT>] [--dns-name <FQDN>] [--kernel-journal] [--interval-ms MS] [--timeout-ms MS] [--retention-days DAYS] [--max-events COUNT]` emits newline-delimited JSON interface snapshots, carrier/speed transitions, available RX/TX error/drop counter deltas, and probe results until Ctrl+C, and stores each cycle locally in SQLite. Kernel journal sampling is disabled by default and enabled only with `--kernel-journal`; only normalized link-up/down and watchdog categories are stored, and unavailable access does not stop monitoring. It requires two distinct external targets and accepts one optional gateway target. DNS is queried only when the single optional `--dns-name` is provided; it must be a fully qualified ASCII name ending in a dot, such as `example.com.`. The configured system nameserver setup is used; `/etc/hosts` is bypassed. TCP and DNS observations run sequentially with no application retry or overlapping rounds and use the configured bounded timeout. The interval is a pause after each round completes: it defaults to 10000 ms and is bounded to 1000–3600000 ms. A Ctrl+C waits for the in-progress bounded probe to finish.
- `netforensics incidents` reads the existing local database without creating or changing it, then lists incidents derived from stored probe events. It reports target, status, event sequence/times, evidence IDs, and gap/clock-adjustment uncertainty. It makes no network connections and does not store derived incidents. If there is no event database, the command reports that history is not available yet.
- `netforensics analyze --latest` prints conservative findings for derived incidents. `netforensics report --latest` renders a text report; add `--format json` for versioned JSON. Target addresses are redacted to stable role-scoped labels by default. `--include-identifiers` includes configured target addresses and prints a warning to stderr. Reports retain event IDs and timestamps, omit raw event payloads and DNS query names, and read the database without modifying it.

The incident lifecycle API analyzes stored/synthetic events without I/O. By default, two consecutive timeouts or network/host-unreachable results for the same role and IP:port within 30 seconds open an incident; two consecutive successful probes recover it. Refused, ambiguous, unsupported, or blocked outcomes do not qualify. Gaps over 60 seconds and backward timestamps reset streaks and are retained as uncertainty markers. Derived incidents are not separately persisted; each history query recomputes them from retained events.

The analysis API and CLI provide deterministic, evidence-cited findings using a 60-second correlation window. They can identify local-link-related evidence, failures at the caller-configured gateway-role target, failures across multiple external targets, endpoint-specific evidence when a separate external target succeeds, and a failed configured DNS query alongside independent external TCP success. DNS wording describes concurrent query failure evidence, not proof DNS caused an incident or that the Internet is unavailable. Other cases remain undetermined; findings cite event IDs and state limitations, and do not diagnose root causes.

DNS query names are persisted in the private local event database because they can identify people or organizations; answer counts are retained, returned IP addresses are not. Event schema v4 is written, while schema-v1 through schema-v3 history remain readable. Hickory uses system resolver configuration but does not reproduce every host NSS/resolver behavior; see [ADR 0004](docs/decisions/0004-system-configured-dns-observation.md).

Foreground monitoring also observes known carrier and reported-speed transitions
and per-round RX/TX error/drop counter deltas from sysfs. Cumulative counters
remain in memory only; unavailable metrics stay unknown, and counter decreases
are recorded as possible resets without guessing wraparound. See [ADR 0006](docs/decisions/0006-linux-interface-counter-deltas.md).

Kernel link messages are collected only when monitoring is started with
`--kernel-journal`. The collector recognizes link-up, link-down, and
`NETDEV WATCHDOG` messages, persists normalized categories and optional
interface names, and discards raw text. It uses the current user's journal
permissions and continues monitoring with a one-time warning when access is
unavailable. See [ADR 0007](docs/decisions/0007-kernel-journal-link-signals.md).

The replay API accepts strict JSON fixtures with an independent fixture schema version, event insertion sequences, and versioned event objects. It runs the same lifecycle and analysis APIs without networking or database access. Input is limited to 16 MiB and 100,000 events; fixture cases and expected results live under `tests/fixtures/`. There is no user-facing replay command yet.

The database is `$XDG_DATA_HOME/netforensics/events.sqlite3`, or
`$HOME/.local/share/netforensics/events.sqlite3` when XDG_DATA_HOME is unset.
On Unix, the application directory is owner-only (`0700`) and the database is
owner read/write (`0600`). Retention defaults to 30 days or 100000 events,
whichever limit is reached first; the database is capped at 256 MiB. The monitor
accepts `--retention-days` from 1 to 3650 and `--max-events` from 1 to 1000000.
Each cycle is committed transactionally before its JSON Lines output is
written. A storage error stops monitoring with an error. Stop the monitor
before deleting the database file to clear history.

## Future work

The [`docs/ROADMAP.md`](docs/ROADMAP.md) is the single inventory of proposed
capabilities and release work. Items there are not implemented unless marked
complete, and they are not implementation assignments until a scoped task is
added under `tasks/`.

## Safety and privacy

Read-only monitoring by default; no packet payload collection, network configuration changes, or runtime telemetry. The current monitor makes only the TCP connections explicitly configured by the user and sends DNS queries only for an explicitly configured name. Kernel journal sampling is separately opt-in and uses the current user's existing permissions. Local event history can contain IP addresses, interface names, and DNS query names; it is stored in a per-user database with restrictive Unix permissions and is never synced by the application. Reports redact target addresses by default; `--include-identifiers` opts into including configured target addresses and emits a warning.

## Project documents

- [`docs/PRD.md`](docs/PRD.md): goals, user stories, requirements, boundaries, acceptance criteria.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): components, event schema, data flow, concurrency, failure semantics, and design decisions.
- [`docs/ROADMAP.md`](docs/ROADMAP.md): staged implementation with exit criteria.
- [`docs/TESTING.md`](docs/TESTING.md): verification and fixture strategy.
- [`docs/SECURITY_PRIVACY.md`](docs/SECURITY_PRIVACY.md): threat model and data-handling policy.
- [`AGENTS.md`](AGENTS.md): persistent instructions for Codex.
- [`tasks/0001-repository-foundation.md`](tasks/0001-repository-foundation.md): first narrowly scoped implementation task.

## Getting started

Build the application with a current Rust toolchain and Cargo:

```sh
cargo build
```

Start with commands that only inspect local state:

```sh
cargo run -- --help
cargo run -- interfaces
cargo run -- monitor --help
```

`interfaces` reads local Linux interface and route information; it does not
probe the network. Before monitoring, choose two distinct numeric IP:port
targets for external TCP services that you are allowed to contact and expect
to accept connections. A gateway target is optional and is only a label for
the endpoint you supply; the program does not verify that it is your gateway.
There are no built-in probe destinations or automatic target discovery.

The following is a command template, not a ready-to-run command. Replace both
`<IP:PORT>` placeholders with your chosen targets before running it:

```sh
cargo run -- monitor --external <IP:PORT> --external <IP:PORT>
```

Monitoring makes repeated TCP connection attempts only to the configured
targets. It does not query DNS unless you add `--dns-name <FQDN>`, and it does
not read kernel journal entries unless you add `--kernel-journal`. Review the
data-retention defaults and privacy notes above before starting. Stop with
Ctrl+C; an in-progress probe may finish within its configured timeout. The
monitor stores observations in the local database described above.

After collecting events, inspect them locally with:

```sh
cargo run -- status
cargo run -- incidents
cargo run -- analyze --latest
cargo run -- report --latest
```

History, analysis, and reports do not make network requests. Reports redact
target addresses by default. To develop or verify changes, run:

```sh
cargo test
cargo fmt --check
cargo clippy -- -D warnings
cargo build
```

For Codex work, open this repository as a project, read `AGENTS.md` and the
currently assigned task, and review its changes and test results when complete.

The event JSON representation is versioned. New events use schema version 4; events from schema versions 1 through 3 remain readable:

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

The caller assigns `id` and is responsible for uniqueness in its collection.
`carrier: null` represents unavailable carrier state, while `false` means the
carrier was observed down. Deserialization accepts schema versions 1 through 4
and rejects unsupported versions and unknown fields. The event
model also represents TCP and DNS probe results, interface snapshots, speed
transitions, and counter deltas. DNS
events persist query names and answer counts, not returned addresses. Event IDs are supplied by the caller and should be
unique within its event collection. The SQLite storage schema is separately
versioned and preserves each serialized event along with indexed envelope
metadata.

## Contribution and licensing

GitHub Actions runs `cargo fmt --check`, `cargo test`,
`cargo clippy -- -D warnings`, and `cargo build` on Linux for pushes and pull requests. The same
commands are listed above for local verification.

The GitHub repository is public, but this project has no license file. Public visibility does not itself grant permission to use, modify, or redistribute the code. Choose and add a license before inviting reuse or accepting contributions; also establish contribution and release policies before a release.
