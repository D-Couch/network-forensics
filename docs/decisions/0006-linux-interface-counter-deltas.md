# ADR 0006 — Linux interface counter deltas

**Status:** Accepted for Task 0012

## Context

The monitor already reads carrier and speed snapshots from sysfs, but does not preserve transitions. Linux exposes cumulative interface error/drop counters. Persisting cumulative values increases retained data and requires consumers to infer reset behavior.

## Decision

- Read only the four RX/TX error/drop counters under each observed interface's sysfs statistics directory.
- Keep the prior sample in memory and persist per-cycle deltas. A baseline event establishes the first sample without claiming a delta.
- Represent each unavailable metric as `null`. If a counter decreases, mark a reset as observed and leave that metric's delta unknown; do not infer wraparound or driver reset.
- Persist carrier transitions only when both values are known and differ. Persist speed changes with optional before/after values to distinguish disappearance of a measurement from a zero speed.
- Write new observations as event schema v3, while accepting v1/v2 stored events. Keep the SQLite schema unchanged.

## Consequences

The monitor retains only per-interface baselines in memory and continues to work when sysfs attributes are absent. Counter deltas cannot distinguish a hardware wrap from a driver/device reset; they preserve that uncertainty. They are counts over a sampling interval, not calibrated rates or proof of packet loss affecting a specific application.

## Alternatives considered

- Store cumulative raw counters: rejected because they retain more data and shift reset semantics to later consumers.
- Infer wraparound: rejected because counter width and driver behavior are not reliably known from these files.
- Require all four counters: rejected because drivers can expose a partial set.
- Add journald or route events in this task: deferred to separate tasks with independent permissions and parsing contracts.
