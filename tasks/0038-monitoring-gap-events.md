# Task 0038 — Record detected monitoring gaps

**Status:** Complete
**Milestone:** Future reliability
**Roadmap item:** Make missing monitoring intervals explicit in event history.

## Problem

Incident analysis can mark an open incident uncertain when probe timestamps
are far apart, but the monitor does not record a typed event that identifies
the observation gap. The history therefore cannot distinguish a gap in
collection from ordinary absence of probe events, and wall-clock changes make
the elapsed interval ambiguous.

## Scope

- Define a versioned `monitoring_gap` event with UTC wall-clock timestamps and
  an explicitly bounded elapsed duration or unknown duration.
- Detect a gap between monitor rounds using an elapsed-time source that can
  account for Linux suspend/resume where available. Do not attribute suspend
  as the cause without independent evidence.
- Keep the event in the normal cycle persistence/output ordering and preserve
  conservative behavior if a duration cannot be determined.
- Update analysis/report wording only as needed to surface the recorded gap
  without asserting a root cause.

## Acceptance criteria

- Deterministic tests cover ordinary intervals, an extended gap, a backwards
  wall-clock adjustment, and unavailable/ambiguous elapsed-time data using an
  injected clock.
- A detected gap is represented by the typed event, persisted in sequence,
  serialized with the current event schema, and accepted safely by existing
  history and replay paths.
- Incident/report output identifies a recorded observation gap while keeping
  its cause unknown unless independently observed.
- Existing event versions, normal probe ordering, and interrupted-shutdown
  behavior remain compatible.
- No root requirement, network request, background service, automatic
  remediation, or dependency is added without a separately documented need.
- Update relevant architecture, PRD, privacy, and testing documentation.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and
  `cargo build`.

## Boundary

Do not implement general scheduler redesign, queueing, or a broad event-loss
framework. Do not claim that a gap proves system suspend or network failure.
