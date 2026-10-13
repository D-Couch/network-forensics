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
- Never automatically alter routes, DNS, firewall, drivers, or interface state.

## Sensitive fields and exports

IPs (especially public IPs), local subnets, interface names, MACs, SSIDs, hostnames, usernames, paths, DNS queries and resolver details can identify a device, environment, or organization. Keep only data required to diagnose incidents. Human-readable and machine-readable **share/export** commands must redact or pseudonymize identifying values by default and state which fields were omitted. Allow raw export only through an explicit user request and warning; do not inadvertently expose data in errors or verbose logs.

## Retention and storage

SQLite stays local, using secure per-user paths/permissions. Specify retention and maximum-size controls during M2. Document how to stop collection and delete history; avoid collecting indefinitely in uncontrolled storage. Never silently sync the database.

## Reliability and misuse prevention

Bound concurrency, network probe rates, memory, and stored data; reject oversized or malformed imported fixtures. Treat symlinks and filesystem paths carefully. Ensure report renderers escape arbitrary strings. Avoid shelling out to unsanitized input. When permissions or sources are unavailable, report reduced observability rather than implying normal conditions.

## Evidence integrity

Reports distinguish raw observations, derived incident grouping, and hypotheses. Reference evidence IDs and note missing samples. Do not claim hardware root causes without independent corroboration. Changes to rules and schemas must be versioned so historical reports remain interpretable.
