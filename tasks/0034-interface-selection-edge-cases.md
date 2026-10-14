# Task 0034 — Interface-selection edge cases proposal

**Status:** Proposed (not assigned)
**Milestone:** Future
**Roadmap item:** Resolve remaining interface-selection edge cases.

## Scope

Inventory ambiguous or unsupported default-route and interface-selection situations, then specify deterministic selection or explicit-unavailable behavior for each accepted case.

## Acceptance criteria for a future implementation task

- Capture concrete route-table and interface-state examples for the unresolved cases.
- Define tie-breaking, filtering, and unavailable outcomes without inferring reachability from route presence.
- Add deterministic parser/selection tests for malformed, duplicate, changing, and ambiguous routes.
- Keep observation read-only and do not privilege route state over direct probe evidence in analysis.

## Boundary

This proposal does not change interface discovery behavior. Each resolved case must be assigned as a separate implementation task with its fixtures.
