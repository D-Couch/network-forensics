# Task 0005 — Local event storage

**Status:** Complete  
**Milestone:** M2  
**Scope:** SQLite event persistence and bounded retention for foreground monitor output. No incident grouping, incident CLI, analysis, DNS probing, or additional collectors.

## Goal

Persist the existing versioned observation events locally so a later task can query event history and build incident timelines.

## Requirements

- Use one per-user SQLite database. Resolve its path from absolute `XDG_DATA_HOME`, falling back to `$HOME/.local/share`; reject missing or relative base paths.
- Keep the database local, never sync it, and use owner-only directory/file permissions on Unix. Refuse a symbolic link at the database path.
- Version the database schema independently from the event envelope. Apply migrations transactionally and reject unsupported future versions without changing the database.
- Persist the complete versioned event JSON plus indexed metadata for sequence, event ID, timestamp, source, and event kind. Preserve insertion sequence as the reliable storage order; do not treat wall-clock timestamps as a unique order.
- Persist each monitor cycle atomically. On storage failure, report an error and stop monitoring rather than silently losing observations. Print a cycle to stdout after its database transaction succeeds.
- Bound retained data by configurable age and event-count limits, and impose a database file size limit. Defaults and accepted ranges must be documented and validated.
- Bound write batches and provide a sequence-cursor read API with a fixed maximum page size so memory use stays predictable.
- Keep storage tests deterministic and independent of live networking. Cover migrations, durable reopen/read, atomic rollback on a failed batch, event/schema rejection, retention, unsupported schema, path validation, and Unix permissions/symlink rejection.

## Out of scope

Incident detection/grouping, `incidents` CLI, analysis and reports, new collectors, automatic remediation, background services, live-network verification, Git publishing.

## Acceptance checks

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
cargo run -- --help
```

## Documentation

Record the schema, migration, database location, retention defaults, permissions, and trade-offs in `docs/ARCHITECTURE.md`, `docs/SECURITY_PRIVACY.md`, and a lightweight ADR. Update README usage and the roadmap to reflect the implemented boundary.

## Stop condition

Stop after local monitor observations persist and all checks pass. Do not implement incident thresholds, event grouping, or history CLI in this task.
