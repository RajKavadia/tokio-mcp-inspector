use serde::{Deserialize, Serialize};

use super::schema_version::SCHEMA_VERSION;

/// Versioned observation envelope. Every collector output uses this.
/// Missing is `None`, never zero. Timeouts are errors, never empty `Ok`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation<T> {
    pub schema_version: u16,
    pub observation_id: String,
    /// Wall clock, for correlation with logs. NOT for duration math.
    pub captured_at_unix_ms: u64,
    /// Milliseconds since collector start (monotonic). Use for deltas.
    pub monotonic_elapsed_ms: u64,
    pub collection_interval_ms: Option<u64>,
    pub source: SourceId,
    pub completeness: Completeness,
    pub data: T,
}

impl<T> Observation<T> {
    pub fn new(
        source: SourceId,
        interval_ms: Option<u64>,
        monotonic_elapsed_ms: u64,
        data: T,
        completeness: Completeness,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            observation_id: uuid::Uuid::new_v4().to_string(),
            captured_at_unix_ms: chrono::Utc::now().timestamp_millis() as u64,
            monotonic_elapsed_ms,
            collection_interval_ms: interval_ms,
            source,
            completeness,
            data,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceId {
    pub console_addr: String,
    pub service_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completeness {
    pub partial: bool,
    pub missing_fields: Vec<String>,
    pub warnings: Vec<String>,
}

impl Completeness {
    pub fn complete() -> Self {
        Self {
            partial: false,
            missing_fields: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn partial(missing_fields: Vec<String>, warnings: Vec<String>) -> Self {
        Self {
            partial: true,
            missing_fields,
            warnings,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeOverview {
    pub service_name: Option<String>,
    pub console_addr: String,
    pub active_tasks: Option<u64>,
    pub completed_tasks: Option<u64>,
    pub tasks_truncated: bool,
    pub metric_names: Vec<String>,
}

/// Metric semantics matter: counters reset, gauges fluctuate, histograms sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum MetricKind {
    Counter {
        total: f64,
        reset_detected: bool,
    },
    Gauge {
        value: f64,
    },
    Histogram {
        count: u64,
        sum: f64,
        p50: Option<f64>,
        p95: Option<f64>,
        p99: Option<f64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricObservation {
    pub name: String,
    pub kind: MetricKind,
    pub unit: String,
    pub observed_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: u64,
    pub name: Option<String>,
    pub polls: Option<u64>,
    pub wakes: Option<u64>,
    pub self_wakes: Option<u64>,
    pub busy_ns: Option<u64>,
    pub scheduled_ns: Option<u64>,
    pub state: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRecord {
    pub resource_id: u64,
    pub concrete_type: Option<String>,
    pub kind: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub overview: RuntimeOverview,
    pub tasks: Vec<TaskRecord>,
    pub resources: Vec<ResourceRecord>,
    pub metrics: Vec<MetricObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDelta {
    pub base_id: String,
    pub current_id: String,
    pub latency_delta_ratio: Option<f64>,
    pub task_growth: Option<i64>,
    pub unreliable: bool,
    pub reason: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingStatus {
    Observed,
    Hypothesis,
    Verified,
    NoIssue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnosis {
    pub status: FindingStatus,
    pub finding: String,
    pub hypothesis: Option<String>,
    /// 0.0..=1.0. Never 1.0 unless `Verified` via controlled A/B.
    pub confidence: f32,
    /// Observation IDs backing this finding. Empty = invalid.
    pub evidence: Vec<String>,
    pub verification: Option<String>,
}

impl Diagnosis {
    pub fn is_valid(&self) -> bool {
        !self.evidence.is_empty() && (0.0..=1.0).contains(&self.confidence)
    }
}
