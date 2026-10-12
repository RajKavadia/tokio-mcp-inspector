# Security

- **Read-only by default.** The MCP surface is exactly the six tools in
  `permissions::READ_ONLY_TOOLS`. No shell, no code execution, no target writes.
  Adding a write tool requires editing that list and this review.
- **Loopback only.** `validate_console_addr` rejects non-loopback targets so the
  server cannot be pointed at arbitrary production hosts.
- **Bounded.** Limits clamped to [1, 200], timeouts to [1000, 30000]ms.
  Snapshot store holds at most 16 observations (in-memory, evicts oldest).
- **Minimal retention.** Raw `tracing` events are never exported; only aggregated
  task/resource records. No auth tokens or request bodies are collected.
- **No secrets in fixtures.** `tests/fixtures/*.json` are synthetic.
- Report vulnerabilities via the repo issue tracker; do not file them against
  instrumented production targets.
