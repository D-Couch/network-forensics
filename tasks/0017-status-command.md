# Task 0017 — Read-only status command

**Status:** Complete  
**Milestone:** M4  
**Scope:** Add `netforensics status` as a summary of retained local observations. Do not add live process detection, network activity, a background service, or a storage schema change.

## Goal

Give users a quick, honest view of what the local event history contains and which values were explicitly unavailable.

## Decisions

- Open the existing default event database read-only. If it does not exist, explain that no observations have been stored and direct the user to `monitor`.
- Show the retained event count and newest event timestamp/sequence using insertion order.
- Show the newest recorded probe and latest interface snapshot/counter sample, including explicit unknown values.
- Show the latest DNS and normalized kernel-journal observations without exposing DNS query names.
- State that the foreground monitor's running state and complete current configuration are not persisted and cannot be reconstructed. Absence of DNS or journal events must not be labeled disabled or unavailable; explain that the history cannot distinguish those cases.
- Keep the command read-only, deterministic over stored events, and free of networking, process inspection, or privilege requirements.

## Acceptance checks

- Tests cover no database/no observations, a populated history, unavailable interface/counter values, missing optional signal events, and read-only behavior.
- Help and README describe the command without implying live monitoring detection or complete configuration reconstruction.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build`.

## Stop condition

Stop after the read-only status summary and its documentation are complete. Keep live monitor state/configuration tracking as separate future work unless the event model is explicitly extended in a later task.
