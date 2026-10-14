# Task 0032 — Interface and route evidence proposal

**Status:** Proposed (not assigned)
**Milestone:** Future
**Roadmap item:** Add further interface evidence rules and richer route-change diagnostics.

## Scope

Identify specific interface or route observations that can improve incident timelines, then define conservative evidence rules that can be evaluated from stored or synthetic events without networking or elevated privileges.

## Acceptance criteria for a future implementation task

- Name the proposed signals, their Linux sources, availability and permission limits, event representation, and resource bounds.
- Define what each signal can support and what it cannot establish; preserve unknown and ambiguous states.
- Define deterministic fixture scenarios including missing, reordered, stale, and contradictory evidence.
- Version any serialized event changes and update architecture/privacy documentation when behavior is approved.

## Boundary

No new collection, dependency, or diagnosis rule is authorized by this proposal. Implement each accepted signal through a separately assigned task.
