# Task 0020 — Guided target setup

**Status:** Complete
**Milestone:** M4
**Scope:** Add an interactive `setup` command that validates user-entered target endpoints and prints a monitor command. Do not start probes or monitoring.

## Goal

Help new users choose valid explicit targets and understand the limits of the gateway-role label before monitoring.

## Decisions

- Prompt for two external numeric IP:port endpoints on distinct IP addresses, plus an optional gateway-role endpoint distinct from both external IP:port targets.
- Explain that external endpoints should be services the user may contact and expects to accept TCP.
- Explain that a gateway-role target is caller-provided metadata and does not verify the active gateway or prove gateway reachability.
- Print a reviewable command only; setup performs no network, database, or system-state access.
- Keep monitor options and defaults unchanged.

## Acceptance checks

- CLI help documents `setup`.
- Deterministic tests cover invalid and duplicate targets, optional gateway selection, EOF, and the generated command/disclaimer.
- README and design docs explain setup's prompts and that monitoring starts only after the user runs the printed command.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build`.

## Stop condition

Stop after the setup command and its tests are complete. Do not add automatic destination discovery or begin monitoring from setup.
