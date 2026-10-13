# Test Strategy

## Principles

- Critical diagnosis logic must be deterministic and run with no network access, privileged APIs, or machine-specific hardware.
- Unit tests assert event/schema behavior; integration tests exercise collector-to-event and event-to-report contracts.
- System-dependent/live tests are optional and clearly marked; CI must not assume network reachability.
- Record expected evidence, classifications, and explicit uncertainty for every simulated incident.

## Routine checks after Task 0001

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
```

The commands should be executed where available; failures and missing tools must be reported verbatim. No tests are claimed to have run before code exists.

The current probe tests inject both transport outcomes and clock readings. They
must remain network-independent; do not add live endpoint checks to the routine
test suite.

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
