# Test Strategy

## Principles

- Critical diagnosis logic must be deterministic and run with no network access, privileged APIs, or machine-specific hardware.
- Unit tests assert event/schema behavior; integration tests exercise collector-to-event and event-to-report contracts.
- System-dependent/live tests are optional and clearly marked; CI must not assume network reachability.
- Record expected evidence, classifications, and explicit uncertainty for every simulated incident.

## Routine checks for the current Cargo package

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
```

Run these checks after relevant changes; failures and missing tools must be reported accurately. Task 0001 is complete, and the package now also includes interface, probe, and monitor behavior.

The GitHub Actions Linux workflow runs those four commands on every push and
pull request, and supports manual dispatch. It uses a read-only repository
token and does not run the environment-dependent system smoke checks.

The current probe tests inject both transport outcomes and clock readings. They
must remain network-independent; do not add live endpoint checks to the routine
test suite.

The monitor's round tests use fake transport and clock implementations and
validate the newline-delimited event output. Signal and CLI target validation
should be tested without invoking a real network connection.

Interface collection tests build synthetic sysfs trees for available, missing,
invalid, and zero-valued counters. Tracker tests cover baseline and per-field
deltas, partial unknown values, counter decreases, disappearing/reappearing
interfaces, carrier and speed transitions, and deterministic event order. No
routine test reads host counters or requires network access.

Kernel journal parsing uses synthetic JSON entries and an injected reader.
Tests cover allowlisted normalized categories, ignored messages, invalid
interface names, malformed or oversized output, in-memory cursor progression,
and unavailable access. Routine tests never invoke the host's `journalctl`.

DNS tests use an injected transport and clock; routine tests never query a
resolver. Cover FQDN validation, opt-in CLI parsing, DNS result serialization
without answer addresses, configured DNS event ordering and timeout, schema-v1
history compatibility, and cautious DNS analysis with independent TCP success.

Report tests exercise pure text/JSON rendering, empty history, sequence and
evidence citations, stable target redaction, explicit raw-identifier opt-in,
CLI option validation, and analysis over read-only paged stored events. They
must not open the default database or make network requests.

Status tests use in-memory history to check empty and populated summaries,
explicit unknown interface/counter values, optional-signal absence wording,
insertion ordering, DNS query-name omission, and read-only behavior. They must
not inspect running processes, open the default database, or access the network.

The event store tests use in-memory SQLite or temporary files. They cover
schema migration and rejection, durable reopen and sequence paging, atomic
rollback, age/count retention, path validation, Unix permissions, and symlink
refusal. Tests must not open the default user database or rely on environment
paths.

Incident lifecycle tests use synthetic stored events and the pure analysis API.
They cover per-target separation, qualifying and inconclusive outcomes,
threshold windows, opening and recovery, repeat episodes, insertion-sequence
validation, schema rejection, forward observation gaps, backward timestamps,
and configuration bounds. They must not create network connections or open the
default event database.

Incident history CLI tests use in-memory or temporary SQLite stores. They cover
empty history, rendered evidence and status, continuation across event-page
boundaries, missing-database behavior, and that read-only access cannot write
or alter the schema. They must not open the default user database.

Analysis tests use synthetic ordered event streams and lifecycle incidents.
They cover carrier-down and normalized kernel link-down/watchdog evidence,
neutral link-up signals, out-of-window and backwards-timestamp suppression,
caller-supplied gateway roles, endpoint
specificity with an independent successful target, multiple external failures,
stale evidence, backward timestamps, schema/sequence validation, and explicit
undetermined limitations. They must not access the network or default database.

Replay tests consume human-readable, versioned fixtures under
`tests/fixtures/`, with a separate `expected.json` for each scenario. They run
the same incident and analysis APIs twice to verify deterministic results and
cover supported link/endpoint findings plus undetermined cases. Parser tests
reject malformed or unknown fields, future schemas, bad ordering, duplicate
IDs, inputs over 16 MiB, and event counts over 100000. Replay never accesses
the network or default database.

The routine test suite must remain deterministic and must not invoke live probe
targets. Manual interface inventory can read the host's sysfs/procfs and is
environment-dependent; it is not a substitute for the fixture-based unit tests.

## Test layers

1. **Model and schema:** round-trip serialization, version enforcement, malformed input, invalid/unknown fields policy, unknown-versus-false semantics.
2. **Collection adapters:** mocked netlink/sysfs/filesystem failures, missing permissions, interface disappearance, absent speed, counter resets.
3. **Probes:** injected fake transport/clock, timeouts, refusals, blocked probes, partial target failure, DNS-only failures.
4. **Incident lifecycle:** debounce, overlapping observations, repeats/recovery, gap handling, sleep/wake, clock adjustment, event tie ordering.
5. **Analysis:** deterministic fixtures, evidence references, correct broad fault domain, no unjustified certainty; unknown when insufficient data.
6. **Persistence/export:** schema migration, interrupted writes, large histories, sanitized output, invalid data/version behavior.
7. **System smoke:** local test machine only, opt-in; verify correct warnings when interfaces or permissions unavailable.

## Fixture format

Keep versioned, human-readable JSON fixtures under `tests/fixtures/`. Each
scenario has events and a separate expected-results declaration so test
failures are interpretable. Ordering should be explicit and unaffected by
local timezone. The current envelope uses `fixture_schema_version`, plus
entries containing a stored `sequence` and event-v1 `event` object. Example
scenarios:

- `carrier-loss-then-gateway-failure`
- `dns-only-failure-with-ip-reachability`
- `one-external-target-fails`
- `gateway-unprobeable`
- `interface-speed-downshift`
- `system-suspend-and-observation-gap`
- `ambiguous-multi-layer-failure`
- `events-with-identical-timestamps`

## Definition of done for implementation tasks

- Requirements accepted by focused tests, not merely manual demonstration.
- No warnings under designated clippy command (or explain justified exceptions).
- Relevant docs updated when behavior or interfaces changed.
- No secrets, raw packet payloads, or unsafe default network changes.
- Final summary includes which commands executed, pass/fail outcomes, and untested risks.
