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
- [x] **Task 0017 — Read-only status:** summarize retained observations and explicit unknowns without claiming live monitor state or reconstructing unstored configuration.
- [x] **Task 0018 — Finding-specific follow-up guidance:** provide actionable checks tied to existing evidence without adding causal claims.
- [x] **Task 0019 — Offline replay command:** run a strict versioned fixture through the existing analysis pipeline without network or database access.
- [x] **Task 0020 — Guided target setup:** validate user-chosen probe endpoints and explain the gateway-role limitation before printing a monitor command.
- [x] **Task 0021 — Incident history filters and comparison:** filter displayed incidents by time/target and compare same-target probes before and after an outage.
- [x] **Task 0022 — Practical examples and sanitized reports:** provide copyable local workflows and sample report formats.
- [x] **Task 0024 — Probe target and cadence guidance:** explain target permission, one-attempt sequential rounds, and interval behavior.
- [x] **Task 0025 — Validate IPv4 default-route netmask:** ignore zero-destination routes whose netmask is not zero.
- [x] **Task 0026 — IPv6 default-route fallback:** discover an active IPv6 default when no usable IPv4 default exists.
- [x] **Task 0027 — Apply age retention to new events:** discard incoming events older than the configured cutoff.
- [x] **Task 0028 — Preserve incompatible database files:** inspect existing schema versions read-only before changing file permissions.

**M4 exit:** An unrelated Linux user can install, monitor, investigate a problem, and share a sanitized report with documented limitations.

## Planned capabilities and release work

This section is the project’s single inventory of unimplemented product capabilities and release work. The PRD and architecture describe intended behavior and design constraints; use this list for their current planning status. Items here are proposals, not assigned implementation tasks. Their proposal task files capture the question and boundaries; selecting a proposal for implementation requires a separately assigned, sufficiently scoped task.

### Usability

- Extend `status` with active-process and complete monitor-configuration awareness only if a later task explicitly adds safe lifecycle/configuration persistence. ([Task 0029](../tasks/0029-status-lifecycle-configuration.md))
- Prebuilt binaries and package repositories are out of scope; users build from source using the README instructions.

### Core capability and reliability

- Consider persistence of derived incidents and file export for reports. ([Task 0030](../tasks/0030-derived-incident-persistence-and-export.md))
- Harden bounded queues and backpressure, retention and size limits, privilege checks, suspend/resume gap reporting, and privacy-safe exports. ([Task 0031](../tasks/0031-reliability-hardening.md))
- Add further interface evidence rules and richer route-change diagnostics. ([Task 0032](../tasks/0032-interface-and-route-evidence.md))
- Consider Wi-Fi and VPN-specific correlation and comparative multi-interface incidents. ([Task 0033](../tasks/0033-wifi-vpn-and-multi-interface-correlation.md))
- Resolve remaining interface-selection edge cases when defining their implementation tasks. ([Task 0034](../tasks/0034-interface-selection-edge-cases.md))

### Release preparation

- [x] **Task 0023 — Contributor documentation:** document local checks, project boundaries, privacy-aware contributions, and source-only releases.

### Separate proposals

- A local dashboard may be considered if it materially improves incident review. ([Task 0035](../tasks/0035-local-dashboard-proposal.md))
- Cross-platform support requires a separate proposal. ([Task 0036](../tasks/0036-cross-platform-support-proposal.md))
- Automated remediation requires a separate proposal. ([Task 0037](../tasks/0037-automated-remediation-policy-proposal.md))

## Release discipline

At each task completion: explain modifications, test results and test gaps, behavior demonstrations, risks, and the next specific task. No implementation beyond the current assignment without approval.

Project release policy: publish manually as versioned GitHub Releases with brief notes and source only. Do not publish prebuilt binaries or package repositories.
