//! MCP server implementation.
//!
//! Wraps `stats::collect()` behind 6 tools + 2 resources per the spec.
#![allow(clippy::collapsible_if)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    AnnotateAble, Implementation, ListResourcesResult, PaginatedRequestParam, ProtocolVersion,
    RawResource, ReadResourceRequestParam, ReadResourceResult, ResourceContents,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::RequestContext;
use rmcp::{
    ErrorData as McpError, Json, RoleServer, ServerHandler, ServiceExt, tool, tool_handler,
    tool_router,
};

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
    snapshots: Arc<tokio::sync::Mutex<SnapshotCache>>,
}

const SNAPSHOT_TTL: Duration = Duration::from_secs(2);

#[derive(Default)]
struct SnapshotCache {
    usage: Option<(Instant, Arc<stats::StatsReport>)>,
    with_quota: Option<(Instant, Arc<stats::StatsReport>)>,
}

impl SnapshotCache {
    fn slot(&mut self, include_quota: bool) -> &mut Option<(Instant, Arc<stats::StatsReport>)> {
        if include_quota {
            &mut self.with_quota
        } else {
            &mut self.usage
        }
    }
}

impl AumMcpServer {
    pub fn new(paths: AgentPaths) -> Self {
        Self {
            tool_router: Self::tool_router(),
            paths: Arc::new(paths),
            snapshots: Arc::new(tokio::sync::Mutex::new(SnapshotCache::default())),
        }
    }

    /// Return a short-lived shared snapshot so a burst of related MCP tool
    /// calls scans the on-disk history once rather than once per tool.
    ///
    /// The cache lock is never held across the collect: a slow `include_quota`
    /// fetch (blocking HTTP) must not stall a plain usage call that would have
    /// been an instant hit on the other slot. Two concurrent misses can both
    /// collect; duplicating that work is cheaper than serialising every tool.
    async fn collect(&self, include_quota: bool) -> Result<Arc<stats::StatsReport>, McpError> {
        if let Some(fresh) = self.cached(include_quota).await {
            return Ok(fresh);
        }

        let opts = stats::CollectOptions {
            include_quota,
            filters: stats::Filters::default(),
        };
        let report = Arc::new(
            stats::collect(&self.paths, opts)
                .await
                .map_err(|e| McpError::internal_error(format!("collect failed: {e}"), None))?,
        );

        let mut snapshots = self.snapshots.lock().await;
        *snapshots.slot(include_quota) = Some((Instant::now(), report.clone()));
        Ok(report)
    }

    async fn cached(&self, include_quota: bool) -> Option<Arc<stats::StatsReport>> {
        let mut snapshots = self.snapshots.lock().await;
        snapshots
            .slot(include_quota)
            .as_ref()
            .filter(|(created, _)| created.elapsed() < SNAPSHOT_TTL)
            .map(|(_, report)| report.clone())
    }
}

