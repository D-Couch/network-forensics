# Task 0025 — Validate IPv4 default-route netmask

**Status:** Complete
**Milestone:** M4
**Scope:** Ensure `/proc/net/route` selection recognizes only an active IPv4 `0.0.0.0/0` route as default.

## Acceptance criteria

- A route with destination `00000000` but a nonzero mask is not selected as the default route.
- Existing active-route metric ordering and deterministic tie breaking remain unchanged.
- Add a regression test for split-default `/1` routes and retain existing parser/discovery coverage.
- Update the architecture description and run the standard Cargo checks.

## Stop condition

Stop after the parser, regression test, and documentation are updated and verified. Do not add IPv6 route parsing or change interface-selection policy beyond the IPv4 `/0` validation.
