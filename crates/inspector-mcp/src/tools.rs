//! Tool parameter schemas (JSON Schema via rmcp's schemars) and shared state.
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use inspector_core::{Observation, Snapshot};
use rmcp::schemars;
use rmcp::serde::{Deserialize, Serialize};

/// In-memory snapshot store so `compare_snapshots` can reference prior captures.
/// Bounded: evicts oldest beyond MAX_STORED.
#[derive(Clone, Default)]
pub struct SnapshotStore {
    inner: Arc<Mutex<HashMap<String, Observation<Snapshot>>>>,
}

impl SnapshotStore {
    pub const MAX_STORED: usize = 16;

    pub async fn put(&self, obs: Observation<Snapshot>) -> String {
        let id = obs.observation_id.clone();
        let mut g = self.inner.lock().await;
        if g.len() >= Self::MAX_STORED {
            if let Some(oldest) = g.keys().next().cloned() {
                g.remove(&oldest);
            }
        }
        g.insert(id.clone(), obs);
        id
    }

    pub async fn get(&self, id: &str) -> Option<Observation<Snapshot>> {
        self.inner.lock().await.get(id).cloned()
    }

    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CommonParams {
    /// Console gRPC address. Loopback only in MVP.
    #[serde(default = "default_addr")]
    pub console_addr: String,
    /// Per-call timeout override (ms). Clamped to [1000, 30000].
    pub timeout_ms: Option<u64>,
}

fn default_addr() -> String {
    "127.0.0.1:6669".to_string()
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ListParams {
    #[serde(flatten)]
    pub common: CommonParams,
    /// Max items returned. Clamped to [1, 200].
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CaptureParams {
    #[serde(flatten)]
    pub common: CommonParams,
    pub service_name: Option<String>,
    pub limit: Option<usize>,
    /// Optional app latency endpoint, e.g. "http://127.0.0.1:3000/debug/latency".
    /// Loopback only. When present, its histogram is merged into the snapshot.
    pub metrics_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CompareParams {
    pub base_id: String,
    pub current_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct InvestigateParams {
    pub base_id: String,
    pub current_id: String,
    /// Override for the latency ratio threshold (default 1.5).
    pub threshold_ratio: Option<f64>,
}
