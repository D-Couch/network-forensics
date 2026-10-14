# Task 0029 — Status lifecycle and configuration proposal

**Status:** Proposed (not assigned)
**Milestone:** Future
**Roadmap item:** Extend `status` with active-process and complete monitor-configuration awareness.

## Scope

Define a safe, local mechanism for `status` to report whether a monitor process is active and which configuration it is using. The proposal must account for stale process records, concurrent monitors, process identity reuse, configuration privacy, and cleanup after normal or interrupted shutdown.

## Acceptance criteria for a future implementation task

- Specify a lifecycle/configuration record format, ownership and permissions, locking behavior, stale-state handling, and shutdown cleanup.
- Report only validated configuration and clearly distinguish unavailable or stale state from an active monitor.
- Do not persist probe secrets or imply that a process record proves probes are succeeding.
- Add deterministic tests for active, stale, malformed, concurrent, and unavailable state.
- Document platform assumptions and privacy implications before implementation is assigned.

## Boundary

This proposal adds no background service and does not change current monitor behavior. Implementation requires a separately assigned task after the lifecycle design is reviewed.
