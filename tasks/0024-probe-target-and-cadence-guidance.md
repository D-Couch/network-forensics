# Task 0024 — Probe target and cadence guidance

**Status:** Complete
**Milestone:** M4
**Scope:** Explain appropriate target selection and the actual monitoring cadence in setup output, CLI help, and README. Do not change probe defaults, cadence, or network behavior.

## Decisions

- Recommend targets the user is permitted to contact and expects to accept TCP on the selected port.
- State that each round probes configured TCP targets sequentially, once each, with no application retries; optional DNS is separately opt-in.
- Clarify that `--interval-ms` is a pause after a completed round, so per-target cadence also includes round duration. Longer pauses reduce request frequency.
- Do not prescribe a new numeric interval or target list.

## Acceptance criteria

- Setup and CLI help make target permission and repeated-probe cadence clear.
- README explains how interval relates to per-target probe frequency and how to lower that frequency.
- Deterministic tests check user-facing setup/help guidance without networking.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build`.

## Stop condition

Stop after user-facing documentation/help and focused tests are complete. Do not change monitor behavior, targets, defaults, or DNS/network requests.
