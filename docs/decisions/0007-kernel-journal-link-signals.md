# ADR 0007 — Optional kernel journal link signals

**Status:** Accepted for Task 0013

## Context

Kernel logs can contain network link transitions useful beside interface and probe observations. Journal messages also contain arbitrary system details, and access varies by host and user. General log ingestion would retain unnecessary private data and could make ordinary monitoring depend on journald.

## Decision

- Invoke `journalctl` only when the user enables `--kernel-journal`; invoke it directly without a shell or elevated privileges.
- Read kernel entries as JSON, using a journal cursor held only in process memory, with a 100-entry limit, 1 MiB output cap, and two-second timeout.
- Recognize only link-up, link-down, and `NETDEV WATCHDOG` message patterns. Store a typed category and a validated optional interface name; never retain raw message text, stderr, or the cursor.
- Warn once and continue other observations if the command is unavailable, denied, malformed, oversized, or times out.
- Write new events using schema v4 and retain reading support for v1–v3. Keep the SQLite schema unchanged.

## Consequences

This provides a small normalized signal set without granting extra permissions or retaining general logs. Driver-specific wording outside the allowlist is ignored. A journal gap, denial, or timeout means journal evidence may be incomplete; it does not imply link health or failure.

## Alternatives considered

- Collect all system journal entries: rejected because it captures unrelated private information.
- Store original log messages: rejected because messages are untrusted, variable, and may reveal sensitive system details.
- Run with elevated permissions or change journal group membership: rejected because optional observations do not justify changing host permissions.
- Require journald for monitoring: rejected because journal support and access vary across Linux systems.
