# Task 0042 — Improve README navigation

**Status:** Complete
**Milestone:** Maintainability
**Scope:** Improve navigation in `README.md` for first-time users and maintainers. Do not change application behavior or remove accurate usage, safety, or privacy information.

## Goal

Let a new Linux user find a safe first-run path quickly while preserving the README's detailed command and behavior reference.

## Requirements

- Put or retain a concise quick-start path near the top that links to the detailed build and usage instructions.
- Clearly distinguish user-facing usage guidance from contributor and maintainer references.
- Keep the key safety caveats near the relevant actions: targets are user supplied, monitoring makes configured connections, and optional DNS/journal collection is opt-in.
- Reduce repeated command or privacy explanations where a concise summary can link to a detailed section.
- Preserve links to project documentation and useful operational detail.

## Acceptance criteria

- A reader can quickly find build instructions, safe initial commands, and the monitoring example.
- User guidance and contributor references are easy to distinguish.
- No implemented capability, limitation, or material safety/privacy caveat is lost or contradicted.
- README structure remains concise and does not duplicate the roadmap's planned-work inventory.
