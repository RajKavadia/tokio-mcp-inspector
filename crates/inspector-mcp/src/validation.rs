//! Input validation and output limits. Every tool is bounded.
use inspector_core::CoreError;

pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;
pub const MAX_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 200;

/// Clamp a client-supplied limit into [1, MAX_LIMIT].
pub fn clamp_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Clamp a client-supplied timeout into [1000, MAX_TIMEOUT_MS].
pub fn clamp_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(1_000, MAX_TIMEOUT_MS)
}

/// Only loopback targets are allowed in the MVP. Rejects anything else
/// so the server cannot be pointed at arbitrary production hosts.
pub fn validate_console_addr(addr: &str) -> Result<String, CoreError> {
    let addr = addr.trim();
    if addr.starts_with("127.0.0.1:")
        || addr.starts_with("localhost:")
        || addr.starts_with("[::1]:")
    {
        Ok(addr.to_string())
    } else {
        Err(CoreError::Validation(format!(
            "console_addr '{addr}' not allowlisted (MVP allows loopback only)"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_limits() {
        assert_eq!(clamp_limit(None), 50);
        assert_eq!(clamp_limit(Some(1000)), 200);
    }

    #[test]
    fn rejects_non_loopback() {
        assert!(validate_console_addr("127.0.0.1:6669").is_ok());
        assert!(validate_console_addr("prod-db:6669").is_err());
    }
}
