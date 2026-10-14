# Task 0022 — Practical examples and sanitized report samples

**Status:** Complete
**Milestone:** M4
**Scope:** Add copyable examples for the existing setup/replay/report workflows and sanitized text/JSON report snapshots. Defer binary installation until release preparation is complete.

## Context

The roadmap asks for prebuilt installation as well as examples. The repository has no license, and release preparation says to establish a license and release policy before distribution. This task resolves the conflict conservatively: document the dependency and complete the independent practical-examples portion without publishing binaries or calling a source build a no-build install path.

## Decisions

- Use existing checked-in fixtures and CLI commands; invent no probe destinations.
- Include synthetic sanitized text and JSON reports with role-scoped target labels and no private addresses or DNS names.
- Make clear that report event IDs/timestamps are synthetic and that text is a snapshot of the current format.
- Keep the prebuilt installation path on the roadmap, gated on license/release preparation.

## Acceptance checks

- README provides practical commands for target setup, offline replay, and local reports.
- A sanitized report is available in both text and JSON, with identifiers marked redacted in JSON.
- Roadmap separates completed examples from deferred binary installation and states the release-preparation dependency.
- Validate sample JSON and run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build`.

## Stop condition

Stop after the examples and sample reports are added and validated. Do not create release workflows, publish binaries, select a license, or configure a package repository.
