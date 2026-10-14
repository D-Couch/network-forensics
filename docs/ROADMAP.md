# Roadmap

Milestones are sequential targets, not commitments to ship all features immediately. Every milestone must leave the application in a usable, tested state. Implement *one assigned task at a time*.

## M1 — CLI and live observation

- [x] **Task 0001 — Repository foundation:** Cargo package, help/version, versioned event envelope, tests, formatting/lint/build. No live network work in this task.
- [x] **Task 0002 — Interface discovery/state:** select interface/default route where possible; report unavailable fields clearly.
- [x] **Task 0003 — Probe runner:** configured TCP probes, bounded timeouts, explicit result categories, injectable clock/transports for tests.
- [x] **Task 0004 — Foreground monitor:** periodic scheduling, JSON Lines output, graceful cancellation, structured observation output.

**M1 exit:** A user can run monitoring and view observed state/probe results. No claim of root-cause diagnosis.

## M2 — Historical incidents

- [x] **Task 0005 — Local event storage:** versioned SQLite schema, per-user path, transactional cycle writes, bounded retention, and storage failure handling.
- [x] **Task 0006 — Probe incident lifecycle:** configurable consecutive failure/recovery thresholds with conservative handling of unknown outcomes, timestamp gaps, and clock changes.
- [x] **Task 0007 — Incident history CLI:** page stored events through the lifecycle engine and expose incident listings with end-to-end history scenarios.

M1 work was delivered across commits after Task 0001. Task specifications are
kept under `tasks/`; Task 0007 adds a read-only history query over retained
events, without persisting derived incident records.

**M2 exit:** Recorded connectivity failures are queryable as incident timelines.

## M3 — Explainable diagnosis and replay

- [x] **Task 0008 — Conservative analysis core:** deterministic evidence-citing Rust API over current probe and carrier observations; unsupported cases remain undetermined.
- [x] **Task 0010 — Explicit DNS observation:** one opt-in system-configured DNS query per monitoring round, typed versioned events, and cautious DNS-query failure analysis.
- [x] **Task 0009 — Versioned fixture replay:** strict, bounded offline fixtures run through the incident and analysis APIs.
- [x] **Task 0011 — Analysis and report CLI:** expose latest findings and sanitized-by-default text/JSON reports.
- Test false-positive avoidance, uncertainty, source availability, timestamp ordering, and ambiguous signals.

**M3 exit:** A replayed or recorded incident produces a reproducible, evidence-backed report without overstating causality.

## M4 — Reliable public beta

- [x] **Task 0012 — Linux interface deltas:** record passive carrier/speed transitions and RX/TX error/drop counter deltas when sysfs exposes them.
- [x] **Task 0013 — Optional kernel journal signals:** persist normalized link-up/down/watchdog categories from opt-in kernel journal sampling.
- [x] **Task 0014 — Kernel link analysis:** allow correlated link-down/watchdog observations to support existing local-link evidence.
- [x] **Task 0015 — Linux CI:** run formatting, tests, Clippy, and build on pushes and pull requests.
- [x] **Task 0016 — First-run CLI guide:** explain a safe local-first path through the existing commands and the effects of monitoring.

**M4 exit:** An unrelated Linux user can install, monitor, investigate a problem, and share a sanitized report with documented limitations.

## Planned capabilities and release work

This section is the project’s single inventory of unimplemented product capabilities and release work. The PRD and architecture describe intended behavior and design constraints; use this list for their current planning status. Items here are proposals, not assigned implementation tasks. Create a scoped task under `tasks/` before implementation.

### Usability

- Add a concise `status` or overview command showing monitor configuration, the latest collection time, and unavailable signals.
- Add guided setup for probe targets, including a clear explanation that the gateway role labels a user-supplied endpoint and does not verify actual gateway reachability.
- Improve incident browsing with time and target filters, plus comparisons of observations before and after an outage.
- Add finding-specific troubleshooting guidance that clearly separates observed evidence from possible explanations.
- Provide an installation path and practical examples, including sanitized sample reports, so Linux users can try the tool without building it or inventing configuration.

### Core capability and reliability

- Add a user-facing command for offline fixture replay.
- Consider persistence of derived incidents and file export for reports.
- Harden bounded queues and backpressure, retention and size limits, privilege checks, suspend/resume gap reporting, and privacy-safe exports.
- Add further interface evidence rules and richer route-change diagnostics.
- Consider Wi-Fi and VPN-specific correlation and comparative multi-interface incidents.
- Resolve remaining probe destination/rate guidance and interface-selection edge cases when defining their implementation tasks.

### Release preparation

- Add contributor documentation.
- Select and add an open-source license before inviting external reuse or contributions; establish contribution and release policies before release.
- Consider additional distribution packages.

### Separate proposals

- A local dashboard may be considered if it materially improves incident review.
- Cross-platform support and automated remediation require separate proposals.

## Release discipline

At each task completion: explain modifications, test results and test gaps, behavior demonstrations, risks, and the next specific task. No implementation beyond the current assignment without approval.
