# Task 0009 — Versioned fixture replay

**Status:** Complete  
**Milestone:** M3  
**Scope:** Pure replay of bounded, versioned synthetic event fixtures through the existing incident and analysis APIs. No replay CLI, DNS collection, report/export command, or live networking.

## Goal

Make representative stored-event scenarios reproducible through the same lifecycle and analysis rules used by the application.

## Requirements

- Define a strict fixture envelope with its own schema version, ordered event sequence numbers, and existing versioned event objects.
- Reject unsupported fixture versions, invalid event schemas, non-increasing sequences, duplicate event IDs, unknown fields, oversized input, and excessive event counts.
- Replay deterministically through `incidents::analyze_events` and `analysis::analyze_incidents`; do not duplicate those rules.
- Add versioned human-readable fixture scenarios and separate expected-result declarations for supported link evidence, endpoint-specific evidence, and undetermined outcomes.
- Keep replay independent of networking, privileges, local databases, and system clock.

## Decisions for this task

- Fixture envelope version starts at 1 and is independent of event schema v1.
- Bound fixture JSON to 16 MiB and 100,000 events.
- Store sequence alongside each event; input order must already be strictly increasing and is never repaired by sorting.
- Do not infer DNS outcomes from TCP probes. Cases that require unavailable evidence remain undetermined.

## Acceptance checks

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
```

## Documentation

Record fixture format and limits in the architecture, testing strategy, and roadmap. Update PRD and README to reflect the replay API boundary.

## Stop condition

Stop after the pure replay API and deterministic fixture scenarios pass. Do not add user-facing replay/report commands or new observation sources.
