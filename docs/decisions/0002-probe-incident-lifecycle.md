# ADR 0002 — Conservative probe incident lifecycle

**Status:** Accepted for Task 0006

## Context

The event store preserves TCP probe results, but the project does not yet define when repeated results form an incident. A failed TCP connect has several meanings: timeout and routing-unreachable outcomes provide reachability-failure evidence, while connection refusal shows that the endpoint responded. Unsupported methods and unspecified errors are ambiguous. Wall-clock changes and sampling gaps also make continuous-outage claims unsafe.

## Decision

- Keep episode grouping per exact `(probe role, numeric IP:port)` target. Do not merge endpoints or claim a common cause.
- Treat only timeout, network-unreachable, and host-unreachable outcomes as qualifying failures.
- Open an episode after two consecutive qualifying failures no more than 30 seconds apart. Recover after two consecutive successful TCP connects.
- Expose failure/recovery counts, the failure window, and maximum observation gap as a validated configuration API. Use 60 seconds as the default maximum gap.
- Use stored insertion sequence as event order. A forward timestamp gap beyond the configured maximum clears pending streaks and marks an open incident as containing an observation gap. A backwards timestamp clears streaks and marks a clock adjustment. Keep an open episode open across these uncertainties until recovery evidence arrives.
- Derive the stable incident ID from its first stored sequence. Preserve failure and recovery event IDs in the result.

## Consequences

This catches repeated target-level TCP probe failures without treating one transient result, an arbitrary refused port, or a blocked method as an outage. Per-target grouping avoids attributing unrelated endpoint failures to one layer. Gaps and clock changes remain explicit limitations. A later task can page stored events through this pure engine and expose incident history without requiring network access.

## Alternatives considered

- Grouping all failed probes into one incident could imply a shared fault domain without sufficient evidence.
- Treating one failure or one successful probe as decisive would be sensitive to transient endpoint behavior.
- Sorting by UTC timestamps would produce unreliable order when the system clock changes; persisted sequence is authoritative.
