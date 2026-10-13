# Task 0015 — Linux continuous integration

**Status:** Complete
**Milestone:** M4
**Scope:** Add one GitHub Actions Linux job that runs the repository's standard Rust checks on pushes, pull requests, and manual dispatch. Do not add release, publish, deployment, or coverage jobs.

## Goal

Run the same formatting, test, lint, and build checks used locally for every contribution.

## Decisions for this task

- Use GitHub-hosted Ubuntu, stable Rust, and only the `clippy` and `rustfmt` components.
- Pin third-party actions to immutable commit references and disable optional action caching.
- Grant the workflow read-only repository contents permission. No secrets or write permissions are needed.
- Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, and `cargo build` as separate steps.

## Acceptance checks

- The workflow triggers on `push`, `pull_request`, and `workflow_dispatch`.
- Workflow permissions are limited to `contents: read`.
- The four standard commands are present and fail the job when they fail.
- No repository token, secret, release, deployment, or publication step is added.
- Validate workflow YAML and run the same four commands locally.

## Documentation

Update the roadmap, testing guide, and README contribution guidance.

## Stop condition

Stop after Linux CI is configured to verify the standard checks. Do not add more platforms or release automation.
