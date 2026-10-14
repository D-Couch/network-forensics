# Task 0027 — Apply age retention to new events

**Status:** Complete
**Milestone:** M4
**Scope:** Ensure events older than the configured retention cutoff are not inserted into SQLite during a new batch.

## Decisions

- Use the existing `recorded_at_utc` timestamp and the same strict `< cutoff` boundary as pruning stored rows.
- Keep events exactly at the cutoff and newer events.
- Preserve transactional age pruning, event-count retention, and database-size limits.

## Acceptance criteria

- Newly submitted stale events are discarded in that write, not retained until a later cycle.
- Events exactly at the age cutoff remain eligible for insertion.
- Add deterministic in-memory storage coverage and update storage documentation.
- Run standard Cargo checks.

## Stop condition

Stop after the retention boundary is fixed, tested, and documented. Do not change retention defaults, database schema, or event timestamps.
