# Product Requirements Document (PRD)

**Project:** Network Forensics (`netforensics`)  
**Status:** Product target; implementation is partial (see `README.md` and `ROADMAP.md`).
**Initial platform:** Linux  
**Primary interface:** command-line interface (CLI)

## 1. Problem

Connectivity failures are easy to experience and difficult to localize. A ping failure alone cannot tell a user whether the problem lies with an Ethernet link, gateway, routing, DNS, upstream connectivity, or a particular endpoint. Existing system signals are spread across tools and logs.

## 2. Product promise

Record relevant, timestamped observations; assemble them into coherent incidents; present the evidence and bounded diagnostic hypotheses in a human-readable report. The application must never portray an inference as an observed fact.

## 3. Users and priority scenarios

1. **Linux desktop user:** Internet briefly stops; tool shows link-carrier transitions, gateway reachability, and upstream probe status around the same period.
2. **Homelab operator:** Intermittent reachability issues; tool preserves incident history for later review.
3. **Support engineer:** Export a report containing relevant event sequence and suggested next tests without sharing sensitive identifiers by default.
4. **Developer/tester:** Replay a known event fixture to verify rules without requiring a live failure.

## 4. Success criteria

- A user can initiate monitoring and later retrieve a timestamped timeline of an outage.
- Reports present evidence separately from interpretations, with unknown/insufficient-evidence outcomes supported.
- Link-carrier loss followed by gateway failure is classified as *local-link-related evidence*, not a definitive bad cable/driver finding.
- DNS-only failure is not mislabeled as loss of all IP connectivity when other probes work.
- Fixtures representing all documented incident rules are deterministically tested.
- Unprivileged default operation on a typical Linux machine, with clear notices when signals are unavailable.
- No network modification, packet payload capture, or unsolicited telemetry.

## 5. Functional requirements

### FR-01 CLI and configuration

The target interface is `netforensics --help`, `--version`, `status`, `monitor`, `incidents`, `analyze --latest`, and `report --latest`. The current implementation provides help/version, `interfaces`, one-shot `probe`, foreground `monitor`, read-only `incidents`, `analyze --latest`, and text/JSON `report --latest`; `status` remains planned. Reports redact configured target addresses by default and require explicit `--include-identifiers` to include them. Task 0001 historically required only help/version. Configuration must have explicit documented defaults, validation, and safe behavior on invalid inputs.

### FR-02 Probe observation

Support a local default-gateway reachability probe and at least two independently configured external reachability targets. For the MVP use an unprivileged probe method (e.g., TCP connect) where appropriate. Distinguish `success`, `failure`, `timeout`, and `unsupported/blocked` rather than equating ICMP failure with Internet outage. DNS lookups must be a separate signal. Task 0010 implements one explicit opt-in `--dns-name` fully qualified query using system nameserver configuration; no lookup occurs without it. Include method, target role, start time, elapsed time, result, and sanitized failure category.

### FR-03 Link/interface observation

Track selected interface identity, operational state, carrier transitions, speed/duplex where available, and pertinent RX/TX error/drop counter deltas. Task 0012 implements passive Linux sysfs observations: monitoring compares carrier/speed and per-field counters with the prior in-memory sample, persists transition/delta events, and treats missing metrics or decreasing counters as unknown rather than synthesizing values or wraparound. Detect unexpected changes, including link-speed downshift. Missing capability is represented explicitly rather than synthesized.

### FR-04 Local evidence

Optionally consume relevant kernel/journald observations with documented permission requirements. This is a later milestone; monitoring must still function when journal access is unavailable.

### FR-05 Incidents

The Task 0006 pure Rust lifecycle API groups stored probe events by exact role and numeric IP:port. Two consecutive timeout, network-unreachable, or host-unreachable outcomes within the default 30-second window open an incident; two consecutive successes recover it. Refused, ambiguous, unsupported, and blocked outcomes do not qualify. Validated configuration controls thresholds and windows, and sequence order plus gap/clock-adjustment markers preserve uncertainty. Task 0007 provides a read-only CLI history query; derived incident persistence remains future work.

### FR-06 Diagnosis