#[tool_router]
impl AumMcpServer {
    #[tool(
        name = "get_daily_stats",
        description = "Get daily usage statistics, optionally filtered by analyzer and date."
    )]
    async fn get_daily_stats(
        &self,
        Parameters(req): Parameters<GetDailyStatsRequest>,
    ) -> Result<Json<DailyStatsResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let limit = req.limit.unwrap_or(30) as usize;
        let mut results: Vec<DailyStat> = Vec::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer {
                if key != a {
                    continue;
                }
            }
            for (date, bucket) in &pr.dates {
                let date_str = date.to_string();
                if let Some(d) = &req.date {
                    if &date_str != d {
                        continue;
                    }
                }
                let mut resolved_models = std::collections::BTreeMap::new();
                for (k, v) in &bucket.models {
                    resolved_models.insert(crate::state::resolve(*k).to_string(), *v);
                }
                results.push(DailyStat {
                    date: date_str,
                    calls: bucket.calls,
                    cost_usd: bucket.cost_usd,
                    input_tokens: bucket.input_tokens,
                    output_tokens: bucket.output_tokens,
                    cache_read_tokens: bucket.cache_read_tokens,
                    cache_creation_tokens: bucket.cache_creation_tokens,
                    models: resolved_models,
                });
            }
        }
        results.sort_by(|a, b| b.date.cmp(&a.date));
        results.truncate(limit);
        Ok(Json(DailyStatsResponse { results }))
    }

    #[tool(
        name = "get_model_usage",
        description = "Get AI model usage breakdown for the requested analyzer (default: all)."
    )]
    async fn get_model_usage(
        &self,
        Parameters(req): Parameters<GetModelUsageRequest>,
    ) -> Result<Json<ModelUsageResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let mut counts: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer {
                if key != a {
                    continue;
                }
            }
            for (date, bucket) in &pr.dates {
                let date_str = date.to_string();
                if let Some(d) = &req.date {
                    if &date_str != d {
                        continue;
                    }
                }
                for (model_spur, count) in &bucket.models {
                    let model_name = crate::state::resolve(*model_spur).to_string();
                    *counts.entry(model_name).or_insert(0) += count;
                }
            }
        }
        let total: u64 = counts.values().sum();
        let mut models: Vec<ModelCount> = counts
            .into_iter()
            .map(|(model, message_count)| ModelCount {
                model,
                message_count,
            })
            .collect();
        models.sort_by(|a, b| b.message_count.cmp(&a.message_count));
        Ok(Json(ModelUsageResponse {
            models,
            total_messages: total,
        }))
    }

    #[tool(
        name = "get_cost_breakdown",
        description = "Get cost breakdown for an analyzer over a date range."
    )]
    async fn get_cost_breakdown(
        &self,
        Parameters(req): Parameters<GetCostBreakRequest>,
    ) -> Result<Json<CostBreakdownResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let mut daily: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer {
                if key != a {
                    continue;
                }
            }
            for (date, bucket) in &pr.dates {
                let date_str = date.to_string();
                if let Some(s) = &req.start_date {
                    if &date_str < s {
                        continue;
                    }
                }
                if let Some(e) = &req.end_date {
                    if &date_str > e {
                        continue;
                    }
                }
                *daily.entry(date_str).or_insert(0.0) += bucket.cost_usd;
            }
        }
        let total: f64 = daily.values().sum();
        let days = daily.len().max(1) as f64;
        let daily_costs: Vec<DailyCost> = daily
            .into_iter()
            .map(|(date, cost)| DailyCost { date, cost })
            .collect();
        Ok(Json(CostBreakdownResponse {
            total_cost: total,
            daily_costs,
            average_daily_cost: total / days,
        }))
    }

    #[tool(
        name = "get_file_operations",
        description = "Get file/tool operation counts aggregated from local agent logs (reads, edits, terminals)."
    )]
    async fn get_file_operations(
        &self,
        Parameters(req): Parameters<GetFileOpsRequest>,
    ) -> Result<Json<FileOpsResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let mut out = FileOpsResponse::default();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer
                && key != a
            {
                continue;
            }
            out.files_read += pr.tool_ops.files_read;
            out.files_edited += pr.tool_ops.files_edited;
            out.files_added += pr.tool_ops.files_added;
            out.files_deleted += pr.tool_ops.files_deleted;
            out.terminal_commands += pr.tool_ops.terminal_commands;
            out.lines_read += pr.tool_ops.lines_read;
            out.lines_edited += pr.tool_ops.lines_edited;
        }
        Ok(Json(out))
    }

    #[tool(name = "get_session_stats", description = "Get per-session summary.")]
    async fn get_session_stats(
        &self,
        Parameters(req): Parameters<GetSessionStatsRequest>,
    ) -> Result<Json<SessionStatsResponse>, String> {
        let report = self.collect(false).await.map_err(|e| e.to_string())?;
        let mut sessions: Vec<SessionEntry> = Vec::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer {
                if key != a {
                    continue;
                }
            }
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
        Ok(Json(SessionStatsResponse {
            sessions,
            total_sessions: total,
        }))
    }

    #[tool(
        name = "get_quota",
        description = "Get live quota (Claude Code, Codex) and account identity for all platforms."
    )]
    async fn get_quota(&self) -> Result<Json<QuotaResponse>, String> {
        let report = self.collect(true).await.map_err(|e| e.to_string())?;
        let quota: Vec<QuotaEntry> = report
            .platforms
            .values()
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
                 for usage queries. get_quota returns live quota for Claude Code and Codex \
                 for Claude Code and Codex."
                    .to_string(),
            ),
        }
    }

    async fn list_resources(
        &self,
        _req: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                RawResource {
                    uri: resource_uris::SUMMARY.to_string(),
                    name: "summary".to_string(),
                    title: Some("Overall usage summary".to_string()),
                    description: Some("Cross-platform totals".to_string()),
                    mime_type: None,
                    size: None,
                    icons: None,
                    meta: None,
                }
                .no_annotation(),
                RawResource {
                    uri: resource_uris::PLATFORMS.to_string(),
                    name: "platforms".to_string(),
                    title: Some("Supported platforms".to_string()),
                    description: Some(format!(
                        "{}-platform index with availability status",
                        crate::platforms::entries().len()
                    )),
                    mime_type: None,
                    size: None,
                    icons: None,
                    meta: None,
                }
                .no_annotation(),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        req: ReadResourceRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        match req.uri.as_str() {
            resource_uris::SUMMARY => {
                let report = self.collect(false).await?;
                let body = serde_json::to_string(&report.totals)
                    .map_err(|e| McpError::internal_error(format!("serialize: {e}"), None))?;
                Ok(ReadResourceResult {
                    contents: vec![ResourceContents::TextResourceContents {
                        uri: req.uri,
                        mime_type: Some("application/json".to_string()),
                        text: body,
                        meta: None,
                    }],
                })
            }
            resource_uris::PLATFORMS => {
                let _report = self.collect(false).await?;
                let platforms: Vec<serde_json::Value> = crate::platforms::entries()
                    .iter()
                    .map(|entry| {
                        let key = crate::stats::platform_canonical_key(entry.platform);
                        let data_path = self.paths.path_for(entry.platform);
                        serde_json::json!({
                            "key": key,
                            "display_name": entry.log_name,
                            "available": data_path.exists(),
                            "data_path": data_path.to_string_lossy(),
                        })
                    })
                    .collect();
                let body = serde_json::to_string(&serde_json::json!({ "platforms": platforms }))
                    .map_err(|e| McpError::internal_error(format!("serialize: {e}"), None))?;
                Ok(ReadResourceResult {
                    contents: vec![ResourceContents::TextResourceContents {
                        uri: req.uri,
                        mime_type: Some("application/json".to_string()),
                        text: body,
                        meta: None,
                    }],
                })
            }
            _ => Err(McpError::resource_not_found(
                format!("unknown resource: {}", req.uri),
                None,
            )),
        }
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

    /// Build synthetic paths whose every registered platform points at `root`.
    /// The tempdir exists but has no parseable data, so all readers return 0
    /// records.
    fn synthetic_paths(root: &std::path::Path) -> AgentPaths {
        let mut map = std::collections::HashMap::new();
        for entry in crate::platforms::entries() {
            map.insert(entry.platform, root.to_path_buf());
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
        // which may or may not exist in the test env.
        let _ = result.0;
    }

    #[tokio::test]
    async fn related_calls_share_a_recent_usage_snapshot() {
        let server = AumMcpServer::new(empty_paths());
        let first = server.collect(false).await.unwrap();
        let second = server.collect(false).await.unwrap();

        assert!(Arc::ptr_eq(&first, &second));
    }

    // Resource tests require constructing a `RequestContext`, which has no
    // `Default` impl in rmcp 0.12 and requires Peer/CancellationToken setup.
    // Resources are verified end-to-end by the black-box integration test
    // in `tests/mcp.rs` (Task 7).
}
