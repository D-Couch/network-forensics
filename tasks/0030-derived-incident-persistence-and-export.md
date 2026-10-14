# Task 0030 — Derived incident persistence and export proposal

**Status:** Proposed (not assigned)
**Milestone:** Future
**Roadmap item:** Consider persistence of derived incidents and file export for reports.

## Scope

Evaluate whether derived incidents should be stored or continue to be recomputed, and define a user-directed way to write reports to a local file. Keep observation events as the source evidence and make any derived representation reproducible from them.

## Acceptance criteria for a future implementation task

- Compare recomputation with persisted derived records, including schema evolution, invalidation, and consistency after event retention.
- Define an explicit, bounded export interface with safe overwrite/error behavior and the existing sanitized-by-default policy.
- Exclude DNS query names and raw event payloads unless a separately reviewed privacy requirement changes that rule.
- Identify migration, determinism, and failure tests required before implementation.

## Boundary

This is a design proposal only. It does not change the SQLite schema, report output, or filesystem behavior.
