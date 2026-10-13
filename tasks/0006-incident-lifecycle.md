# Task 0006 — Deterministic probe incident lifecycle

**Status:** Complete  
**Milestone:** M2  
**Scope:** Pure incident grouping over stored probe-result events. No incident CLI, persistence of derived incidents, diagnosis, reports, or new collectors.

## Goal

Turn repeated, temporally bounded probe outcomes into conservative incident records while preserving recovery evidence, unknown outcomes, event sequence, and observation gaps.

## Decisions for this task

- Group by the exact configured `(role, IP:port)` target; do not merge different endpoints or infer a shared root cause.
- A qualifying failure is timeout, network unreachable, or host unreachable. Connection refused shows a response from the endpoint; `Other` and unsupported/blocked outcomes do not qualify.
- Require two consecutive qualifying failures to open an incident, within a configurable failure window (default 30 seconds).
- Require two consecutive successes to mark it recovered. Other outcomes and failures break the success streak.
- Use stored insertion sequence as event order. A timestamp gap greater than the configurable maximum (default 60 seconds) resets streaks and marks an already-open incident as containing an observation gap. A backwards timestamp resets streaks and marks a clock adjustment. Neither condition proves continuous downtime.
- An incident represents repeated failure of one configured TCP probe target, not a diagnosis or general Internet outage. Interface snapshots and link observations remain supporting evidence for future analysis and do not independently open incidents in this task.

## Requirements

- Add a small, independently testable incident module that accepts stored/synthetic events and performs no I/O or networking.
- Make thresholds configurable through a validated Rust configuration API; keep the CLI out of scope.
- Return deterministic incidents with stable IDs based on their first stored sequence, target, start/recovery times and sequences, failure/recovery evidence IDs, open/recovered state, and gap/clock-adjustment markers.
- Reject non-increasing stored sequences and incompatible event schema versions.
- Cover pending failures, threshold opening, endpoint separation, outcome distinctions, recovery, reset behavior, observation gaps, backward/forward clock changes, and configuration/sequence errors with deterministic tests.

## Acceptance checks

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
```

## Documentation

Record the episode semantics and defaults in a lightweight ADR and update the architecture, testing strategy, and roadmap.

## Stop condition

Stop after the pure incident lifecycle engine and tests pass. Do not add `incidents` CLI or diagnostic classifications.
