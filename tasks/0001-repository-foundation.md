# Task 0001 — Rust repository foundation

**Status:** Complete
**Milestone:** M1  
**Scope:** One task only — no live monitoring

## Goal

Create a compiling, tested Rust CLI foundation with a versioned, serializable event model. Do not implement network collectors, probing, SQLite, incident grouping, analysis, system services, or visual interfaces in this task.

## Required context

Before changes, read `AGENTS.md`, `docs/PRD.md`, `docs/ARCHITECTURE.md`, and `docs/TESTING.md`. Check existing files first; preserve this documentation.

## Deliverables

1. Initialize one Rust Cargo package with binary name `netforensics` (single package, not speculative multi-crate workspace).
2. Implement `netforensics --help` and `netforensics --version` using conventional CLI behavior. The help should describe the project honestly without implying unfinished commands work.
3. Implement a small, typed event model in a separate module with an explicit `schema_version`, unique identifier or clearly documented placeholder identifier semantics, UTC observation timestamp, source, and at least one typed variant for link state changes. Use a stable, documented Serde JSON representation.
4. Add tests that serialize/deserialize the event, validate `schema_version` behavior (e.g., explicit rejection for incompatible future versions), and distinguish `carrier=false` from unavailable carrier state.
5. Update `README.md` with actual build/run/test instructions **only after** they work; keep future features marked as planned.
6. Add `.gitignore` suitable for Rust and other minimal project essentials. Do not add CI, packaging, services, or unrelated infrastructure yet.

## Constraints

- No live probing, DNS lookups, network configuration changes, packet capture, SQLite, root requirements, or background network/service processes.
- Do not scaffold empty abstractions for future milestones.
- Select small, commonly used dependencies where justified; list and explain them in the final summary.
- Do not change the product's privacy/architecture commitments or expand task scope.
- Do not commit, push, publish, or configure a Git remote.

## Acceptance criteria

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy -- -D warnings
cargo run -- --help
cargo run -- --version
```

All commands must succeed where Rust tooling is present; if an environment tool is missing, report it rather than claiming success. The versioned event representation and round-trip/unknown tests must pass. Final report: files changed, architectural choices, dependencies, exact checks/results, known limitations, and proposed Task 0002 scope.

**Stop after Task 0001.**
