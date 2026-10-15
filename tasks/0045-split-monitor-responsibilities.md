# Task 0045 — Split monitor collection responsibilities

**Status:** Proposed (not assigned)
**Milestone:** Maintainability
**Scope:** Refactor `src/monitor.rs` along existing collection responsibilities into cohesive modules while retaining a clear monitor orchestration layer. Preserve behavior; do not add collection signals or change scheduling policy.

## Goal

Make distinct monitoring concerns independently understandable and testable without scattering the cycle lifecycle across modules.

## Requirements

- Inspect existing ownership and test boundaries before extracting code; use existing concepts such as cycle collection, interface transition tracking, and foreground runner/output as candidate seams.
- Keep scheduling, cancellation, ordering, cycle persistence, and stdout emission semantics explicit in the orchestration layer.
- Preserve event contents and ordering, limits, optional collector behavior, errors, warnings, and privacy guarantees.
- Keep extracted modules small with clear contracts; retain focused tests near the logic they cover where practical.
- Do not add dependencies, collectors, background services, or unrelated behavior changes.

## Acceptance criteria

- `monitor.rs` primarily coordinates a cycle and its lifecycle rather than owning all collection details.
- Extracted modules have clear responsibilities and do not create circular dependencies.
- Existing monitoring and failure-path behavior remains unchanged and is covered by the existing or appropriately relocated deterministic tests.
- `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build` pass.
- The architecture document's module map is updated if the resulting module responsibilities materially differ.
