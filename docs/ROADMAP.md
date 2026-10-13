# Roadmap

Milestones are sequential targets, not commitments to ship all features immediately. Every milestone must leave the application in a usable, tested state. Implement *one assigned task at a time*.

## M1 — CLI and live observation

- **Task 0001 — Repository foundation:** Cargo package, help/version, one serializable versioned event envelope, tests, formatting/lint/build. No live network work.
- Task 0002 — Interface discovery/state: select interface/default route where possible; report unavailable fields clearly.
- Task 0003 — Probe runner: configured local/external probes, bounded timeouts, explicit result categories, injectable clock/transports for tests.
- Task 0004 — Foreground monitor: periodic scheduling, CLI status output, graceful cancellation, structured observation output.

**M1 exit:** A user can run monitoring and view observed state/probe results. No claim of root-cause diagnosis.

## M2 — Historical incidents

- Choose and document SQLite schema/migration and local file location.
- Persist bounded observation history; handle persistence failures and clean shutdown.
- Introduce configurable incident thresholds, grouping, resolution, and missing-data treatment.
- Implement `incidents` listings; test burst failures, recovery, long gaps, and clock changes.

**M2 exit:** Recorded connectivity failures are queryable as incident timelines.

## M3 — Explainable diagnosis and replay

- Define rules and evidence-citation format for local link, gateway/LAN, DNS, upstream path, endpoint-only, undetermined.
- Reuse analysis engine for live/stored observations and synthetic fixtures.
- Support `analyze --latest`, human-readable and sanitized JSON `report --latest`.
- Test false-positive avoidance, uncertainty, source availability, timestamp ordering, and ambiguous signals.

**M3 exit:** A replayed or recorded incident produces a reproducible, evidence-backed report without overstating causality.

## M4 — Reliable public beta

- Add optional Linux journal correlation and relevant NIC counters where supported.
- Add CLI/TUI overview only if required to make incident review materially easier.
- Harden bounded queues, limits, retention, privilege checks, suspend/resume, and privacy-safe exports.
- CI on Linux, installation/distribution instructions, example sanitized reports, contributor documentation, selected open-source license.

**M4 exit:** An unrelated Linux user can install, monitor, investigate a problem, and share a sanitized report with documented limitations.

## Later, optional

Wi-Fi and VPN-specific correlation, richer route-change and kernel diagnostics, comparative multi-interface incidents, user-defined rule packs, more distribution packages, and optional local dashboards. Treat cross-platform support and automated remediation as separate future proposals.

## Release discipline

At each task completion: explain modifications, test results and test gaps, behavior demonstrations, risks, and the next specific task. No implementation beyond the current assignment without approval.
