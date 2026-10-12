//! MCP contract: tool surface stays small, read-only, and bounded.
//! Runs without a live console or model.
use inspector_mcp::{permissions::READ_ONLY_TOOLS, validation};

#[test]
fn tool_surface_is_the_frozen_mvp_set() {
    assert_eq!(
        READ_ONLY_TOOLS,
        &[
            "get_runtime_overview",
            "list_tasks",
            "list_resources",
            "capture_snapshot",
            "compare_snapshots",
            "investigate_latency",
        ]
    );
}

#[test]
fn bounds_are_enforced() {
    assert_eq!(validation::clamp_limit(Some(10_000)), 200);
    assert_eq!(validation::clamp_limit(None), 50);
    assert_eq!(validation::clamp_timeout_ms(Some(120_000)), 30_000);
}

#[test]
fn only_loopback_targets_allowed() {
    assert!(validation::validate_console_addr("127.0.0.1:6669").is_ok());
    assert!(validation::validate_console_addr("127.0.0.1:6670").is_ok());
    assert!(validation::validate_console_addr("example.com:6669").is_err());
    assert!(validation::validate_console_addr("10.0.0.5:6669").is_err());
}

#[test]
fn unknown_tools_rejected() {
    assert!(inspector_mcp::permissions::assert_read_only("exec_sql").is_err());
    assert!(inspector_mcp::permissions::assert_read_only("list_tasks").is_ok());
}
