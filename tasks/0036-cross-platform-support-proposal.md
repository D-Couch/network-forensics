# Task 0036 — Cross-platform support proposal

**Status:** Proposed (not assigned)
**Milestone:** Future
**Roadmap item:** Evaluate cross-platform support as a separate proposal.

## Scope

Assess whether supporting an operating system beyond Linux is practical and define a platform-neutral core boundary plus one candidate platform plan, without changing current Linux behavior.

## Acceptance criteria for a future planning task

- Inventory Linux-specific collection, storage, permissions, and tests, separating portable analysis/storage from platform adapters.
- Select and justify at most one candidate platform for an initial feasibility proposal.
- Specify equivalent signal availability and explicit unsupported states; do not promise parity where a signal has no counterpart.
- Estimate dependencies, CI coverage, maintenance cost, privacy behavior, and a staged implementation sequence.

## Boundary

This proposal does not add another target, platform-specific code, or CI configuration. Platform support remains a non-goal until a separate proposal is accepted and implementation is assigned.
