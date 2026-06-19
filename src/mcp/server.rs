//! MCP server implementation.
//!
//! Wraps `stats::collect()` behind 6 tools + 2 resources per the spec.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    AnnotateAble, Implementation, ListResourcesResult, PaginatedRequestParam,
    ProtocolVersion, RawResource, ReadResourceRequestParam, ReadResourceResult,
    ResourceContents, ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, Json, RoleServer, ServerHandler, ServiceExt, tool, tool_handler, tool_router};

use crate::mcp::types::*;
use crate::state::AgentPaths;
use crate::stats;

mod resource_uris {
    pub const SUMMARY: &str = "aum://summary";
    pub const PLATFORMS: &str = "aum://platforms";
}

#[derive(Clone)]
pub struct AumMcpServer {
    tool_router: ToolRouter<Self>,
    paths: Arc<AgentPaths>,
}

impl AumMcpServer {
    pub fn new(paths: AgentPaths) -> Self {
        Self {
            tool_router: Self::tool_router(),
            paths: Arc::new(paths),
        }
    }

    /// Drive a fresh collect() and return the StatsReport.
    async fn collect(&self, include_quota: bool) -> Result<stats::StatsReport, McpError> {
        let opts = stats::CollectOptions {
            include_quota,
            filters: stats::Filters::default(),
        };
        stats::collect(&self.paths, opts)
            .await
            .map_err(|e| McpError::internal_error(format!("collect failed: {e}"), None))
    }
}

#[tool_router]
impl AumMcpServer {
    #[tool(name = "get_daily_stats", description = "Get daily usage statistics, optionally filtered by analyzer and date.")]
    async fn get_daily_stats(
        &self,
        Parameters(req): Parameters<GetDailyStatsRequest>,
    ) -> Result<Json<DailyStatsResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let limit = req.limit.unwrap_or(30) as usize;
        let mut results: Vec<DailyStat> = Vec::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer { if key != a { continue; } }
            for (date, bucket) in &pr.dates {
                if let Some(d) = &req.date { if date != d { continue; } }
                results.push(DailyStat {
                    date: date.clone(),
                    calls: bucket.calls,
                    cost_usd: bucket.cost_usd,
                    input_tokens: bucket.input_tokens,
                    output_tokens: bucket.output_tokens,
                    cache_read_tokens: bucket.cache_read_tokens,
                    cache_creation_tokens: bucket.cache_creation_tokens,
                    models: bucket.models.clone(),
                });
            }
        }
        results.sort_by(|a, b| b.date.cmp(&a.date));
        results.truncate(limit);
        Ok(Json(DailyStatsResponse { results }))
    }

    #[tool(name = "get_model_usage", description = "Get AI model usage breakdown for the requested analyzer (default: all).")]
    async fn get_model_usage(
        &self,
        Parameters(req): Parameters<GetModelUsageRequest>,
    ) -> Result<Json<ModelUsageResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let mut counts: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer { if key != a { continue; } }
            for (date, bucket) in &pr.dates {
                if let Some(d) = &req.date { if date != d { continue; } }
                for (model, count) in &bucket.models {
                    *counts.entry(model.clone()).or_insert(0) += count;
                }
            }
        }
        let total: u64 = counts.values().sum();
        let mut models: Vec<ModelCount> = counts.into_iter()
            .map(|(model, message_count)| ModelCount { model, message_count })
            .collect();
        models.sort_by(|a, b| b.message_count.cmp(&a.message_count));
        Ok(Json(ModelUsageResponse { models, total_messages: total }))
    }

    #[tool(name = "get_cost_breakdown", description = "Get cost breakdown for an analyzer over a date range.")]
    async fn get_cost_breakdown(
        &self,
        Parameters(req): Parameters<GetCostBreakRequest>,
    ) -> Result<Json<CostBreakdownResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let mut daily: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer { if key != a { continue; } }
            for (date, bucket) in &pr.dates {
                if let Some(s) = &req.start_date { if date < s { continue; } }
                if let Some(e) = &req.end_date { if date > e { continue; } }
                *daily.entry(date.clone()).or_insert(0.0) += bucket.cost_usd;
            }
        }
        let total: f64 = daily.values().sum();
        let days = daily.len().max(1) as f64;
        let daily_costs: Vec<DailyCost> = daily.into_iter()
            .map(|(date, cost)| DailyCost { date, cost })
            .collect();
        Ok(Json(CostBreakdownResponse {
            total_cost: total,
            daily_costs,
            average_daily_cost: total / days,
        }))
    }

    #[tool(name = "get_file_operations", description = "Get file operation statistics (returns 0 in spec 2; reader-side data not yet collected).")]
    async fn get_file_operations(
        &self,
        _req: Parameters<GetFileOpsRequest>,
    ) -> Result<Json<FileOpsResponse>, String> {
        Ok(Json(FileOpsResponse::default()))
    }

    #[tool(name = "get_session_stats", description = "Get per-session summary.")]
    async fn get_session_stats(
        &self,
        Parameters(req): Parameters<GetSessionStatsRequest>,
    ) -> Result<Json<SessionStatsResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let mut sessions: Vec<SessionEntry> = Vec::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer { if key != a { continue; } }
            for s in &pr.sessions {
                sessions.push(SessionEntry {
                    session: s.session.clone(),
                    analyzer: key.clone(),
                    calls: s.calls,
                    cost_usd: s.cost_usd,
                    input_tokens: s.input_tokens,
                    output_tokens: s.output_tokens,
                });
            }
        }
        let total = sessions.len() as u64;
        Ok(Json(SessionStatsResponse { sessions, total_sessions: total }))
    }

    #[tool(name = "get_quota", description = "Get live quota for Claude Code and Codex.")]
    async fn get_quota(&self) -> Result<Json<QuotaResponse>, String> {
        let report = self.collect(true).await.map_err(|e| e.to_string())?;
        let quota: Vec<QuotaEntry> = report.platforms.values()
            .filter(|pr| pr.quota.is_some())
            .map(|pr| {
                let q = pr.quota.as_ref().unwrap();
                QuotaEntry {
                    platform: pr.platform_key.clone(),
                    tool_name: q.tool_name.clone(),
                    email: q.email.clone(),
                    windows: q.windows.clone(),
                    fetched_at: q.fetched_at.clone(),
                    error: q.error.clone(),
                }
            })
            .collect();
        Ok(Json(QuotaResponse { quota }))
    }
}