Tasks 0008 and 0010 provide a pure deterministic analysis API over stored TCP, DNS, and carrier observations. It can return local-link-related evidence, configured gateway-role target failure, multiple external targets failed, endpoint-specific evidence supported by another successful external target, DNS-query failure alongside independent external TCP success, or undetermined. Task 0011 exposes it through `analyze --latest` and read-only text/JSON reports. DNS wording reports concurrent query failure evidence and does not establish causation or general reachability. Findings cite event IDs and include explanatory text, limitations, and a follow-up check. The 60-second correlation window is skipped when timestamps move backwards. These labels describe evidence and do not establish root causes; report file export remains future work. No ML/LLM service is required.

### FR-07 Persistence and export

Persist records locally in SQLite, with schema migration strategy, retention options, and explicit handling of write failures. The current implementation persists versioned observation events with indexed metadata and bounded retention; `incidents`, `analyze --latest`, and `report --latest` read the database without modifying it and recompute derived results. Text and versioned JSON reports redact target addresses by default; raw target addresses appear only when explicitly requested and warned. DNS query names and raw event payloads are excluded from reports. Persistence of derived incidents and report files remains future work.

### FR-08 Replay

Task 0009 provides a pure API for deterministic replay of strict, versioned JSON event fixtures through the same incident and analysis logic. Fixture schema version is independent from the event schema; entries carry insertion sequence and must already be ordered. Inputs are limited to 16 MiB and 100000 events, and duplicate IDs, unknown fields, unsupported versions, and invalid sequences are rejected. This API performs no file, database, network, or privileged operations. A user-facing replay command remains future work.

### FR-09 Operational behavior

Gracefully handle SIGINT/shutdown; bound memory and queues; persist without blocking probe collection indefinitely; state monitoring gaps when system sleep, clock adjustments, permission failures, or unavailable signals occur.

## 6. Nonfunctional requirements

- **Safety:** Read-only by default; never change DNS, routes, interfaces, firewall, or driver settings.
- **Privacy:** Local-first; collect minimum necessary metadata; no traffic contents or telemetry.
- **Portability:** Linux-first; kernel capabilities and hardware-specific observations optional with graceful degradation.
- **Reliability:** Avoid unbounded buffers; log collector failures and sampling gaps rather than hiding them.
- **Reproducibility:** Deterministic analysis over recorded events with versioned schemas.
- **Usability:** Plain text CLI output, clear error messages, machine-readable exports.
- **Maintainability:** Distinct observation, analysis, and storage boundaries with automated tests.

## 7. Explicit non-goals (initial releases)

- Packet capture, deep packet inspection, traffic sniffing, or application payload analysis.
- Automatically fixing networks, restarting services, modifying system configuration, or running privileged daemons by default.
- Guaranteeing detection of every failure or proving a unique physical root cause.
- General-purpose intrusion detection or security surveillance.
- Cloud account, web dashboard, mobile clients, Windows/macOS support.
- Machine-learning diagnosis, always-on system tray, enterprise fleet management.

## 8. Proposed default experience

`netforensics monitor` starts foreground collection with concise status; users can stop with Ctrl+C. `netforensics incidents` lists history already stored locally. `netforensics analyze --latest` and `netforensics report --latest` inspect retained observations. If no default gateway exists (VPN-only, offline, unusual routes), explain the limitation and let the user select targets rather than guessing.

## 9. Acceptance examples

- **Link issue:** carrier down precedes gateway/upstream failures; report cites ordered carrier and probe observations and highlights a likely local-link fault domain, without naming a specific physical component.
- **DNS-specific:** DNS lookup fails while independent IP/TCP reachability works; report avoids “Internet down.”
- **Remote endpoint:** one configured endpoint fails while others and gateway pass; report suggests endpoint-specific trouble, not ISP outage.
- **Unavailable signal:** speed unavailable due to device or permissions; report says “unknown,” not 0 Mbps.
- **System sleep:** long sampling gap is reported as missing observations, not as a confirmed outage.

## 10. Open decisions, explicitly deferred

Exact probe destinations and rates, interface-selection edge cases, incident-threshold constants, binary packaging, and open-source license are to be proposed during their respective milestones and recorded before implementation. Event-storage location, retention defaults, and SQLite schema v1 are decided in Task 0005 and `docs/decisions/0001-local-event-storage.md`. Never assume public endpoints are guaranteed responsive or universally reachable.

The repository is already public on GitHub but has no license file. Public visibility does not grant reuse rights; licensing remains an open release decision and should be resolved before inviting external reuse or contributions.
