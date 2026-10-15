# Product Requirements Document (PRD)

**Project:** Network Forensics (`netforensics`)  
**Status:** Product requirements and design target. Supported user-visible behavior is documented in the [README](../README.md); planned and completed work is tracked in the [roadmap](ROADMAP.md).
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

Provide discoverable help and version output and a CLI for local interface inspection, explicit target setup and probing, foreground monitoring, incident history, analysis and reports, and offline fixture replay. Configuration must have explicit documented defaults, validation, and safe behavior on invalid inputs. Guided setup must validate user-entered endpoints, explain that a gateway-role target is only a caller-supplied label, and print a monitor command for review without probing or starting monitoring. Status must distinguish an active monitor's configuration from a saved last-run configuration that may be stale, and must not display the configured DNS query name. Reports must redact target addresses by default and require an explicit option, with a warning, to include them. The README documents the commands and options currently available.

### FR-02 Probe observation

Support a probe for a user-supplied gateway-role endpoint and at least two independently configured external reachability targets on distinct IP addresses. Use an unprivileged method such as TCP connect where appropriate. Distinguish success, failure, timeout, and unsupported/blocked results rather than equating one probe failure with an Internet outage. Users choose permitted targets; do not supply public defaults or discover targets automatically. Attempt each configured TCP target once per round, sequentially. The configured interval is a pause after the round, so time between attempts to the same target also includes round duration. DNS must be a separate opt-in signal: query only a user-supplied fully qualified name, using system resolver configuration, and perform no lookup when it is unset. Include method, target role, start time, elapsed time, result, and sanitized failure category.

### FR-03 Link/interface observation

Passively observe the selected interface's identity, operational state, carrier transitions, speed/duplex where available, and pertinent RX/TX error/drop counter deltas. Identify a usable default route when possible, preferring IPv4 and using IPv6 when no usable IPv4 default exists. Detect relevant changes such as a link-speed downshift. Missing capabilities remain explicitly unknown; a decreasing counter is not treated as a valid delta unless wraparound can be established. Bound collection memory without silently truncating observations. The README describes current user-visible interface behavior.

### FR-04 Local evidence

Kernel journal sampling, when enabled, must be limited to relevant link messages and store normalized categories rather than raw message text. Use the invoking user's existing access without requesting elevation. Unavailable journal access reduces observability but must not stop monitoring.

### FR-05 Incidents

Group probe events by exact role and numeric IP:port. Two consecutive timeout, network-unreachable, or host-unreachable outcomes within the default 30-second window open an incident; two consecutive successes recover it. Refused, ambiguous, unsupported, and blocked outcomes do not qualify. Validated configuration controls thresholds and windows, and sequence order plus gap/clock-adjustment markers preserve uncertainty. The read-only history query must support inclusive incident-start time filters, exact IP:port filtering, and optional same-target comparison before an incident and after recovery. Detection always uses the full retained sequence before display filtering. Derived incident records remain ephemeral and are recomputed from retained observation events.

### FR-06 Diagnosis

Analysis must be deterministic over stored TCP, DNS, carrier, and normalized kernel-link observations. Nearby carrier-down, kernel-link-down, or watchdog evidence may support a local-link-related finding; it does not identify cause or prove the signal affected the probe path. Link-up and unrelated kernel messages are neutral. Findings may describe a configured gateway-role target failure, multiple external targets failing, endpoint-specific evidence supported by another successful external target, DNS-query failure alongside independent external TCP success, or an undetermined result. DNS wording reports concurrent query-failure evidence and must not claim causation or general reachability. Findings cite event IDs and include an explanation, limitations, and a follow-up check. Skip timestamp correlations when timestamps move backwards. Labels describe evidence and do not establish root causes. No ML/LLM service is required.

### FR-07 Persistence and export

Persist versioned observation records locally in SQLite, with schema migration, retention, and explicit handling of write failures. Discard incoming events older than the configured age cutoff in the same transaction. History, analysis, and report commands must read stored events without modifying them and recompute derived results. A report may be saved to a bounded local file without overwriting an existing path. Reports redact target addresses by default; include raw target addresses only when explicitly requested with a warning. Exclude DNS query names and raw event payloads from reports. Do not persist derived incidents: they duplicate deterministic results and require invalidation when observations are retained or analysis rules change.

### FR-08 Replay

Provide deterministic replay of strict, versioned JSON event fixtures through the same incident and analysis logic. The CLI reads one user-supplied local file with a 16 MiB cap and prints derived incidents and findings. Fixture schema version is independent from event schema version; entries carry insertion sequence and must already be ordered. Inputs are limited to 16 MiB and 100000 events; reject duplicate IDs, unknown fields, unsupported versions, and invalid sequences. Replay performs no networking, database access, or privileged operations.

### FR-09 Operational behavior

Gracefully handle SIGINT/shutdown; bound memory and queues; and prevent persistence from blocking probe collection indefinitely. Report detected monitoring gaps while preserving uncertainty about their cause. Use Linux `CLOCK_BOOTTIME` for inter-round gap measurements so suspend time is included and wall-clock adjustments do not affect elapsed time. Do not infer a gap when the elapsed clock is unavailable or inconsistent, and do not claim that a detected gap was caused by sleep or network failure.

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

## 10. Open decisions

Never assume public endpoints are guaranteed responsive or universally reachable. See [`CONTRIBUTING.md`](../CONTRIBUTING.md) for the release policy.
