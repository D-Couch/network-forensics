# ADR 0004 — Explicit system-configured DNS observation

**Status:** Accepted for Task 0010

## Context

The project needs DNS evidence separate from numeric TCP reachability. The current monitor is opt-in, sequential, and bounded; resolver choice, search-list expansion, hosts-file behavior, and storage privacy must remain clear. The operating system resolver API does not provide the same explicit asynchronous timeout control as a dedicated DNS stub resolver.

## Decision

- Add one optional monitor query name. No DNS packets are sent unless the user supplies `--dns-name`.
- Require a fully qualified ASCII domain ending with a dot. Use the system recursive nameserver configuration and do not append search suffixes or consult `/etc/hosts`.
- Use Hickory Resolver with Tokio, a single lookup attempt bounded by the existing monitor timeout, and no hard-coded resolver address. The query checks both A and AAAA records.
- Store the query name and answer count with a typed result; do not store returned addresses. Query names can identify users or organizations and remain in the private local event store under its existing retention rules.
- Bump event envelope to v2 while preserving v1 deserialization. Do not change the SQLite schema.
- Describe concurrent DNS failure plus independent TCP success as DNS-query failure evidence only; do not claim a unique cause.

## Consequences

DNS observation is explicit and testable through an injected transport. System resolver configuration can vary by platform; Hickory's Unix configuration reads `/etc/resolv.conf` and does not implement every host resolver/NSS behavior. Search domains are intentionally excluded by requiring a final dot. A later task may add configured resolver comparison or more system-specific integration if needed.

## Alternatives considered

- libc `getaddrinfo` follows more host resolver behavior but does not expose a reliable per-lookup timeout.
- Hard-coding a public resolver would bypass VPN, enterprise, and local DNS policy and disclose queries to an unsolicited service.
- Storing returned addresses would duplicate endpoint metadata and increase local data exposure without being necessary to detect DNS-query failures.
