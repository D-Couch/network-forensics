# Task 0035 — Local dashboard proposal

**Status:** Proposed (not assigned)
**Milestone:** Future
**Roadmap item:** Consider a local dashboard if it materially improves incident review.

## Scope

Determine whether a local dashboard solves incident-review needs better than the current CLI, and document the smallest useful local-only design before any implementation is considered.

## Acceptance criteria for a future planning task

- Describe the user workflow and concrete gaps in the existing `incidents`, `analyze`, and `report` commands.
- Assess runtime, dependency, accessibility, security, and maintenance costs against the expected benefit.
- If justified, define a read-only, loopback-only interface and its lifecycle, data exposure, and safe shutdown behavior.
- Recommend proceed or reject with reasons and a separately scoped implementation plan if proceeding.

## Boundary

No server, listener, browser UI, or new dependency is authorized by this proposal. Any implementation requires a separately assigned task and privacy/security review.
