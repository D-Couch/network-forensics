# Task specifications

This directory contains the scope and acceptance criteria for individual
implementation or planning tasks. Use the [roadmap](../docs/ROADMAP.md) as the
single place to see planned work and its order; this index deliberately does
not repeat the roadmap's task list or statuses.

## Finding a task

Task files use the form `NNNN-short-description.md`, with a zero-padded
number and a short kebab-case name. Numbers identify task specifications;
gaps in the sequence do not imply a missing or unfinished task. The roadmap
links tasks that are part of current planning. To inspect earlier work, browse
the numbered files in this directory.

Each task file defines its own scope and acceptance criteria. Read its linked
requirements and relevant project instructions before implementation.

## Status and lifecycle

Every task file starts with a `**Status:**` field. Use these values:

- `Proposed (not assigned)` — candidate work that has not been selected for
  implementation. Its presence in `tasks/` does not authorize starting it.
- `In progress` — the task has been explicitly selected and work is underway.
- `Complete` — the task's acceptance criteria have been met.
- `Complete (planning)` — the planning or review deliverable is complete, but
  any implementation it proposed is not thereby assigned.

The normal lifecycle is `Proposed (not assigned)` → `In progress` → `Complete`.
Use `Complete (planning)` only when the task itself is a proposal or review.
The roadmap remains authoritative for whether work is planned and how it is
ordered; task status records the lifecycle of that specification's work.
