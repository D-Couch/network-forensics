# ADR 0001 — Local event storage

**Status:** Accepted for Task 0005

## Context

The foreground monitor emitted versioned observation events as JSON Lines but did not preserve history. Later incident analysis needs stable, paged access to those same events. The storage layer must remain local, bounded, and testable without network activity.

## Decision

- Store events in one SQLite database at `$XDG_DATA_HOME/netforensics/events.sqlite3`, or `$HOME/.local/share/netforensics/events.sqlite3` when XDG_DATA_HOME is absent.
- Use `rusqlite` with bundled SQLite so deployment does not depend on a system SQLite library.
- Store the complete schema-versioned event JSON with indexed envelope metadata. Use an append-only integer sequence as the stable read cursor; wall-clock timestamps are for presentation and time filtering, not unique ordering.
- Track the storage schema in SQLite `PRAGMA user_version`, apply schema changes transactionally, and reject unknown schema versions.
- Commit each monitor cycle in one transaction. Apply age and row-count retention in that transaction. Default retention is 30 days and 100,000 events; cap the database at 256 MiB. The CLI may narrow/adjust retention within documented limits.
- Bound one write batch to 4096 events and one read page to 1000 events to keep memory use predictable.
- Keep the database owner-only on Unix, refuse a symbolic link at the database path, and do not enable synchronization or telemetry.

## Consequences

The raw event envelope remains authoritative and can be deserialized by the existing model, while metadata indexes make future paging and filtering straightforward. SQLite adds a native dependency build step, addressed by bundling. The database may contain IP addresses and interface names from probe and interface events, so local file permissions are part of the privacy boundary. A write failure stops monitoring rather than allowing output and stored history to diverge silently.

## Alternatives considered

- JSON Lines append-only storage is simpler, but it lacks transactional cycle writes, indexed retrieval, and straightforward bounded retention.
- Fully normalizing each event variant into separate tables duplicates the evolving model and complicates migrations. Keep the typed JSON payload and index only common envelope metadata for now.
