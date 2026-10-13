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

The current probe tests inject both transport outcomes and clock readings. They
must remain network-independent; do not add live endpoint checks to the routine
test suite.

The monitor's round tests use fake transport and clock implementations and
validate the newline-delimited event output. Signal and CLI target validation
should be tested without invoking a real network connection.

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

Keep versioned, human-readable JSON fixtures under `tests/fixtures/` when replay is implemented. Each scenario has events and a separate expected-results declaration so test failures are interpretable. Ordering should be explicit and unaffected by local timezone. Example future fixture names:

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
