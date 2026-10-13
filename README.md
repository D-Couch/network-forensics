# Network Forensics

**Status:** Repository foundation, passive interface inventory, explicit one-shot TCP probes, and foreground monitoring are implemented; persistence and analysis remain planned.

Network Forensics is a Linux-first, privacy-conscious network diagnostic tool that connects observed connectivity failures to local interface, routing, DNS, and system events. It helps answer *what happened, when, what evidence supports an explanation, and what to check next*.

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
- `netforensics monitor --external <IP:PORT> --external <IP:PORT> [--gateway <IP:PORT>] [--interval-ms MS] [--timeout-ms MS]` emits newline-delimited JSON interface snapshots and probe results until Ctrl+C. It requires two distinct external targets and accepts one optional gateway target. Probes run sequentially with no retries or overlapping rounds. The interval is a pause after each round completes: it defaults to 10000 ms and is bounded to 1000–3600000 ms. A Ctrl+C waits for the in-progress probe to finish or time out (at most 30000 ms).

## Planned capabilities

- Periodic local-gateway and multiple external endpoint probes, with explicit handling of blocked/unresponsive probe methods.
- Interface change tracking, selected counters, and persisted monitoring history.
- Timestamped local evidence and persistent SQLite incident history.
- Explainable, conservative rule-based incident analysis and human/JSON reports.
- Replayable synthetic incidents for deterministic diagnosis tests.

These are **planned features**, not currently implemented.

## Safety and privacy

Read-only monitoring by default; no packet payload collection, network configuration changes, or remote telemetry. Shared diagnostic reports must redact identifiable network details by default. Additional privileges and log collection will be opt-in and documented.

## Project documents

- [`docs/PRD.md`](docs/PRD.md): goals, user stories, requirements, boundaries, acceptance criteria.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md): components, event schema, data flow, concurrency, failure semantics, and design decisions.
- [`docs/ROADMAP.md`](docs/ROADMAP.md): staged implementation with exit criteria.
- [`docs/TESTING.md`](docs/TESTING.md): verification and fixture strategy.
- [`docs/SECURITY_PRIVACY.md`](docs/SECURITY_PRIVACY.md): threat model and data-handling policy.
- [`AGENTS.md`](AGENTS.md): persistent instructions for Codex.
- [`tasks/0001-repository-foundation.md`](tasks/0001-repository-foundation.md): first narrowly scoped implementation task.

## Getting started with Codex

1. Extract this starter package into the local repository directory.
2. Open the **directory** as a project in Codex.
3. Ask Codex to read `AGENTS.md` and the relevant task file, then consult the referenced documentation.
4. Review changes and test results at each milestone.

The Cargo package includes `--help`, `--version`, a passive interface inventory,
and a versioned link-state event model. With Rust and Cargo installed, build and
verify it with:

```sh
cargo build
cargo run -- --help
cargo run -- --version
cargo run -- interfaces
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
1 and rejects unknown fields. Monitoring and analysis commands remain planned.

## Contribution and licensing

Before a public release, choose and add an open-source license, contribution policy, and release workflow. No license is granted merely by publishing a repository without a license file.
