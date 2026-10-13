# Task 0014 — Correlate kernel link signals with incidents

**Status:** Complete
**Milestone:** M4
**Scope:** Extend the existing local-link-related analysis rule to accept normalized kernel link-down and watchdog events. Do not add a new classification or collect new signals.

## Goal

Use Task 0013 kernel journal observations as additional local-link evidence when they occur near an existing probe incident.

## Decisions for this task

- Correlate `KernelLinkSignalCategory::LinkDown` and `NetdevWatchdog` within the existing configured timestamp window.
- Treat `LinkUp`, absent/unknown event types, and out-of-window signals as neutral.
- Preserve insertion sequence for evidence ordering and existing behavior that skips timestamp correlations when timestamps move backwards or the incident has a clock-adjustment marker.
- Describe kernel messages as observations only. Do not infer a physical cause, verified interface-to-probe route, or that the signal caused the incident.
- Keep event schema and SQLite schema unchanged.

## Acceptance checks

- Deterministic tests cover link-down and watchdog support, neutral link-up, out-of-window events, and backwards-timestamp suppression.
- Findings cite the supporting journal event IDs and retain explicit limitations.
- Existing carrier-based findings, other classifications, and v1–v4 event compatibility remain unchanged.
- No journal access, network activity, new dependency, CLI option, or persistence change is added.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build`.

## Documentation

Update ADR 0003, PRD, architecture, testing, and roadmap to describe the additional evidence source and limits.

## Stop condition

Stop after normalized kernel link-down/watchdog events can support the existing local-link-related evidence classification. Do not add a new finding class or analyze unrelated journal categories.
