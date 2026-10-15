# Task 0043 — Clarify PRD status references

**Status:** Proposed (not assigned)
**Milestone:** Maintainability
**Scope:** Reduce unnecessary duplication of implementation status in `docs/PRD.md` while preserving requirements and constraints. Do not change product scope or make the roadmap a source of requirements.

## Goal

Keep the PRD focused on what the product should do and the constraints it must satisfy, while pointing readers to the right current-state references.

## Requirements

- Review implementation-status detail embedded in PRD requirements and distinguish it from normative requirements, product boundaries, and acceptance criteria.
- Preserve requirements needed to interpret the product's intended behavior, including privacy, uncertainty, and safety constraints.
- Where implementation status is useful context, link to the README for user-visible current behavior and the roadmap for planned/completed work.
- Do not turn the README or roadmap into authoritative sources for product requirements.
- Avoid broad rewriting unrelated to duplicated status statements.

## Acceptance criteria

- PRD requirements remain actionable and understandable without treating implementation status as a requirement.
- Current user-visible behavior and planning status have clear references to their designated documents.
- No requirement, safety boundary, or acceptance criterion is removed merely to reduce duplication.
- Changes are limited to status-reference clarity and directly necessary wording.
