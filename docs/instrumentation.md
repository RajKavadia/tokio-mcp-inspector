# Instrumentation

Pinned baseline: `tokio 1.44` (features `full`, `tracing`),
`console-subscriber 0.5.0`, `console-api 0.9.0`, Rust stable.

1. `.cargo/config.toml` must contain:
   ```toml
   [build]
   rustflags = ["--cfg", "tokio_unstable"]
   ```
   Without it you get: `task tracing requires Tokio to be built with RUSTFLAGS="--cfg tokio_unstable"`.
2. Instrument at the top of `main`:
   ```rust
   console_subscriber::Builder::default()
       .server_addr(([127, 0, 0, 1], 6669))
       .init();
   ```
   (`init()` enables the `tokio=trace,runtime=trace` filter automatically.)
3. Name tasks: `tokio::task::Builder::new().name("wait").spawn(...)` — unnamed
   tasks appear with `task.name` absent (recorded in `missing_fields`, never invented).
4. Verify with the reference CLI: `cargo install --locked tokio-console`,
   then `tokio-console http://127.0.0.1:6669`.

Version notes: task instrumentation needs Tokio ≥1.0; wakers ≥1.7; `block_on` ≥1.12;
`tokio::time` resources ≥1.13; `tokio::sync` resources ≥1.15. Use latest stable
for fullest telemetry. Alternating builds with/without the cfg flag forces full rebuilds.
