# Architecture

AI agent → MCP server (rmcp stdio) → Collector (console-api gRPC) → Analysis (pure rules).

- `inspector-core`: versioned `Observation<T>`, `MetricKind` (Counter/Gauge/Histogram),
  `TaskRecord`/`ResourceRecord`, `Diagnosis` with evidence IDs. No tokio, no tonic, no rmcp.
- `inspector-collector`: external `ConsoleCollector` (tonic `watch_updates`),
  bounded windows, reconnect per capture, `Completeness` tracks every absent field.
  `metrics_adapter` scrapes loopback `/debug/latency` (dependency-free HTTP) into a
  `Histogram`; scrape failures mark the snapshot partial, never empty.
- `inspector-analysis`: `compare_snapshots` (counter-reset + drift detection),
  `investigate_latency` (ratio rule, default x1.5, min 30 samples),
  `detect_task_growth` (+50 threshold), `detect_contention` (median scheduled
  time x3 across ≥5 tasks), `validate_diagnosis` (evidence required).
- `inspector-mcp`: 6 read-only tools, loopback-only addresses, clamped limits/timeouts.

Layers are independently testable: analysis runs on JSON fixtures, collector runs
without MCP or a model, MCP contract tests run without a live console.
