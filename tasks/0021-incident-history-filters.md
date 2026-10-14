# Task 0021 — Incident history filters and comparison

**Status:** Complete
**Milestone:** M4
**Scope:** Add optional time/target filters and same-target before/after probe comparison to the read-only `incidents` command. Preserve full-history incident detection.

## Decisions

- `--since` and `--until` accept RFC3339 timestamps and inclusively filter incidents by start time.
- `--target` accepts an exact numeric IP:port and matches incidents regardless of role.
- Compute incidents from the complete retained event sequence before applying display filters; never filter events before lifecycle analysis.
- `--compare` shows the latest same-target probe before the incident and the first recovery-streak success after failures. Open incidents have no after-recovery observation yet.
- Comparison describes probe outcomes only and makes no causal claim. Keep the command read-only.

## Acceptance checks

- CLI accepts and validates each option, rejects duplicates, invalid timestamps/targets, and reversed time bounds.
- Deterministic tests cover inclusive time filters, exact target matching, full-history lifecycle behavior, and before/after rendering for recovered and open incidents.
- Update user and design documentation.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build`.

## Stop condition

Stop after incident listing filters and same-target probe comparison are implemented. Do not add database indexes, incident persistence, or other history views.
