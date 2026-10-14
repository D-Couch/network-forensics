# Task 0030 — Local report-file export

**Status:** Complete
**Milestone:** M4
**Roadmap item:** Consider persistence of derived incidents and file export for reports.

## Decisions

- Continue to recompute derived incidents from retained observation events. Persisting a second derived representation would require invalidation when retention or analysis rules change; no measured need justifies that additional state yet.
- Add explicit `report --latest --output FILE` for a caller-selected local path. Keep stdout as the default.
- Create output only when the destination does not exist. Write a complete bounded report through a private temporary file, then link it into place without replacing files or following destination symlinks.
- Limit file output to 16 MiB; use mode `0600` on Unix. Do not create parent directories.
- Preserve sanitized-by-default output and the explicit warning for `--include-identifiers`. Continue to omit DNS query names and raw event payloads.

## Acceptance criteria

- The report CLI accepts one `--output FILE`; without it, the report is written to stdout.
- Text and JSON output use the existing report model and file output is capped at 16 MiB.
- Existing files are never overwritten; successful Unix exports have mode `0600`, and a partial report is not visible before completion.
- Tests cover option parsing, output bounds, no-overwrite behavior, temporary-file cleanup, and Unix permissions.
- README, architecture, privacy, testing, PRD, and ADR documentation describe the output and decision to keep derived incidents ephemeral.
- Run standard Cargo checks and `cargo run -- --help`.

## Boundary

Do not persist derived incidents, change the SQLite schema, or change report identifier/privacy policy.
