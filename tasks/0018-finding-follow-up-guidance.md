# Task 0018 — Finding-specific follow-up guidance

**Status:** Complete
**Milestone:** M4
**Scope:** Improve the existing `Finding::suggested_follow_up` text for the six current classifications. Do not change classifications, evidence selection, collection, or CLI behavior.

## Goal

Give users a concrete next check for each finding while keeping observations, hypotheses, and uncertainty distinct.

## Decisions

- Tie each follow-up to the finding's evidence and available signals.
- Do not present a follow-up as proof of a cause or as a remediation instruction.
- Preserve the gateway-role limitation, endpoint-specific scope, DNS query scope, and local-link route uncertainty in the wording.
- For undetermined findings, suggest collecting independent observations rather than naming a fault domain.

## Acceptance checks

- Deterministic tests assert the guidance associated with every classification and retain the relevant uncertainty language.
- Rendered text and JSON continue to include the follow-up field without schema changes.
- No new event types, collection behavior, dependencies, or network requests are added.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build`.

## Stop condition

Stop after the six existing finding classes have useful, evidence-aligned follow-up guidance.
