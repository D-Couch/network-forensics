# Task 0010 — Explicit DNS observation

**Status:** Complete
**Milestone:** M3
**Scope:** Add a versioned DNS-result event, an explicit opt-in DNS lookup in foreground monitoring, and DNS-aware evidence wording in the pure analysis API. No DNS history CLI changes, raw answer address storage, direct resolver selection, DNS-over-TLS/HTTPS, or other collectors.

## Goal

Record whether one user-configured fully qualified name resolves through the machine's configured recursive DNS servers, separately from configured TCP endpoint reachability.

## Decisions for this task

- Add event schema v2 and continue reading v1 event envelopes; new events use v2. SQLite schema does not change.
- Add one optional `--dns-name FQDN` to `monitor`. Without it, monitoring sends no DNS queries.
- Require a fully qualified ASCII DNS name ending in `.`; reject search-list names so the query is not expanded with local search domains.
- Use the system recursive nameserver configuration (`/etc/resolv.conf` on Unix), query A and AAAA through the resolver, and bypass the hosts file. Do not choose a public resolver.
- Use the configured monitor timeout, one bounded lookup attempt, and no retry at the application layer. Preserve timeout, NXDOMAIN, no-address-record, resolver-failure, resolver-unavailable, and success outcomes distinctly.
- Persist the configured query name and answer count, but not returned IP addresses. Warn that query names are identifying local data.
- Add Hickory Resolver and Tokio as dependencies for a bounded async DNS resolver; document the addition and system-resolver limitations.
- DNS-related analysis requires a failed DNS lookup and an independent successful external TCP probe in the correlation window. This is concurrent DNS-query failure evidence, not proof that DNS caused the incident.

## Requirements

- Add typed DNS event/outcome models and version-aware event deserialization.
- Keep v1 stored events readable by incidents, analysis, and fixture replay; reject unsupported future event schemas. Only write schema-v2 events.
- Add an injectable DNS transport and clock-backed runner. Unit tests must use fake DNS responses and must not contact a resolver.
- Add validated FQDN parsing and monitor CLI integration. The query runs once per cycle only when explicitly configured and remains time-bounded.
- Extend analysis to cite concurrent DNS failure plus independent TCP success; otherwise preserve existing findings or `Undetermined`.
- Cover DNS outcomes, timeouts, domain validation, schema compatibility, parser behavior, monitor event ordering, privacy-minimized event fields, and DNS-analysis uncertainty deterministically.

## Acceptance checks

```bash
cargo fmt --check
cargo test
cargo clippy -- -D warnings
cargo build
cargo run -- --help
```

## Documentation

Record resolver behavior and limitations in an ADR; update README, PRD, architecture, security/privacy, testing, and roadmap.

## Stop condition

Stop after opt-in bounded DNS observation is recorded locally and its analysis is tested. Do not add background monitoring, additional domains, resolver overrides, or export commands.
