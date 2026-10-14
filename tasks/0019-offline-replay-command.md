# Task 0019 — Offline fixture replay command

**Status:** Complete
**Milestone:** M4
**Scope:** Add `netforensics replay <FIXTURE.json>` using the existing replay API. No fixture format or analysis changes.

## Goal

Let users run a checked-in or local versioned fixture through the same incident and analysis logic without a live failure.

## Decisions

- Read exactly one caller-supplied local file, with a bounded read no larger than the replay API's 16 MiB limit.
- Reuse `replay::replay_json`; do not duplicate validation or analysis rules.
- Render incidents and findings to stdout, keeping the command offline and independent of the event database.
- Do not echo the input path or write files.

## Acceptance checks

- CLI help documents the new command and its offline behavior.
- Deterministic tests cover a valid fixture, rendered findings/evidence, and oversized input rejection.
- Documentation describes the file boundary, output, and no-network/no-database behavior.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build`.

## Stop condition

Stop after the single-file replay command passes its tests and the required checks. Do not add fixture editing, report export, or live monitoring.
