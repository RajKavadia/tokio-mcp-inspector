//! Deterministic latency rule. No LLM involved.
use inspector_core::{Diagnosis, FindingStatus, MetricKind, Snapshot};

/// Threshold: current p50 must exceed `threshold_ratio * baseline p50`
/// with at least `min_samples` observations. Tuned from Experiment C data.
pub const DEFAULT_THRESHOLD_RATIO: f64 = 1.5;
pub const MIN_SAMPLES: u64 = 30;

pub fn investigate_latency(
    base: &Snapshot,
    current: &Snapshot,
    base_id: &str,
    current_id: &str,
) -> Diagnosis {
    investigate_latency_with_threshold(base, current, base_id, current_id, DEFAULT_THRESHOLD_RATIO)
}

pub fn investigate_latency_with_threshold(
    base: &Snapshot,
    current: &Snapshot,
    base_id: &str,
    current_id: &str,
    threshold_ratio: f64,
) -> Diagnosis {
    let evidence = vec![base_id.to_string(), current_id.to_string()];
    let (bp50, bcount) = latency_p50_count(base);
    let (cp50, ccount) = latency_p50_count(current);

    match (bp50, cp50) {
        (Some(b), Some(c)) if b > 0.0 => {
            if ccount < MIN_SAMPLES {
                return Diagnosis {
                    status: FindingStatus::Observed,
                    finding: format!(
                        "insufficient samples for verdict (current count {ccount} < {MIN_SAMPLES})"
                    ),
                    hypothesis: None,
                    confidence: 0.4,
                    evidence,
                    verification: Some("capture a longer window, then re-compare".into()),
                };
            }
            let ratio = c / b;
            let _ = bcount;
            if ratio >= threshold_ratio {
                Diagnosis {
                    status: FindingStatus::Hypothesis,
                    finding: format!("request latency rose {b:.0}ms -> {c:.0}ms (x{ratio:.1})"),
                    hypothesis: Some(
                        "increase may be associated with resource contention or blocking (see task/resource evidence)".into(),
                    ),
                    confidence: 0.72,
                    evidence,
                    verification: Some(
                        "reproduce with controlled workload; remove suspected bottleneck and re-capture".into(),
                    ),
                }
            } else {
                Diagnosis {
                    status: FindingStatus::NoIssue,
                    finding: format!(
                        "no latency regression (ratio x{ratio:.2}, threshold x{threshold_ratio})"
                    ),
                    hypothesis: None,
                    confidence: 0.9,
                    evidence,
                    verification: None,
                }
            }
        }
        _ => Diagnosis {
            status: FindingStatus::Observed,
            finding: "latency data missing in one or both snapshots; no verdict possible".into(),
            hypothesis: None,
            confidence: 0.3,
            evidence,
            verification: Some(
                "check completeness.missing_fields for histogram availability".into(),
            ),
        },
    }
}

fn latency_p50_count(snap: &Snapshot) -> (Option<f64>, u64) {
    for m in &snap.metrics {
        if m.name == "request_latency_ms" {
            if let MetricKind::Histogram { p50, count, .. } = &m.kind {
                return (*p50, *count);
            }
        }
    }
    (None, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inspector_core::{MetricObservation, RuntimeOverview};

    fn snap_with(p50: Option<f64>, count: u64) -> Snapshot {
        Snapshot {
            overview: RuntimeOverview {
                service_name: None,
                console_addr: "t".into(),
                active_tasks: Some(10),
                completed_tasks: None,
                tasks_truncated: false,
                metric_names: vec![],
            },
            tasks: vec![],
            resources: vec![],
            metrics: p50
                .map(|p| {
                    vec![MetricObservation {
                        name: "request_latency_ms".into(),
                        kind: MetricKind::Histogram {
                            count,
                            sum: p * count as f64,
                            p50: Some(p),
                            p95: None,
                            p99: None,
                        },
                        unit: "ms".into(),
                        observed_at_unix_ms: 0,
                    }]
                })
                .unwrap_or_default(),
        }
    }

    #[test]
    fn fires_on_regression() {
        let d = investigate_latency(
            &snap_with(Some(40.0), 100),
            &snap_with(Some(310.0), 100),
            "a",
            "b",
        );
        assert_eq!(d.status, FindingStatus::Hypothesis);
        assert!(d.is_valid());
    }

    #[test]
    fn quiet_on_normal() {
        let d = investigate_latency(
            &snap_with(Some(40.0), 100),
            &snap_with(Some(42.0), 100),
            "a",
            "b",
        );
        assert_eq!(d.status, FindingStatus::NoIssue);
    }

    #[test]
    fn missing_has_no_verdict() {
        let d = investigate_latency(&snap_with(None, 0), &snap_with(Some(40.0), 100), "a", "b");
        assert_eq!(d.status, FindingStatus::Observed);
    }
}
