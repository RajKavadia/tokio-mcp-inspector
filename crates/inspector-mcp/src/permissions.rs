//! Read-only permission model: there are no write tools in the MVP.
//! This module is the single place that documents and enforces that invariant.
use inspector_core::CoreError;

/// All tool names exposed by the server. Adding a write tool requires
/// editing this list AND the security review in SECURITY.md.
pub const READ_ONLY_TOOLS: &[&str] = &[
    "get_runtime_overview",
    "list_tasks",
    "list_resources",
    "capture_snapshot",
    "compare_snapshots",
    "investigate_latency",
];

pub fn assert_read_only(tool_name: &str) -> Result<(), CoreError> {
    if READ_ONLY_TOOLS.contains(&tool_name) {
        Ok(())
    } else {
        Err(CoreError::Validation(format!(
            "tool '{tool_name}' is not part of the read-only MVP surface"
        )))
    }
}
