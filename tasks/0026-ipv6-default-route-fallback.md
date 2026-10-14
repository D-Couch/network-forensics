# Task 0026 — IPv6 default-route fallback

**Status:** Complete
**Milestone:** M4
**Scope:** Use an active IPv6 default route from `/proc/net/ipv6_route` when no usable IPv4 default route is available. Preserve IPv4 preference when both families have a usable default.

## Decisions

- Require an all-zero destination and source with prefix lengths `/0`, an up route, and no reject flag.
- Select the lowest metric and break ties by interface name, as for IPv4.
- Keep the existing singular default-route interface model; do not infer per-target routing or report both families.
- Do not add dependencies or perform network requests.

## Acceptance criteria

- IPv6-only default routes can identify the default interface.
- More-specific, source-specific, down, and reject routes are ignored.
- IPv4 remains preferred when a usable IPv4 default exists; IPv6 is fallback only.
- Add deterministic parser and discovery tests, update documentation, and run standard Cargo checks.

## Stop condition

Stop after conservative IPv6 fallback, tests, and documentation are complete. Do not add per-target route lookup or change probe behavior.
