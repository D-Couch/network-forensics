# Task 0039 — Bound per-cycle monitor events

**Status:** Complete
**Milestone:** Future reliability
**Roadmap item:** Bound collection memory before storage batch validation.

## Problem

The monitor stores a cycle in a `Vec<Event>` before calling the storage API.
Storage rejects batches larger than 4096 events, but interface inventory
collection currently has no corresponding limit, so a sufficiently large
inventory can allocate an oversized cycle before that rejection.

## Scope

- Define a clear maximum number of interface observations/events retained in
  one monitoring cycle, consistent with the storage batch limit.
- Bound collection before allocating or appending beyond that limit.
- Preserve deterministic event order and report that interface observations
  were limited; do not silently present a partial inventory as complete.
- Keep the monitor synchronous; do not add a queue, worker thread, or
  backpressure framework.

## Acceptance criteria

- A deterministic test injects an inventory larger than the configured cycle
  limit and proves memory/event production stays within the limit.
- The result clearly represents omitted interface observations or fails the
  cycle with a concise, safe error; it must not silently truncate.
- A normal inventory still produces the same event order and persisted
  observation behavior.
- Storage still rejects oversized direct batches, and transactional rollback
  behavior is unchanged.
- No dependency, privilege, network request, or background service is added.
- Update architecture and testing documentation with the selected limit and
  behavior.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and
  `cargo build`.

## Boundary

Do not add asynchronous collection, queues, retries, or a general event-loss
framework. Do not claim full interface coverage when the configured limit has
been reached.
