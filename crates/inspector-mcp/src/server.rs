//! MCP server: thin, bounded, read-only wrapper over tested core operations.
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, ServiceExt,
};

use inspector_analysis::{
    baselines::compare_snapshots, contention::detect_contention, evidence::validate_diagnosis,
    latency::investigate_latency_with_threshold, task_growth::detect_task_growth,
};
use inspector_collector::{
    metrics_adapter::{attach_latency, validate_metrics_url},
    ConnectionConfig, ConsoleCollector,
};

use crate::tools::{
    CaptureParams, CommonParams, CompareParams, InvestigateParams, ListParams, SnapshotStore,
};
use crate::validation::{clamp_limit, clamp_timeout_ms, validate_console_addr};

#[derive(Clone)]
pub struct Inspector {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    store: SnapshotStore,
}

impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

impl Inspector {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            store: SnapshotStore::default(),
        }
    }

    pub fn store(&self) -> SnapshotStore {
        self.store.clone()
    }

    fn collector(
        addr: &str,
        timeout_ms: Option<u64>,
        limit: Option<usize>,
    ) -> Result<ConsoleCollector, McpError> {
        let addr = validate_console_addr(addr)
            .map_err(|e| McpError::invalid_params(e.to_string(), None))?;
        let n = clamp_limit(limit);
        Ok(ConsoleCollector::new(ConnectionConfig {
            addr,
            connect_timeout_ms: clamp_timeout_ms(timeout_ms),
            collection_timeout_ms: clamp_timeout_ms(timeout_ms),
            max_tasks: n,
            max_resources: n,
        }))
    }

    fn to_mcp(core: inspector_core::CoreError) -> McpError {
        McpError::internal_error(core.to_string(), None)
    }

    fn json(result: impl serde::Serialize) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

#[tool_router]
impl Inspector {
    #[tool(
        description = "Check service identity, telemetry health, and available metrics (read-only)"
    )]
    async fn get_runtime_overview(
        &self,
        Parameters(p): Parameters<CommonParams>,
    ) -> Result<CallToolResult, McpError> {
        let c = Self::collector(&p.console_addr, p.timeout_ms, Some(10))?;
        let obs = c.capture_snapshot(None).await.map_err(Self::to_mcp)?;
        Self::json(&obs)
    }

    #[tool(description = "Inspect available task metadata, paginated (read-only)")]
    async fn list_tasks(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<CallToolResult, McpError> {
        let n = clamp_limit(p.limit);
        let c = Self::collector(&p.common.console_addr, p.common.timeout_ms, Some(n))?;
        let mut obs = c.capture_snapshot(None).await.map_err(Self::to_mcp)?;
        obs.data.tasks.truncate(n);
        Self::json(&obs)
    }

    #[tool(description = "Inspect available resource telemetry, paginated (read-only)")]
    async fn list_resources(
        &self,
        Parameters(p): Parameters<ListParams>,
    ) -> Result<CallToolResult, McpError> {
        let n = clamp_limit(p.limit);
        let c = Self::collector(&p.common.console_addr, p.common.timeout_ms, Some(n))?;
        let mut obs = c.capture_snapshot(None).await.map_err(Self::to_mcp)?;
        obs.data.resources.truncate(n);
        Self::json(&obs)
    }

    #[tool(description = "Collect a bounded diagnostic observation and store it for comparison")]
    async fn capture_snapshot(
        &self,
        Parameters(p): Parameters<CaptureParams>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(url) = &p.metrics_url {
            validate_metrics_url(url).map_err(|e| McpError::invalid_params(e.to_string(), None))?;
        }
        let c = Self::collector(&p.common.console_addr, p.common.timeout_ms, p.limit)?;
        let mut obs = c
            .capture_snapshot(p.service_name)
            .await
            .map_err(Self::to_mcp)?;
        if let Some(url) = &p.metrics_url {
            let timeout = std::time::Duration::from_millis(clamp_timeout_ms(p.common.timeout_ms));
            attach_latency(&mut obs, url, timeout).await;
        }
        let id = self.store.put(obs.clone()).await;
        Self::json(serde_json::json!({ "stored_id": id, "observation": obs }))
    }

    #[tool(description = "Calculate changes between two stored observations")]
    async fn compare_snapshots(
        &self,
        Parameters(p): Parameters<CompareParams>,
    ) -> Result<CallToolResult, McpError> {
        let base = self.store.get(&p.base_id).await.ok_or_else(|| {
            McpError::invalid_params(format!("unknown base_id '{}'", p.base_id), None)
        })?;
        let current = self.store.get(&p.current_id).await.ok_or_else(|| {
            McpError::invalid_params(format!("unknown current_id '{}'", p.current_id), None)
        })?;
        let delta = compare_snapshots(&base, &current).map_err(Self::to_mcp)?;
        Self::json(&delta)
    }

    #[tool(
        description = "Run deterministic latency, task-growth, and contention rules over two stored observations"
    )]
    async fn investigate_latency(
        &self,
        Parameters(p): Parameters<InvestigateParams>,
    ) -> Result<CallToolResult, McpError> {
        let base = self.store.get(&p.base_id).await.ok_or_else(|| {
            McpError::invalid_params(format!("unknown base_id '{}'", p.base_id), None)
        })?;
        let current = self.store.get(&p.current_id).await.ok_or_else(|| {
            McpError::invalid_params(format!("unknown current_id '{}'", p.current_id), None)
        })?;
        let threshold = p.threshold_ratio.unwrap_or(1.5).clamp(1.1, 100.0);
        let latency = investigate_latency_with_threshold(
            &base.data,
            &current.data,
            &p.base_id,
            &p.current_id,
            threshold,
        );
        let growth = detect_task_growth(&base.data, &current.data, &p.base_id, &p.current_id);
        let contention = detect_contention(&base.data, &current.data, &p.base_id, &p.current_id);
        validate_diagnosis(&latency).map_err(Self::to_mcp)?;
        Self::json(
            serde_json::json!({ "latency": latency, "task_growth": growth, "contention": contention }),
        )
    }
}

#[tool_handler]
impl rmcp::ServerHandler for Inspector {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Read-only Tokio diagnostics. Observed/Hypothesis/Verified only; every finding cites observation IDs.",
            )
    }
}

pub async fn run_stdio() -> anyhow::Result<()> {
    let service = Inspector::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
