//! Task-growth rule: distinguishes leaks/contention pile-up from plain slowness.
use inspector_core::{Diagnosis, FindingStatus, Snapshot};

pub const GROWTH_THRESHOLD: i64 = 50;

pub fn detect_task_growth(
    base: &Snapshot,
    current: &Snapshot,
    base_id: &str,
    current_id: &str,
) -> Diagnosis {
    let evidence = vec![base_id.to_string(), current_id.to_string()];
    match (base.overview.active_tasks, current.overview.active_tasks) {
        (Some(b), Some(c)) => {
            let growth = c as i64 - b as i64;
            if growth >= GROWTH_THRESHOLD {
                Diagnosis {
                    status: FindingStatus::Hypothesis,
                    finding: format!("active tasks grew {b} -> {c} (+{growth})"),
                    hypothesis: Some(
                        "growth may indicate spawn leak or contention pile-up; correlate with scheduled-time".into(),
                    ),
                    confidence: 0.65,
                    evidence,
                    verification: Some(
                        "capture task list sorted by polls; check for unbounded spawners".into(),
                    ),
                }
            } else {
                Diagnosis {
                    status: FindingStatus::NoIssue,
                    finding: format!("task count stable ({b} -> {c})"),
                    hypothesis: None,
                    confidence: 0.85,
                    evidence,
                    verification: None,
                }
            }
        }
        _ => Diagnosis {
            status: FindingStatus::Observed,
            finding: "task counts unavailable; no growth verdict".into(),
            hypothesis: None,
            confidence: 0.3,
            evidence,
            verification: Some("check completeness.missing_fields".into()),
        },
    }
}
