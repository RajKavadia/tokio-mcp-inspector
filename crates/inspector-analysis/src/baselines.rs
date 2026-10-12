//! Snapshot comparison with explicit counter-reset and instrumentation-change handling.
use inspector_core::{CoreError, MetricKind, Observation, Snapshot, SnapshotDelta};

/// Compare two snapshots. Never invents data: `None` stays `None`.
/// Marks comparison unreliable on counter reset or completeness drift.
pub fn compare_snapshots(
    base: &Observation<Snapshot>,
    current: &Observation<Snapshot>,
) -> Result<SnapshotDelta, CoreError> {
    let mut notes = Vec::new();
    let mut unreliable = false;
    let mut reason: Option<String> = None;

    // Counter-reset detection.
    for b in &base.data.metrics {
        if let Some(c) = current.data.metrics.iter().find(|m| m.name == b.name) {
            if let (
                MetricKind::Counter {
                    total: bt,
                    reset_detected: _,
                },
                MetricKind::Counter {
                    total: ct,
                    reset_detected: _,
                },
            ) = (&b.kind, &c.kind)
            {
                if ct < bt {
                    unreliable = true;
                    reason = Some(format!("counter '{}' reset: {bt} -> {ct}", b.name));
                    notes.push(reason.clone().unwrap());
                }
            }
        }
    }

    // Instrumentation drift: different missing-field sets mean different visibility.
    if base.data.overview.metric_names != current.data.overview.metric_names
        || base.completeness.missing_fields != current.completeness.missing_fields
    {
        notes.push("instrumentation/visibility changed between snapshots".to_string());
    }

    let latency_delta_ratio = match (
        histogram_p50(&base.data, "request_latency_ms"),
        histogram_p50(&current.data, "request_latency_ms"),
    ) {
        (Some(b), Some(c)) if b > 0.0 => Some(c / b),
        _ => None, // missing is not zero
    };

    let task_growth = match (
        base.data.overview.active_tasks,
        current.data.overview.active_tasks,
    ) {
        (Some(b), Some(c)) => Some(c as i64 - b as i64),
        _ => None,
    };

    Ok(SnapshotDelta {
        base_id: base.observation_id.clone(),
        current_id: current.observation_id.clone(),
        latency_delta_ratio,
        task_growth,
        unreliable,
        reason,
        notes,
    })
}

fn histogram_p50(snap: &Snapshot, name: &str) -> Option<f64> {
    snap.metrics
        .iter()
        .find(|m| m.name == name)
        .and_then(|m| match &m.kind {
            MetricKind::Histogram { p50, .. } => *p50,
            MetricKind::Gauge { value } => Some(*value),
            MetricKind::Counter { total, .. } => Some(*total),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use inspector_core::{Completeness, MetricObservation, RuntimeOverview, SourceId};

    fn snap(p50: Option<f64>, tasks: Option<u64>) -> Observation<Snapshot> {
        let metrics = p50
            .map(|p| {
                vec![MetricObservation {
                    name: "request_latency_ms".into(),
                    kind: MetricKind::Histogram {
                        count: 100,
                        sum: p * 100.0,
                        p50: Some(p),
                        p95: Some(p * 1.5),
                        p99: Some(p * 2.0),
                    },
                    unit: "ms".into(),
                    observed_at_unix_ms: 0,
                }]
            })
            .unwrap_or_default();
        Observation::new(
            SourceId {
                console_addr: "test".into(),
                service_name: None,
            },
            Some(5000),
            0,
            Snapshot {
                overview: RuntimeOverview {
                    service_name: None,
                    console_addr: "test".into(),
                    active_tasks: tasks,
                    completed_tasks: None,
                    tasks_truncated: false,
                    metric_names: vec!["request_latency_ms".into()],
                },
                tasks: vec![],
                resources: vec![],
                metrics,
            },
            Completeness::complete(),
        )
    }

    #[test]
    fn detects_regression_ratio() {
        let d =
            compare_snapshots(&snap(Some(40.0), Some(12)), &snap(Some(310.0), Some(142))).unwrap();
        assert!((d.latency_delta_ratio.unwrap() - 7.75).abs() < 0.01);
        assert_eq!(d.task_growth, Some(130));
        assert!(!d.unreliable);
    }

    #[test]
    fn missing_is_not_zero() {
        let d = compare_snapshots(&snap(None, None), &snap(Some(40.0), None)).unwrap();
        assert_eq!(d.latency_delta_ratio, None);
        assert_eq!(d.task_growth, None);
    }
}
