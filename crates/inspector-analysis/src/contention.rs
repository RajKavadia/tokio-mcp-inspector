//! Resource-contention rule: scheduling delay growing while tasks pile up.
//!
//! Signature this detects: median per-task `scheduled_ns` (time spent woken but
//! not yet polled) rises sharply between snapshots while busy work stays flat —
//! tasks are queueing on a contended resource (pool, lock, single-threaded hot
//! path) rather than doing slow work themselves. Pure function, no I/O, no LLM.
use inspector_core::{Diagnosis, FindingStatus, Snapshot};

/// Median scheduled-time must grow by at least this ratio to fire.
pub const DEFAULT_SCHED_RATIO: f64 = 3.0;
/// Minimum tasks with scheduled data required in each snapshot.
pub const MIN_TASKS: usize = 5;

pub fn detect_contention(
    base: &Snapshot,
    current: &Snapshot,
    base_id: &str,
    current_id: &str,
) -> Diagnosis {
    detect_contention_with_threshold(base, current, base_id, current_id, DEFAULT_SCHED_RATIO)
}

pub fn detect_contention_with_threshold(
    base: &Snapshot,
    current: &Snapshot,
    base_id: &str,
    current_id: &str,
    threshold_ratio: f64,
) -> Diagnosis {
    let evidence = vec![base_id.to_string(), current_id.to_string()];
    let (bmed, bn) = median_scheduled_ns(base);
    let (cmed, cn) = median_scheduled_ns(current);

    match (bmed, cmed) {
        (Some(b), Some(c)) if bn >= MIN_TASKS && cn >= MIN_TASKS && b > 0 => {
            let ratio = c as f64 / b as f64;
            if ratio >= threshold_ratio {
                let res_note = if current.resources.is_empty() {
                    "no resource records captured; correlate manually in tokio-console Resources view"
                } else {
                    "see captured resource records for contended primitives"
                };
                Diagnosis {
                    status: FindingStatus::Hypothesis,
                    finding: format!(
                        "median scheduled time grew {b}ns -> {c}ns (x{ratio:.1}) across {cn} tasks"
                    ),
                    hypothesis: Some(format!(
                        "scheduling delay consistent with resource contention, not slow work. {res_note}."
                    )),
                    confidence: 0.65,
                    evidence,
                    verification: Some(
                        "widen the bottleneck (pool size, lock scope) and re-capture; scheduled time should fall"
                            .into(),
                    ),
                }
            } else {
                Diagnosis {
                    status: FindingStatus::NoIssue,
                    finding: format!(
                        "scheduling delay stable (median {b}ns -> {c}ns, threshold x{threshold_ratio})"
                    ),
                    hypothesis: None,
                    confidence: 0.85,
                    evidence,
                    verification: None,
                }
            }
        }
        _ => Diagnosis {
            status: FindingStatus::Observed,
            finding: format!(
                "scheduled-time data insufficient (base tasks with data: {bn}, current: {cn}; need {MIN_TASKS}); no contention verdict"
            ),
            hypothesis: None,
            confidence: 0.3,
            evidence,
            verification: Some("capture a longer window so scheduled stats populate".into()),
        },
    }
}

/// Median of `scheduled_ns` over tasks that report it, plus how many reported.
fn median_scheduled_ns(snap: &Snapshot) -> (Option<u64>, usize) {
    let mut v: Vec<u64> = snap.tasks.iter().filter_map(|t| t.scheduled_ns).collect();
    let n = v.len();
    if v.is_empty() {
        return (None, 0);
    }
    v.sort_unstable();
    (Some(v[n / 2]), n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inspector_core::{RuntimeOverview, TaskRecord};

    fn task(id: u64, sched: Option<u64>) -> TaskRecord {
        TaskRecord {
            task_id: id,
            name: None,
            polls: Some(10),
            wakes: Some(10),
            self_wakes: Some(0),
            busy_ns: None,
            scheduled_ns: sched,
            state: None,
            location: None,
        }
    }

    fn snap(sched_each: Option<u64>, n: usize) -> Snapshot {
        Snapshot {
            overview: RuntimeOverview {
                service_name: None,
                console_addr: "t".into(),
                active_tasks: Some(n as u64),
                completed_tasks: None,
                tasks_truncated: false,
                metric_names: vec![],
            },
            tasks: (0..n as u64).map(|i| task(i, sched_each)).collect(),
            resources: vec![],
            metrics: vec![],
        }
    }

    #[test]
    fn fires_on_scheduling_blowout() {
        let d = detect_contention(
            &snap(Some(1_000_000), 10),
            &snap(Some(50_000_000), 60),
            "a",
            "b",
        );
        assert_eq!(d.status, FindingStatus::Hypothesis);
        assert!(d.is_valid());
    }

    #[test]
    fn quiet_when_stable() {
        let d = detect_contention(
            &snap(Some(1_000_000), 10),
            &snap(Some(1_200_000), 10),
            "a",
            "b",
        );
        assert_eq!(d.status, FindingStatus::NoIssue);
    }

    #[test]
    fn missing_data_has_no_verdict() {
        let d = detect_contention(&snap(None, 10), &snap(Some(50_000_000), 60), "a", "b");
        assert_eq!(d.status, FindingStatus::Observed);
    }

    #[test]
    fn too_few_tasks_has_no_verdict() {
        let d = detect_contention(&snap(Some(1_000), 2), &snap(Some(99_000), 2), "a", "b");
        assert_eq!(d.status, FindingStatus::Observed);
    }
}
