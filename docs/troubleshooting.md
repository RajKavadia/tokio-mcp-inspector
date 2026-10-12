# Troubleshooting

- **Collector timeout**: target not started or wrong port. Each example uses its own
  port (6669/6670/6671). Error is `CoreError::Timeout`, never an empty success.
- **Empty task list**: window too short or service idle. Re-capture with larger
  `timeout_ms` (max 30000) or generate load (`curl localhost:3000/work`).
- **`task.busy_ns` missing**: expected — recorded in `completeness.missing_fields`.
  The current wire mapping does not decode busy histograms; rules use polls/wakes/scheduled.
- **Counter reset / unreliable delta**: target restarted between snapshots.
  `compare_snapshots` marks `unreliable: true` with a reason — take a fresh baseline.
- **Non-loopback address rejected**: MVP allowlists `127.0.0.1:`, `localhost:`, `[::1]:` only.
- **MCP client can't find server**: stdio transport — check the absolute binary path
  in client config; logs go to stderr (stdout is the protocol).
