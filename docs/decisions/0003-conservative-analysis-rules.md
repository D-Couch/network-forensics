# ADR 0003 — Conservative evidence-based analysis rules

**Status:** Accepted for Task 0008

## Context

The current event model contains TCP probe results and link-carrier observations. These can support bounded correlations, but the gateway/external roles are caller-supplied and there are no DNS events, selected-route identity on each probe, or independent path measurements. Analysis must not turn these gaps into root-cause claims.

## Decision

- Implement a pure Rust analysis API over ordered stored events and lifecycle incidents; return one deterministic finding per incident with evidence IDs, explanation, limitations, and a follow-up check.
- Use a validated 60-second UTC correlation window. Insertion sequence remains authoritative for event order. If stored timestamps move backward, skip cross-event timestamp correlations.
- A nearby carrier-down observation may be described only as local-link-related evidence; do not infer which interface carries the probe path or name a physical cause.
- Describe gateway-role incidents as failures of the configured gateway-role target. The role is not verified against the active route.
- Describe failures at multiple external targets as multi-target failure evidence, without asserting a common upstream cause.
- Describe an external incident as endpoint-specific only when another external target succeeds and has no qualifying failure in the same window.
- Leave unsupported or ambiguous cases undetermined. DNS-specific findings wait for a DNS observation event.

## Consequences

The engine can summarize current evidence without I/O, networking, dependencies, or privileges. It deliberately returns uncertainty when observations do not distinguish fault domains. New event sources can support additional rules later without changing current interpretations.

## Alternatives considered

- Assigning a fault domain to every incident would imply certainty unsupported by the event model.
- Treating caller-provided target roles as verified topology would misstate the configured probe.
- Correlating across backward wall-clock jumps could associate unrelated events; skipping those correlations preserves uncertainty.
