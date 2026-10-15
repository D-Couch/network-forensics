# Task 0040 — Clarify architecture documentation

**Status:** Complete
**Milestone:** Maintainability
**Scope:** Reorganize `docs/ARCHITECTURE.md` so implemented behavior, design constraints, and future direction are clearly distinguished. Do not change application behavior or implement roadmap items.

## Goal

Make it straightforward for readers to tell which components and behaviors exist today and which statements guide future work.

## Requirements

- Update the document's opening status to describe the document's current coverage rather than only Tasks 0005–0014.
- Refresh the module map to accurately summarize the current responsibilities of `main.rs`, `monitor.rs`, and the already extracted modules, including `monitor_state` and `report_output`.
- Describe the implemented monitor pipeline, including collection, interface tracking, optional journal sampling, event persistence, and JSON Lines output.
- Separate implemented architecture from design constraints and future direction, or apply consistent labels that make the distinction clear throughout.
- Preserve accurate event, storage, privacy, and analysis constraints; link planned behavior to its roadmap entry when one exists.
- Do not introduce architectural decisions that are unsupported by the code or approved documentation.

## Acceptance criteria

- A reader can identify the implemented components and data flow without relying on task history.
- Future-facing design rules are visibly distinct from descriptions of existing behavior.
- Claims about current module responsibilities match the source tree and CLI behavior.
- Documentation diff is limited to architecture clarity and necessary cross-references.
