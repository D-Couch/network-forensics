# Task 0011 — Analysis and report CLI

**Status:** Complete
**Milestone:** M3
**Scope:** Expose the existing incident-analysis API as `analyze --latest` and add read-only text/JSON reports. No changes to diagnosis rules, event collection, report persistence, raw export beyond an explicit identifier flag, or future collectors.

## Goal

Let users inspect conservative findings from local event history and create a shareable report that protects identifying network metadata by default.

## Decisions for this task

- `analyze --latest` prints one human-readable finding per derived incident, or a clear no-incidents message.
- `report --latest` defaults to human-readable text and accepts `--format text|json`.
- Reports redact target IP:port values by default, replacing each distinct target consistently with `gateway-target-N` or `external-target-N`. The JSON report states whether identifiers were included.
- `--include-identifiers` explicitly includes raw configured target addresses and emits a warning to stderr. DNS query names, event IDs, and unrelated event payloads are not added to the report.
- Both commands read all retained events through the existing read-only paged API, preserve sequence order, and perform no network or database writes.
- Report construction/rendering is pure and testable with synthetic events. JSON has a versioned report envelope.

## Acceptance checks

- Add deterministic report model/render tests for empty history, multiple incidents, redaction consistency, raw opt-in, JSON validity, and citations/limitations.
- Test CLI argument parsing, unknown formats/options, and that the raw identifier flag is explicit.
- Verify stored v1/v2 event compatibility and read-only database behavior through existing storage tests/APIs.
- `analyze --latest`, `report --latest`, and `report --latest --format json` produce useful output without writing to the database.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, `cargo build`, and `cargo run -- --help`.

## Documentation

Record the redaction/format decision in an ADR; update README, PRD, architecture, testing, security/privacy, and roadmap.

## Stop condition

Stop after the analysis and report commands render existing stored evidence as specified. Do not add analysis rules, persistence, monitoring, or report export files.
