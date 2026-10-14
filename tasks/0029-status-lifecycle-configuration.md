# Task 0029 — Status lifecycle and configuration

**Status:** Complete
**Milestone:** M4
**Roadmap item:** Extend `status` with active-process and complete monitor-configuration awareness.

## Decisions

- Use a Linux advisory exclusive file lock as the live-process signal. The kernel releases the lock after normal shutdown or process termination, avoiding stale PID and PID-reuse checks.
- Store a versioned JSON configuration record beside the event database in the existing per-user data directory. Keep the directory at `0700` and the record at `0600` on Linux.
- Reject a second monitor while the lock is held. Do not add a background service, event schema change, or database write for status metadata.
- Validate records before displaying them. Show an inactive record as the last configuration, which may be stale; show unknown/unavailable state when the record cannot be trusted.
- Keep the DNS name in the private record so the configuration is complete, but omit its value from status output.

## Acceptance criteria

- `status` reports an active monitor from its held Linux advisory lock and shows its validated intervals, targets, retention, and optional collection settings.
- When no monitor holds the lock, `status` labels a valid saved record as last-run configuration and does not claim that monitoring is active.
- A malformed or unsafe record is not trusted, symlinks are not followed, and DNS query names are omitted from status output.
- Concurrent monitor startup is rejected; the lock is released automatically after shutdown or process exit.
- Deterministic tests cover active/stopped states, concurrent locking, malformed records, symlinks, and DNS-name redaction.
- README, architecture, PRD, and privacy documentation describe the behavior and its Linux-only assumptions.
- Run standard Cargo checks.

## Boundary

The status lock is Linux-specific. It does not prove that probes are succeeding, and the last configuration may no longer reflect current settings. The monitor remains foreground-only.
