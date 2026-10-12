//! Application-level metrics adapter (read-only HTTP scrape).
//!
//! The demo services expose `GET /debug/latency` (loopback only). This adapter
//! fetches it with a dependency-free HTTP/1.0 client, parses the JSON body,
//! and normalizes it into a `MetricKind::Histogram` named `request_latency_ms`.
//! Endpoint absent, empty samples, or malformed bodies yield `None`/errors —
//! the caller records `missing_fields`, never zeros.
use std::time::Duration;

use inspector_core::{CoreError, MetricKind, MetricObservation, Observation, Snapshot};
use serde::{Deserialize, Serialize};

/// Wire shape of the demo `/debug/latency` endpoint (mirrors demo_support).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyEndpointBody {
    pub samples: u64,
    pub sum: f64,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
}

pub const LATENCY_METRIC_NAME: &str = "request_latency_ms";

/// Only loopback HTTP URLs are allowed.
pub fn validate_metrics_url(url: &str) -> Result<(String, u16, String), CoreError> {
    let rest = url.strip_prefix("http://").ok_or_else(|| {
        CoreError::Validation(format!("metrics_url must be http:// (got '{url}')"))
    })?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (host, port) = host_port.rsplit_once(':').ok_or_else(|| {
        CoreError::Validation(format!("metrics_url must include a port (got '{url}')"))
    })?;
    if host != "127.0.0.1" && host != "localhost" && host != "[::1]" {
        return Err(CoreError::Validation(format!(
            "metrics_url host '{host}' not allowlisted (loopback only)"
        )));
    }
    let port: u16 = port
        .parse()
        .map_err(|_| CoreError::Validation(format!("bad port in metrics_url '{url}'")))?;
    Ok((host.to_string(), port, path))
}

/// Fetch and parse the endpoint body with a hard timeout.
pub async fn fetch_latency_body(
    url: &str,
    timeout: Duration,
) -> Result<LatencyEndpointBody, CoreError> {
    let (host, port, path) = validate_metrics_url(url)?;
    let target = format!("{host}:{port}");

    let body = tokio::time::timeout(timeout, async {
        let mut stream = tokio::net::TcpStream::connect(&target)
            .await
            .map_err(|e| CoreError::Unavailable(format!("metrics connect {target}: {e}")))?;
        let req = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        tokio::io::AsyncWriteExt::write_all(&mut stream, req.as_bytes())
            .await
            .map_err(|e| CoreError::Unavailable(format!("metrics request: {e}")))?;
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut buf)
            .await
            .map_err(|e| CoreError::Unavailable(format!("metrics read: {e}")))?;
        Ok::<Vec<u8>, CoreError>(buf)
    })
    .await
    .map_err(|_| CoreError::Timeout {
        after_ms: timeout.as_millis() as u64,
        op: format!("GET {url}"),
    })??;

    let text = String::from_utf8_lossy(&body);
    let (head, json) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| CoreError::Unavailable(format!("malformed HTTP response from {url}")))?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(CoreError::Unavailable(format!(
            "metrics endpoint status '{status}'"
        )));
    }
    serde_json::from_str(json.trim())
        .map_err(|e| CoreError::Unavailable(format!("metrics JSON: {e}")))
}

/// Normalize to a histogram metric. `None` when there are no samples yet.
pub fn to_metric(
    body: &LatencyEndpointBody,
    observed_at_unix_ms: u64,
) -> Option<MetricObservation> {
    if body.samples == 0 {
        return None;
    }
    Some(MetricObservation {
        name: LATENCY_METRIC_NAME.to_string(),
        kind: MetricKind::Histogram {
            count: body.samples,
            sum: body.sum,
            p50: body.p50,
            p95: body.p95,
            p99: body.p99,
        },
        unit: "ms".to_string(),
        observed_at_unix_ms,
    })
}

/// Attach app latency to an existing console observation. On endpoint failure
/// the observation is kept and marked partial — console data stays valid.
pub async fn attach_latency(obs: &mut Observation<Snapshot>, metrics_url: &str, timeout: Duration) {
    match fetch_latency_body(metrics_url, timeout).await {
        Ok(body) => match to_metric(&body, obs.captured_at_unix_ms) {
            Some(m) => {
                if !obs.data.overview.metric_names.contains(&m.name) {
                    obs.data.overview.metric_names.push(m.name.clone());
                }
                obs.data.metrics.push(m);
            }
            None => {
                push_missing(obs, format!("{LATENCY_METRIC_NAME} (no samples yet)"));
            }
        },
        Err(e) => {
            push_missing(obs, format!("{LATENCY_METRIC_NAME} ({e})"));
            obs.completeness
                .warnings
                .push(format!("metrics scrape failed: {e}"));
            obs.completeness.partial = true;
        }
    }
}

fn push_missing(obs: &mut Observation<Snapshot>, field: String) {
    if !obs.completeness.missing_fields.contains(&field) {
        obs.completeness.missing_fields.push(field);
    }
    obs.completeness.partial = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_loopback() {
        assert!(validate_metrics_url("http://127.0.0.1:3000/debug/latency").is_ok());
        assert!(validate_metrics_url("http://example.com:3000/x").is_err());
        assert!(validate_metrics_url("https://127.0.0.1:3000/x").is_err());
        assert!(validate_metrics_url("http://127.0.0.1/x").is_err());
    }

    #[test]
    fn empty_samples_yield_no_metric() {
        let b = LatencyEndpointBody {
            samples: 0,
            sum: 0.0,
            p50: None,
            p95: None,
            p99: None,
        };
        assert!(to_metric(&b, 0).is_none());
    }

    #[tokio::test]
    async fn scrapes_canned_endpoint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 512];
            let _ = s.read(&mut buf).await;
            let body = r#"{"samples":10,"sum":400.0,"p50":40.0,"p95":60.0,"p99":90.0}"#;
            let resp = format!(
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes()).await;
        });
        let url = format!("http://127.0.0.1:{port}/debug/latency");
        let body = fetch_latency_body(&url, Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(body.samples, 10);
        let m = to_metric(&body, 0).unwrap();
        assert!(matches!(m.kind, MetricKind::Histogram { .. }));
    }

    #[tokio::test]
    async fn non_200_is_an_error_not_empty_data() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 512];
            let _ = s.read(&mut buf).await;
            let _ = s.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n").await;
        });
        let url = format!("http://127.0.0.1:{port}/nope");
        assert!(fetch_latency_body(&url, Duration::from_secs(5))
            .await
            .is_err());
    }
}
