# Task 0007 — Incident history CLI

**Status:** Complete  
**Milestone:** M2  
**Scope:** Read-only `incidents` command that reads stored observation events in bounded pages and displays lifecycle results. No derived-incident persistence, diagnosis, export/report command, or collection changes.

## Goal

Let users inspect target-level probe incidents derived from their local event history, including open episodes, recovery evidence, and uncertainty markers.

## Requirements

- Add `netforensics incidents` and document it in `--help` and README.
- Open the existing default event database without creating it, changing its schema, or modifying stored data. Report clearly when no database exists or its schema is unsupported.
- Read stored events using the existing insertion-sequence cursor and bounded page API, continue across page boundaries, and pass the ordered history through Task 0006's deterministic lifecycle engine.
- Display each incident's stable ID, target role and IP:port, open/recovered status, start and last-failure times, evidence event IDs, and gap/clock-adjustment markers. An empty history or no incidents must be clear.
- Keep output local and human-readable. Do not add raw-data export, diagnostic hypotheses, network activity, dependencies, or derived incident storage.
- Add deterministic tests for output content, empty history, read-only/missing-database behavior, and an incident whose evidence spans event-page boundaries.

## Acceptance checks

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
cargo run -- --help
```

## Documentation

Update README, PRD, architecture, security/privacy, testing, and roadmap to describe the implemented history query and its limits.

## Stop condition

Stop after the local, read-only incident history command and tests pass. Do not add diagnosis, report/export, or live monitoring features.
