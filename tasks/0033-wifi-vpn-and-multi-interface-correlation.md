# Task 0033 — Wi-Fi, VPN, and multi-interface correlation proposal

**Status:** Proposed (not assigned)
**Milestone:** Future
**Roadmap item:** Consider Wi-Fi and VPN-specific correlation and comparative multi-interface incidents.

## Scope

Assess whether local Wi-Fi, VPN, and multiple-interface observations can support useful incident comparisons while preserving the current conservative analysis model.

## Acceptance criteria for a future implementation task

- Identify candidate signals, source availability, platform and permission requirements, and whether collection is passive.
- Explain how interface identity and route changes affect probe-path attribution; do not assume a probe used a particular interface without evidence.
- Define synthetic scenarios for simultaneous, divergent, missing, and changing interface observations.
- Minimize identifying wireless metadata and state clearly when evidence is insufficient.

## Boundary

This proposal does not add scanning, packet capture, VPN control, or network changes. Implementation requires separate approval and task scope for each signal or correlation rule.
