# ADR 0005 — Sanitized diagnostic reports

**Status:** Accepted for Task 0011

## Context

Analysis findings currently exist only as a pure API. The PRD calls for readable reports and sanitized JSON sharing, but does not prescribe default CLI format or the boundary for explicitly raw values.

## Decision

- Add read-only `analyze --latest` and `report --latest` commands using retained events in insertion-sequence order.
- `analyze` prints findings; `report` defaults to text and supports JSON.
- Redact target socket addresses by default with stable per-report role-scoped labels. JSON carries an `identifiers_included` marker.
- Include raw configured target addresses only with `--include-identifiers`, and print a stderr warning when that flag is used.
- Keep report content limited to incident summaries and findings. Do not include DNS query names or raw event payloads; retain evidence event IDs to support review against local history.
- Keep report construction and serialization pure. Commands do not modify the database or create report files.

## Consequences

Default reports are suitable for sharing with less risk of disclosing configured target addresses. Event IDs and timestamps remain, so reports may still be correlatable with a local database. The explicit raw mode improves local troubleshooting while making disclosure visible. JSON consumers can distinguish redacted from identifier-inclusive output.

## Alternatives considered

- Always include raw values: rejected because users may share reports without noticing identifying addresses.
- Omit evidence IDs: rejected because findings would be harder to verify against local history.
- Add an export-file workflow: deferred; stdout keeps this task read-only and avoids overwrite/path handling.