#[tool_handler]
impl ServerHandler for AumMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            server_info: Implementation {
                name: "aum".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("agent-usage-monitor".to_string()),
                website_url: None,
                icons: None,
            },
            instructions: Some(
                "aum is a usage monitor for AI coding agents. Use get_daily_stats, \
                 get_model_usage, get_cost_breakdown, get_file_operations, get_session_stats \
                 for usage queries. get_quota returns live quota for Claude Code and Codex."
                    .to_string(),
            ),
        }
    }

    async fn list_resources(
        &self,
        _req: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        // Implemented in Task 5.
        Ok(ListResourcesResult { resources: vec![], next_cursor: None, meta: None })
    }

    async fn read_resource(
        &self,
        _req: ReadResourceRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        // Implemented in Task 5.
        Err(McpError::resource_not_found("not implemented yet".to_string(), None))
    }
}

pub async fn run_mcp_server(paths: AgentPaths) -> anyhow::Result<()> {
    let server = AumMcpServer::new(paths);
    let transport = rmcp::transport::io::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentPaths;
    use std::collections::HashMap;

    /// Build a synthetic AgentPaths whose every registered Tab points at
    /// `root`. The tempdir exists but has no parseable data, so all 13
    /// readers return 0 records.
    fn synthetic_paths(root: &std::path::Path) -> AgentPaths {
        let mut map = std::collections::HashMap::new();
        for entry in crate::platforms::entries() {
            map.insert(entry.tab, root.to_path_buf());
        }
        AgentPaths::new(map)
    }

    fn empty_paths() -> AgentPaths {
        synthetic_paths(tempfile::tempdir().expect("tempdir").path())
    }

    #[tokio::test]
    async fn get_daily_stats_returns_empty_when_no_data() {
        let server = AumMcpServer::new(empty_paths());
        let req = GetDailyStatsRequest::default();
        let result = server.get_daily_stats(Parameters(req)).await.unwrap();
        assert_eq!(result.0.results.len(), 0);
    }

    #[tokio::test]
    async fn get_model_usage_returns_empty_when_no_data() {
        let server = AumMcpServer::new(empty_paths());
        let req = GetModelUsageRequest::default();
        let result = server.get_model_usage(Parameters(req)).await.unwrap();
        assert_eq!(result.0.models.len(), 0);
        assert_eq!(result.0.total_messages, 0);
    }

    #[tokio::test]
    async fn get_cost_breakdown_returns_empty_when_no_data() {
        let server = AumMcpServer::new(empty_paths());
        let req = GetCostBreakRequest::default();
        let result = server.get_cost_breakdown(Parameters(req)).await.unwrap();
        assert_eq!(result.0.total_cost, 0.0);
        assert_eq!(result.0.daily_costs.len(), 0);
    }

    #[tokio::test]
    async fn get_file_operations_returns_zeros() {
        let server = AumMcpServer::new(empty_paths());
        let req = GetFileOpsRequest::default();
        let result = server.get_file_operations(Parameters(req)).await.unwrap();
        assert_eq!(result.0.files_read, 0);
        assert_eq!(result.0.files_edited, 0);
    }

    #[tokio::test]
    async fn get_session_stats_returns_empty_when_no_data() {
        let server = AumMcpServer::new(empty_paths());
        let req = GetSessionStatsRequest::default();
        let result = server.get_session_stats(Parameters(req)).await.unwrap();
        assert_eq!(result.0.sessions.len(), 0);
        assert_eq!(result.0.total_sessions, 0);
    }

    #[tokio::test]
    async fn get_quota_returns_valid_response() {
        let server = AumMcpServer::new(empty_paths());
        let result = server.get_quota().await.unwrap();
        // No assertions on content — quota fetch depends on local credentials
        // which may or may not exist in the test env. We just verify the
        // response is a well-formed QuotaResponse (one that deserializes).
        let _ = result.0;
    }
}
