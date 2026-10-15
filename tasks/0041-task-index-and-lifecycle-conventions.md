# Task 0041 — Add a task index and lifecycle conventions

**Status:** Complete
**Milestone:** Maintainability
**Scope:** Add navigation and lifecycle guidance for `tasks/`. Keep `docs/ROADMAP.md` as the single inventory and ordering source for planned work; do not create a second status tracker.

## Goal

Help contributors find task specifications and understand how proposals become assigned implementation work.

## Requirements

- Add a concise `tasks/README.md` index describing the filename/numbering convention, task status metadata, and the relationship between task files and the roadmap.
- Explain that the roadmap is authoritative for planning and ordering, while each task file defines its own scope and acceptance criteria.
- Define a small consistent status vocabulary for task files, including completed implementation, proposed/unassigned work, and any explicitly assigned or in-progress work if used by the project.
- Provide useful navigation to task specifications without repeating the roadmap's descriptions or maintaining a duplicate task-status list.
- Review existing task headers and make only the minimal updates needed to align their status metadata with the documented convention.

## Acceptance criteria

- A new contributor can locate task specifications and understand how a proposal is selected and assigned.
- Task status terminology and formatting are clear and consistent with the documented lifecycle.
- Planned work remains listed and ordered only in `docs/ROADMAP.md`.
- No implementation scope or roadmap priority changes as part of this task.
