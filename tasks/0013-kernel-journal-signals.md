# Task 0013 — Optional kernel journal link signals

**Status:** Complete
**Milestone:** M4
**Scope:** Add opt-in, read-only sampling of a narrow set of kernel network-link messages from `journalctl`. Do not collect general system logs, infer root cause, add background services, or change network settings.

## Goal

Correlate explicit kernel link messages with existing interface and probe observations while preserving normal monitoring when journal access is absent.

## Decisions for this task

- Require an explicit `--kernel-journal` monitor option. Without it, do not invoke `journalctl`.
- Invoke `journalctl` directly without a shell, with kernel-only JSON output, no pager, and a bounded result count. Do not request root or change permissions.
- Recognize only a small documented set of link-up, link-down, and `NETDEV WATCHDOG` messages. Store a typed category and an optional parsed interface name; never store raw log text, command stderr, or journal cursors.
- Keep any cursor only in memory. If `journalctl` is missing, access is denied, output is malformed, or a query fails, report reduced observability once on stderr and continue other monitoring.
- New observations use event schema v4; read schemas v1–v3. No SQLite schema change.

## Acceptance checks

- Parser tests cover supported and ignored messages, malformed JSON, missing fields, and unsafe/unrecognized interface text without retaining message contents.
- Collector tests use an injected command runner; no test invokes the host journal.
- Monitoring makes no journal request unless explicitly opted in; unavailable journal access does not stop interface/probe sampling or persistence.
- No external dependency, privileged API, raw log retention, or general-purpose log collection is added.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, `cargo build`, and `cargo run -- --help`.

## Documentation

Record filtering, privacy, and permission semantics in an ADR; update README, PRD, architecture, testing, security/privacy, and roadmap.

## Stop condition

Stop after opt-in normalized kernel link signals are stored with ordinary monitor cycles. Do not add analysis rules, non-kernel journal collection, or future milestones.
