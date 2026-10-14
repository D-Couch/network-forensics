# ADR 0008 — Local report-file export

**Status:** Accepted for Task 0030

## Context

Task 0011 deliberately kept reports on stdout. Users need a simple way to save
a sanitized report locally, while avoiding accidental disclosure, overwrite,
and partial-file behavior.

## Decision

- Keep stdout as the default and add explicit `report --latest --output FILE`.
- Use the existing text or versioned JSON report renderer and its
  sanitized-by-default target handling.
- Cap file output at 16 MiB. Create a private temporary sibling file, write and
  sync the complete report, then create the destination with a no-replace hard
  link. Do not create parent directories.
- Use mode `0600` on Unix. Fail if the destination already exists, including
  when it is a symlink.
- Continue to recompute incidents from retained observation events. Do not
  persist derived incident records until a concrete need justifies the
  invalidation and schema-maintenance costs.

## Consequences

Report files are explicitly requested and are not written over existing paths.
The existing report model controls redaction; raw target addresses remain
opt-in and trigger the current warning. Export is bounded and an incomplete
temporary file is not exposed at the requested destination. Users choose the
path and are responsible for handling the saved file as private local data.

Derived incident state remains consistent with retained observations because
it is recalculated on each query.

## Alternatives considered

- Overwrite the destination: rejected because an existing report or unrelated
  file could be lost.
- Persist derived incidents: deferred because the current deterministic
  analysis is inexpensive and persistence would need invalidation on retention
  and rule changes.
- Export automatically on each report command: rejected because file creation
  should be an explicit user choice.
