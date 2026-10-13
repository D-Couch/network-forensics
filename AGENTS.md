# Codex instructions — Network Forensics

## Read first

Treat `docs/PRD.md` as the source of truth for behavior and scope, `docs/ARCHITECTURE.md` for architecture, `docs/TESTING.md` for verification, and `docs/SECURITY_PRIVACY.md` for safety/privacy. `docs/ROADMAP.md` orders milestones. If requirements conflict, report the conflict rather than guessing; propose a small documented resolution.

## Working contract

- Implement **only the currently assigned task**. Do not preempt future milestones.
- Prefer simple, idiomatic, maintainable Rust and small modules with clear contracts. Do not create a large multi-crate workspace without a concrete need.
- Keep OS observation, active probing, event persistence, incident detection, and explanations independently testable. The analysis engine must accept stored/synthetic events without networking or privileges.
- Distinguish observations from hypotheses. Never attribute a root cause more specifically than evidence permits; preserve unknown states and alternative explanations.
- Use typed event variants, versioned serialization, UTC wall-clock timestamps plus monotonic elapsed time where relevant; never infer reliable event order from wall clocks alone.
- Prefer passive, read-only, unprivileged collection. No auto-remediation, packet payload capture, blanket root requirement, or cloud telemetry.
- Treat probe responses, system logs, paths, and serialized data as untrusted. Avoid logging secrets and minimize personally identifiable metadata.
- Do not silently add network requests, dependencies, elevated permissions, or background services. Explain proposed additions.
- Do not commit, push, publish, configure remotes, or change system/network settings unless expressly requested.

## Implementation protocol

1. Read the assigned task and the relevant requirements.
2. For substantive changes, provide a concise plan and identify ambiguities before modifying code (unless already resolved by the task).
3. Implement the smallest solution meeting acceptance criteria.
4. Add/update deterministic tests, including relevant failure/edge cases.
5. Run applicable checks (`cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`, `cargo build`) once a Cargo project exists. State what was actually run; never claim unrun tests passed.
6. Summarize changed files, behavior, checks/results, deviations, remaining risks, and recommended **next** task; then stop.

## Documentation maintenance

If an approved implementation decision materially alters scope, interface contracts, privacy behavior, or architecture, update the corresponding docs in the same task. Avoid speculative abstractions or unnecessary documentation churn.
