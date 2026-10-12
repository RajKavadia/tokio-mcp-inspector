use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Connection lifecycle config: explicit timeouts, bounded collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub addr: String,
    pub connect_timeout_ms: u64,
    pub collection_timeout_ms: u64,
    pub max_tasks: usize,
    pub max_resources: usize,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:6669".to_string(),
            connect_timeout_ms: 5000,
            collection_timeout_ms: 8000,
            max_tasks: 200,
            max_resources: 200,
        }
    }
}

impl ConnectionConfig {
    pub fn collection_timeout(&self) -> Duration {
        Duration::from_millis(self.collection_timeout_ms.min(30_000))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectorStatus {
    Connected { addr: String },
    Disconnected { reason: String },
    Degraded { warnings: Vec<String> },
}
