# Task 0044 — Separate CLI responsibilities from main.rs

**Status:** Proposed (not assigned)
**Milestone:** Maintainability
**Scope:** Refactor CLI parsing, command dispatch, and command-specific rendering/orchestration out of `src/main.rs` into cohesive Rust modules. Keep one Cargo package and preserve behavior. Do not add commands or features.

## Goal

Make `main.rs` a thin executable entry point and make CLI concerns easier to review independently from core application behavior.

## Requirements

- Inspect current boundaries before moving code; keep pure argument parsing and rendering separable from database, filesystem, and network actions where practical.
- Keep command behavior, help text, exit statuses, output formats, privacy behavior, and error handling unchanged.
- Preserve existing public module/API organization unless a small visibility change is required for the refactor.
- Use idiomatic modules within the existing crate; do not add dependencies or create another crate.
- Avoid unrelated cleanup and do not split code solely to reduce line count.

## Acceptance criteria

- `src/main.rs` is reduced to executable setup and delegation to the CLI layer.
- CLI responsibilities have clear module boundaries with no circular dependency structure.
- Existing deterministic tests and command-level behavior remain valid; add or adjust tests only where needed to protect moved behavior.
- `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build` pass.
- The architecture document's module map is updated if the resulting module responsibilities materially differ.
