# Task 0016 — First-run CLI guide

**Status:** Complete  
**Milestone:** M4  
**Scope:** Improve README onboarding for the existing CLI. Do not change application behavior, add commands, select probe destinations, or add platform support.

## Goal

Help a new Linux user build the application, inspect local state safely, understand the requirements and effects of starting monitoring, and read collected history.

## Requirements

- Provide a short sequence of existing commands for build, local interface inspection, monitoring help, and history review.
- State that interface inventory is local-only and that monitoring requires two distinct, user-selected numeric external IP:port targets.
- Explain that the optional gateway role is caller-supplied and not verified against routing state.
- Clearly distinguish the repeated TCP connections made by monitoring from optional DNS queries and optional kernel journal access.
- Explain stopping behavior, local storage, and default target redaction for reports.
- Keep the guide within current implemented behavior; do not invent defaults or promise reachability.

## Acceptance checks

- Every example uses an existing command and matches its current CLI syntax.
- The monitoring example is clearly labeled as a template requiring user-selected targets.
- The documentation states which example commands make network requests and which do not.
- No source code, dependency, network setting, or additional collection behavior changes.

## Stop condition

Stop after the README first-run guide is updated and reviewed against the current CLI.
