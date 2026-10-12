//! Diagnosis regression: deterministic rules separate slow from normal
//! using pinned fixtures. No model, no live telemetry.
use inspector_analysis::{compare_snapshots, detect_task_growth, investigate_latency};
use inspector_core::{FindingStatus, Observation, Snapshot};

fn load(name: &str) -> Observation<Snapshot> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(format!("{path}.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn slow_service_triggers_hypothesis_with_evidence() {
    let base = load("normal_snapshot");
    let current = load("slow_snapshot");

    let delta = compare_snapshots(&base, &current).unwrap();
    assert!((delta.latency_delta_ratio.unwrap() - 7.75).abs() < 0.01);
    assert_eq!(delta.task_growth, Some(130));
    assert!(!delta.unreliable);

    let d = investigate_latency(
        &base.data,
        &current.data,
        &base.observation_id,
        &current.observation_id,
    );
    assert_eq!(d.status, FindingStatus::Hypothesis);
    assert!(d.is_valid());
    assert!(d.evidence.contains(&base.observation_id));

    let g = detect_task_growth(
        &base.data,
        &current.data,
        &base.observation_id,
        &current.observation_id,
    );
    assert_eq!(g.status, FindingStatus::Hypothesis);
}

#[test]
fn normal_vs_normal_is_quiet() {
    let base = load("normal_snapshot");
    let d = investigate_latency(
        &base.data,
        &base.data,
        &base.observation_id,
        &base.observation_id,
    );
    assert_eq!(d.status, FindingStatus::NoIssue);

    let g = detect_task_growth(
        &base.data,
        &base.data,
        &base.observation_id,
        &base.observation_id,
    );
    assert_eq!(g.status, FindingStatus::NoIssue);
}
