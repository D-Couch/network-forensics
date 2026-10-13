# Security and Privacy Policy (Design)

## Trust boundaries

The tool receives data from the local OS, active network probes, optional system logs, local configuration, and imported replay files. All of these can contain untrusted or identifying information. Diagnostic reports can be forwarded to third parties, increasing disclosure risk.

## Collection policy

- Collect network *metadata* needed to establish fault domains; never capture packet payloads in the initial product.
- Do not read credentials, browser histories, cookies, application traffic, or unrelated logs.
- Active probes must be rate-limited, time-bounded, documented, and user-configurable; avoid treating blocked responses as definitive outages.
- The current one-shot probe command connects only to a numeric IP and port supplied directly by the user, applies a bounded timeout, and performs no DNS lookup or retry. It sends no probe unless explicitly invoked.
- Foreground monitoring applies the same restrictions, requires two distinct external targets, runs them sequentially, and pauses at least one second after each round. Ctrl+C stops new attempts while the current attempt completes within its configured timeout.
- Monitoring should not require root by default; optional privileged capabilities need explicit opt-in and a reason.
- Do not silently upload diagnostics or enable telemetry. Work offline except for user-configured diagnostic probes.
- The replay API parses in-memory JSON fixture input only, enforces a 16 MiB/100000-event limit, rejects unknown fields and unsupported schemas, and performs no networking or database writes.
- The foreground monitor persists observations locally and emits them to stdout. The `incidents` command reads this history locally without modifying the database. No runtime telemetry or cloud sync is implemented.
- Interface monitoring reads only the available `rx_errors`, `tx_errors`, `rx_dropped`, and `tx_dropped` sysfs counters. It retains the previous sample in memory and stores per-round deltas, not cumulative values. Missing metrics remain unknown; decreases indicate a possible reset and are not treated as wraparound. These reads require no elevated privileges.
- DNS queries are sent only when the user supplies one `--dns-name`; the name must be fully qualified and the lookup is bounded by the configured monitor timeout. System-configured recursive DNS servers are used; the application does not select a public resolver and bypasses hosts-file entries. DNS query names are stored locally because they can identify a person, organization, or internal service; returned IP addresses are not stored. Users should treat this field as private local data.
- `analyze --latest` and `report --latest` open retained history read-only. Reports omit raw event payloads and DNS query names, and replace target addresses with stable role-scoped labels by default. `--include-identifiers` explicitly includes configured target addresses and emits a warning on stderr; event IDs and timestamps remain in either mode.
- Never automatically alter routes, DNS, firewall, drivers, or interface state.

## Sensitive fields and exports

IPs (especially public IPs), local subnets, interface names, MACs, SSIDs, hostnames, usernames, paths, DNS queries and resolver details can identify a device, environment, or organization. Keep only data required to diagnose incidents. Human-readable and machine-readable **share/export** commands must redact or pseudonymize identifying values by default and state which fields were omitted. Allow raw export only through an explicit user request and warning; do not inadvertently expose data in errors or verbose logs.

## Retention and storage

SQLite event storage is implemented for monitor observations. It stays at the per-user XDG data location or `$HOME/.local/share/netforensics/events.sqlite3`, is never synced by the application, and uses `0700` directory and `0600` database permissions on Unix. Retention defaults to 30 days or 100000 events, whichever limit is reached first, with a 256 MiB database cap. Stop the monitor and delete `events.sqlite3` to clear stored history. The database contains identifying network metadata such as IP addresses and interface names; handle it as private local data.

## Reliability and misuse prevention

Bound concurrency, network probe rates, memory, and stored data; reject oversized or malformed imported fixtures. Treat symlinks and filesystem paths carefully. Ensure report renderers escape arbitrary strings. Avoid shelling out to unsanitized input. When permissions or sources are unavailable, report reduced observability rather than implying normal conditions.

## Evidence integrity

Reports distinguish raw observations, derived incident grouping, and hypotheses. Reference evidence IDs and note missing samples. Do not claim hardware root causes without independent corroboration. Changes to rules and schemas must be versioned so historical reports remain interpretable.
