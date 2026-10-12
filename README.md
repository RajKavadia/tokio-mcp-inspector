# Tokio MCP Inspector

Read-only runtime investigation for instrumented Tokio services, exposed over MCP.

- `examples/`: good case (`normal_service` :3000/6669) vs worst cases
  (`slow_service` :3001/6670, `resource_contention` :3002/6671).
- `crates/inspector-core`: versioned `Observation<T>`, metric semantics, evidence-gated `Diagnosis`.
- `crates/inspector-collector`: external Console gRPC collector (live-verified).
- `crates/inspector-analysis`: deterministic latency / task-growth rules + comparison.
- `crates/inspector-mcp`: 6 read-only MCP tools (rmcp stdio).

## Quickstart

```powershell
cargo build --workspace
cargo test --workspace
cargo run -p normal_service   # console on 127.0.0.1:6669
cargo run -p inspector-mcp    # MCP over stdio (logs to stderr)
```

Live collector proof: `cargo test -p inspector-collector -- --ignored --nocapture`.
Docs: `docs/`, `examples/README.md`. Security model: `SECURITY.md`.
