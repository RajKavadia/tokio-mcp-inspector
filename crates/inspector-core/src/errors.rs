use thiserror::Error;

/// Bounded, structured errors. Timeouts and disconnects never masquerade as empty success.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("timeout after {after_ms}ms waiting for {op}")]
    Timeout { after_ms: u64, op: String },

    #[error("telemetry unavailable: {0}")]
    Unavailable(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("unsupported schema version {0}")]
    Schema(u16),

    #[error("snapshot comparison unreliable: {0}")]
    UnreliableComparison(String),

    #[error("transport: {0}")]
    Transport(String),
}
