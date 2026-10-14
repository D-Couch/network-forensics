# Task 0028 — Preserve incompatible database files

**Status:** Complete
**Milestone:** M4
**Scope:** Avoid changing an existing database file before determining that its SQLite schema version is supported.

## Decisions

- Inspect existing databases through a read-only SQLite connection before applying private file permissions or opening them for writes.
- Accept the empty version 0 database used for first-time initialization and the current schema; continue to reject unknown versions.
- Keep the existing write-time schema validation and database size limits as defense in depth.

## Acceptance criteria

- Opening a database with an unsupported schema returns the existing unsupported-schema error without changing its contents or Unix permissions.
- New databases and supported existing databases continue to receive private permissions and normal initialization.
- Run standard Cargo checks.

## Stop condition

Stop after preflight validation is implemented, tested, and documented. Do not change the database schema or retention behavior.
