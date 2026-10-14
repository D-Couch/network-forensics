# Contributing

Thanks for your interest in Network Forensics. The project is a Linux-first,
local network diagnostics tool. Contributions should preserve its conservative
evidence handling, privacy, and read-only behavior.

## Development

Install a current Rust toolchain with Cargo, clone the repository, and run the
test suite and standard checks before submitting a change:

```sh
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
```

Tests should be deterministic and must not probe live network targets. Prefer
fixtures, synthetic events, and injected transports or clocks. Update the
relevant documentation when a change affects behavior, interfaces, privacy,
or architecture.

## Project boundaries

- Keep observations distinct from hypotheses; preserve unknown states and
  limitations rather than claiming a root cause the evidence cannot establish.
- Prefer passive, read-only, unprivileged collection. Do not add automatic
  remediation, packet payload capture, cloud telemetry, or background services.
- Do not silently add network requests, dependencies, elevated permissions, or
  system changes; explain proposed additions.
- Treat local events, fixture data, paths, and logs as potentially sensitive
  or untrusted. Keep test data synthetic and redact identifying information
  before sharing diagnostics.

## Changes and reports

Keep changes focused. In a pull request, describe the user-visible effect,
relevant design choices, checks run, and any known limitations. Do not include
unredacted reports, local database files, real target addresses, or private DNS
names in issues or pull requests.

## Releases

Releases are manual, versioned GitHub Releases with brief notes and source
only. The project does not publish prebuilt binaries or package repositories.
