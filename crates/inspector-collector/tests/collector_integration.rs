//! Live collector integration (Experiment A/C).
//! Spawns the instrumented demo service as a SEPARATE process, connects
//! externally, and asserts a bounded observation arrives.
//! Run with: cargo test -p inspector-collector -- --ignored --nocapture
//! Requires the example binary built (cargo build --workspace).
use inspector_collector::{ConnectionConfig, ConsoleCollector};
use std::time::Duration;

fn binary(name: &str) -> std::path::PathBuf {
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop(); // crates/inspector-collector -> crates
    dir.pop(); // crates -> workspace root
    dir.join("target").join("debug").join(format!("{name}.exe"))
}

#[tokio::test]
#[ignore = "needs a live instrumented service; run explicitly"]
async fn external_process_yields_real_observation() {
    let exe = binary("normal_service");
    assert!(exe.exists(), "build workspace first: {exe:?} missing");

    let mut child = tokio::process::Command::new(&exe)
        .env("RUST_LOG", "info")
        .env("CONSOLE_PORT", "6689")
        .env("HTTP_PORT", "3089")
        .kill_on_drop(true)
        .spawn()
        .expect("spawn normal_service");

    // Give console-subscriber time to bind.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let collector = ConsoleCollector::new(ConnectionConfig {
        addr: "127.0.0.1:6689".to_string(),
        connect_timeout_ms: 5000,
        collection_timeout_ms: 8000,
        max_tasks: 50,
        max_resources: 50,
    });
    let obs = collector
        .capture_snapshot(Some("normal_service".into()))
        .await;
    child.kill().await.ok();

    let obs = obs.expect("collector must produce a real observation");
    assert_eq!(obs.schema_version, inspector_core::SCHEMA_VERSION);
    assert!(obs.data.overview.active_tasks.is_some());
    // Unsupported fields are absent, not zero — and declared as such.
    assert!(obs.data.tasks.iter().all(|t| t.task_id != u64::MAX));
}

/// End-to-end app latency (Experiment C): drive `/work`, scrape
/// `/debug/latency`, and prove a real histogram attaches to the snapshot.
#[tokio::test]
#[ignore = "needs a live instrumented service; run explicitly"]
async fn work_drives_real_latency_histogram() {
    use inspector_collector::metrics_adapter::{attach_latency, LATENCY_METRIC_NAME};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn get(host: &str, port: u16, path: &str) {
        let mut s = tokio::net::TcpStream::connect(format!("{host}:{port}"))
            .await
            .expect("connect demo http");
        let req = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).await.expect("write");
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.expect("read");
        let text = String::from_utf8_lossy(&buf);
        assert!(
            text.contains(" 200 "),
            "expected 200 for {path}: {text:.120}"
        );
    }

    let exe = binary("normal_service");
    assert!(exe.exists(), "build workspace first: {exe:?} missing");

    let mut child = tokio::process::Command::new(&exe)
        .env("RUST_LOG", "warn")
        .env("CONSOLE_PORT", "6679")
        .env("HTTP_PORT", "3079")
        .kill_on_drop(true)
        .spawn()
        .expect("spawn normal_service");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Generate load so the histogram has samples.
    for _ in 0..5 {
        get("127.0.0.1", 3079, "/work").await;
    }

    let collector = ConsoleCollector::new(ConnectionConfig {
        addr: "127.0.0.1:6679".to_string(),
        connect_timeout_ms: 5000,
        collection_timeout_ms: 8000,
        max_tasks: 50,
        max_resources: 50,
    });
    let mut obs = collector
        .capture_snapshot(Some("normal_service".into()))
        .await
        .expect("console capture");
    attach_latency(
        &mut obs,
        "http://127.0.0.1:3079/debug/latency",
        Duration::from_secs(5),
    )
    .await;
    child.kill().await.ok();

    let m = obs
        .data
        .metrics
        .iter()
        .find(|m| m.name == LATENCY_METRIC_NAME)
        .expect("latency histogram must attach after load");
    match &m.kind {
        inspector_core::MetricKind::Histogram { count, p50, .. } => {
            assert!(*count >= 5, "expected >=5 samples, got {count}");
            let p50 = p50.expect("p50 present with samples");
            assert!(
                (20.0..=500.0).contains(&p50),
                "healthy p50 should be ~40ms, got {p50}"
            );
        }
        other => panic!("expected histogram, got {other:?}"),
    }
    assert!(obs
        .data
        .overview
        .metric_names
        .contains(&LATENCY_METRIC_NAME.to_string()));
}
