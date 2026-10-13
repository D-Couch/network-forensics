# Task 0012 — Linux interface transitions and counter deltas

**Status:** Complete
**Milestone:** M4
**Scope:** Add passive carrier/speed transition and RX/TX error/drop delta events to foreground monitoring from Linux sysfs. Do not add journald, route-change collection, rate calculations, or new CLI configuration.

## Goal

Make carrier/speed changes and increasing interface error/drop counters visible as local observations without requiring elevated privileges or persisting cumulative counter values.

## Decisions for this task

- Read `rx_errors`, `tx_errors`, `rx_dropped`, and `tx_dropped` from `/sys/class/net/<interface>/statistics/` when available.
- Emit a baseline counter event on a new interface observation with `baseline_available: false`; subsequent cycles emit per-field optional deltas. Missing values remain `null` and never become zero.
- If a current counter is less than its previous value, mark `counter_reset_detected` and omit that counter's delta; do not guess whether wraparound or driver reset occurred.
- Emit `LinkStateChanged` only when two known carrier values differ. Emit `LinkSpeedChanged` when reported speed changes, retaining old/new optional values so unavailable remains distinct from zero.
- Keep only prior in-memory samples for delta computation; persist deltas and transition observations, not cumulative counters.
- New events use schema v3; continue reading schema v1 and v2. SQLite schema does not change.

## Acceptance checks

- Deterministic tests cover supported/missing sysfs counter files, initial baseline, ordinary deltas, partial unknown metrics, interface disappearance/reappearance, counter decreases/resets, carrier transitions, speed downshift, and stable event order.
- Monitoring emits snapshots plus the typed transition/delta events and persists them in the same cycle.
- Existing analysis, incident, and replay paths continue accepting v1/v2 events and ignore unrelated counter events safely.
- No privileged API, live network connection, journald, or external dependency is added.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, `cargo build`, and `cargo run -- --help`.

## Documentation

Record the reset/unknown semantics in an ADR; update README, PRD, architecture, testing, security/privacy, and roadmap.

## Stop condition

Stop after bounded sysfs interface transition and counter-delta events are recorded locally. Do not add journal monitoring, analysis rules, or rate calculations.
