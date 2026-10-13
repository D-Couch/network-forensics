# Task 0008 — Conservative analysis core

**Status:** Complete  
**Milestone:** M3  
**Scope:** Pure Rust rule analysis over stored observations and Task 0006 incidents. No analysis/report CLI, JSON export, DNS collector, new event variants, or automated remediation.

## Goal

Produce deterministic, evidence-cited findings from currently supported link-state and TCP probe events while preserving ambiguity and caller-provided target labels.

## Decisions for this task

- Use a validated 60-second correlation window and stored insertion sequence as the stable event ordering. Do not make timestamp-based correlations when the incident contains a backward clock adjustment.
- Call a carrier-down correlation “local-link-related evidence,” never a physical diagnosis.
- Treat gateway role as caller-provided metadata. A gateway-role incident is reported as a failure of that configured target, not proof of gateway/LAN failure.
- Describe multiple external target failures as multi-target failure evidence, not proof of a shared upstream cause.
- Call an external incident endpoint-specific only when another distinct external target succeeds in the correlation window and there is no qualifying failure for that other target in the same window.
- Keep cases without supporting evidence undetermined. DNS-specific classification is unavailable until DNS observations exist.
- Every finding cites event IDs, states limitations, and offers a bounded follow-up check. Do not use confidence scores or causal certainty.

## Requirements

- Add an independently testable analysis module with no I/O, networking, or privileges.
- Return one deterministic finding per supplied incident, ordered by incident start sequence, with a stable incident ID reference, classification, explanation, evidence event IDs, limitations, and follow-up suggestion.
- Use only existing event types and preserve unknown/unsupported outcomes.
- Reject non-increasing sequences and incompatible schemas.
- Cover link evidence, configured gateway target wording, endpoint-specific evidence, multiple external failures, stale/ambiguous evidence, clock adjustment, and input validation in synthetic tests.

## Acceptance checks

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
```

## Documentation

Record the correlation rules in an ADR and update the README, PRD, architecture, testing strategy, and roadmap.

## Stop condition

Stop after the pure analysis API and tests pass. Do not add CLI reports, replay fixtures, DNS observation, or live monitoring.
