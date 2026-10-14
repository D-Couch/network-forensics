# Task 0031 — Reliability hardening proposal

**Status:** Complete (planning)
**Milestone:** Future
**Roadmap item:** Harden bounded queues and backpressure, retention and size limits, privilege checks, suspend/resume gap reporting, and privacy-safe exports.

## Findings and disposition

This proposal was completed as a reliability review. It found two concrete
implementation gaps, tracked separately in [Task 0038](0038-monitoring-gap-events.md)
and [Task 0039](0039-bound-monitor-cycle-events.md). The other areas already
have bounded behavior or do not have the architecture that would justify an
implementation change:

### Queueing and backpressure

- **Current behavior:** Monitoring collects one cycle synchronously, stores it
  in one transaction, then writes it to stdout. There is no producer queue or
  concurrent consumer. Configured targets are capped at eight and storage
  rejects batches over 4096 events, but interface enumeration is not capped
  before events are accumulated in memory.
- **Failure scenario:** An unusually large interface inventory can grow the
  cycle vector beyond the storage batch limit before the write rejects it. A
  slow or failed database write delays sampling or stops monitoring before
  that cycle is printed. There is no accumulating asynchronous backlog.
- **Expected behavior:** Bound per-cycle interface events before they grow
  without limit, preserve visible failure/unknown information, and keep
  synchronous storage unless concurrency or a demonstrated queued backlog is
  introduced.
- **Verification:** Existing monitor/storage tests cover transactional
  rollback and storage batch rejection. Add deterministic oversized-inventory
  coverage with the bounded-cycle implementation.
- **Disposition:** Accepted as [Task 0039](0039-bound-monitor-cycle-events.md).

### Retention and database limits

- **Current behavior:** Age and count retention run transactionally; stale
  incoming events are discarded at insertion. SQLite's maximum page count
  bounds the database to 256 MiB. Database and report output sizes are capped.
- **Failure scenario:** Once the database reaches its configured cap, a write
  can fail and stop monitoring rather than growing storage without bound.
- **Expected behavior:** Keep storage bounded and report write failure; do not
  silently remove more history than the configured age/count policy allows.
- **Verification:** Storage tests cover retention boundaries, maximum page
  configuration, unsupported files, and rollback. Report export tests cover
  its 16 MiB cap.
- **Disposition:** No new implementation task.

### Privilege use

- **Current behavior:** Interface collection reads procfs/sysfs without
  elevation. Optional kernel journal sampling uses the caller's permissions,
  is explicitly enabled, and degrades with a warning when unavailable. No
  privileged API or root requirement is present.
- **Failure scenario:** A user without journal access receives fewer signals;
  monitoring continues. No component attempts to elevate privileges.
- **Expected behavior:** Preserve unprivileged operation and graceful loss of
  optional signals; any future privileged collector needs a separately scoped
  justification and explicit opt-in.
- **Verification:** Injected journal-reader tests cover unavailable access;
  collection tests do not invoke host journal or privileged APIs.
- **Disposition:** No new implementation task.

### Suspend/resume and observation gaps

- **Current behavior:** Incident analysis notices excessive or backwards
  timestamp spacing between probe events and marks uncertainty on open
  incidents. Monitoring does not emit a typed event explaining a sampling
  gap, and wall-clock timestamps alone cannot reliably distinguish suspend
  from a clock adjustment.
- **Failure scenario:** After suspend, a history reader sees missing samples
  but cannot identify the gap as a monitor observation gap; a wall-clock jump
  may also look like elapsed time.
- **Expected behavior:** Emit a bounded typed gap event when the monitor can
  detect a gap, preserve unknown causes, and use injectable elapsed-time
  sources for deterministic tests. Do not label a gap as suspend unless a
  platform signal supports that conclusion.
- **Verification:** Inject clock readings for normal rounds, long gaps,
  backwards wall time, and resume-like elapsed-time jumps; verify event order,
  serialization compatibility, and conservative incident/report wording.
- **Disposition:** Accepted as [Task 0038](0038-monitoring-gap-events.md).

### Report export privacy

- **Current behavior:** Reports redact target addresses by default, disclose
  that redaction in text, require an explicit warning-producing opt-in for raw
  addresses, and omit raw event payloads and DNS query names by construction.
  File exports are capped, private on Unix, and refuse overwrite.
- **Failure scenario:** A user explicitly opts into identifiers or shares the
  private local export; documentation already warns that identifiers remain
  sensitive. The default export does not include the reviewed sensitive
  fields.
- **Expected behavior:** Keep sanitized output as the default and preserve the
  explicit raw-identifier opt-in and private-file protections.
- **Verification:** Report rendering tests assert target redaction and
  identifier opt-in; file-output tests cover size, permissions, and
  no-overwrite behavior.
- **Disposition:** No new implementation task; existing controls meet this
  proposal's scope.

## Boundary

This review authorizes no runtime implementation, privilege increase, new
background service, or collection change. Tasks 0038 and 0039 are separate
proposals and must be assigned before implementation.
