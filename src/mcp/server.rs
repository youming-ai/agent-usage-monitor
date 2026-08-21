//! MCP server implementation.
//!
//! Wraps `stats::collect()` behind 6 tools + 2 resources per the spec.
#![allow(clippy::collapsible_if)]

use chrono::{Datelike, NaiveDate};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    AnnotateAble, Implementation, ListResourcesResult, PaginatedRequestParams, ProtocolVersion,
    RawResource, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
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

/// Cache identity for a collected snapshot. The two hot unfiltered slots are
/// shared by every tool; one extra slot remembers the last filtered collect
/// (e.g. a dated `get_session_stats`) so repeated dated queries in a burst
/// don't each rescan the on-disk history.
#[derive(PartialEq, Eq)]
enum CacheKey {
    Usage,
    WithQuota,
    Filtered {
        include_quota: bool,
        platforms: Option<std::collections::BTreeSet<String>>,
        since: Option<NaiveDate>,
        until: Option<NaiveDate>,
    },
}

impl CacheKey {
    fn of(opts: &stats::CollectOptions) -> Self {
        let stats::Filters {
            platforms,
            since,
            until,
        } = &opts.filters;
        match (opts.include_quota, platforms, since, until) {
            (false, None, None, None) => Self::Usage,
            (true, None, None, None) => Self::WithQuota,
            (include_quota, platforms, since, until) => Self::Filtered {
                include_quota,
                platforms: platforms.clone(),
                since: *since,
                until: *until,
            },
        }
    }
}

#[derive(Default)]
struct SnapshotCache {
    usage: Option<(Instant, Arc<stats::StatsReport>)>,
    with_quota: Option<(Instant, Arc<stats::StatsReport>)>,
    filtered: Option<(CacheKey, Instant, Arc<stats::StatsReport>)>,
}

impl SnapshotCache {
    fn lookup(&self, key: &CacheKey) -> Option<Arc<stats::StatsReport>> {
        let slot = match key {
            CacheKey::Usage => &self.usage,
            CacheKey::WithQuota => &self.with_quota,
            CacheKey::Filtered { .. } => {
                return self
                    .filtered
                    .as_ref()
                    .filter(|(k, created, _)| k == key && created.elapsed() < SNAPSHOT_TTL)
                    .map(|(_, _, report)| report.clone());
            }
        };
        slot.as_ref()
            .filter(|(created, _)| created.elapsed() < SNAPSHOT_TTL)
            .map(|(_, report)| report.clone())
    }

    fn store(&mut self, key: CacheKey, report: Arc<stats::StatsReport>) {
        match key {
            CacheKey::Usage => self.usage = Some((Instant::now(), report)),
            CacheKey::WithQuota => self.with_quota = Some((Instant::now(), report)),
            key @ CacheKey::Filtered { .. } => {
                self.filtered = Some((key, Instant::now(), report));
            }
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
        self.collect_opts(stats::CollectOptions {
            include_quota,
            filters: stats::Filters::default(),
        })
        .await
    }

    /// Like [`collect`](Self::collect) for a filtered report; cached per
    /// filter key under the same TTL, so a burst of dated queries scans once.
    async fn collect_opts(
        &self,
        opts: stats::CollectOptions,
    ) -> Result<Arc<stats::StatsReport>, McpError> {
        let key = CacheKey::of(&opts);
        if let Some(fresh) = self.cached(&key).await {
            return Ok(fresh);
        }

        let report = Arc::new(
            stats::collect(&self.paths, opts)
                .await
                .map_err(|e| McpError::internal_error(format!("collect failed: {e}"), None))?,
        );

        let mut snapshots = self.snapshots.lock().await;
        snapshots.store(key, report.clone());
        Ok(report)
    }

    async fn cached(&self, key: &CacheKey) -> Option<Arc<stats::StatsReport>> {
        let snapshots = self.snapshots.lock().await;
        snapshots.lookup(key)
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
        let date = parse_date_param(req.date.as_deref())?.map(compact_date);
        let mut results: Vec<DailyStat> = Vec::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer {
                if key != a {
                    continue;
                }
            }
            for (d, bucket) in &pr.dates {
                if date.is_some_and(|filter| d != &filter) {
                    continue;
                }
                let date_str = d.to_string();
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
        let date = parse_date_param(req.date.as_deref())?.map(compact_date);
        let mut counts: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer {
                if key != a {
                    continue;
                }
            }
            for (d, bucket) in &pr.dates {
                if date.is_some_and(|filter| d != &filter) {
                    continue;
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
        let start = parse_date_param(req.start_date.as_deref())?.map(compact_date);
        let end = parse_date_param(req.end_date.as_deref())?.map(compact_date);
        let mut daily: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer {
                if key != a {
                    continue;
                }
            }
            for (d, bucket) in &pr.dates {
                if start.is_some_and(|s| d < &s) || end.is_some_and(|e| d > &e) {
                    continue;
                }
                let date_str = d.to_string();
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
        let date = parse_date_param(req.date.as_deref())?.map(compact_date);
        let mut out = FileOpsResponse::default();
        for (key, pr) in &report.platforms {
            if let Some(a) = &req.analyzer
                && key != a
            {
                continue;
            }
            let ops = select_tool_ops(&pr.tool_ops, date);
            let Some(ops) = ops else { continue };
            out.files_read += ops.files_read;
            out.files_edited += ops.files_edited;
            out.files_added += ops.files_added;
            out.files_deleted += ops.files_deleted;
            out.terminal_commands += ops.terminal_commands;
            out.lines_read += ops.lines_read;
            out.lines_edited += ops.lines_edited;
        }
        Ok(Json(out))
    }

    #[tool(name = "get_session_stats", description = "Get per-session summary.")]
    async fn get_session_stats(
        &self,
        Parameters(req): Parameters<GetSessionStatsRequest>,
    ) -> Result<Json<SessionStatsResponse>, String> {
        let report = if let Some(date) = parse_date_param(req.date.as_deref())? {
            self.collect_opts(stats::CollectOptions {
                include_quota: false,
                filters: stats::Filters {
                    platforms: req
                        .analyzer
                        .clone()
                        .map(|analyzer| std::collections::BTreeSet::from([analyzer])),
                    since: Some(date),
                    until: Some(date),
                },
            })
            .await
            .map_err(|e| e.to_string())?
        } else {
            self.collect(false).await.map_err(|e| e.to_string())?
        };
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
                    plan: q.plan.clone(),
                    org: q.org.clone(),
                    windows: q.windows.clone(),
                    live_summary: q.live_summary.clone(),
                    fetched_at: q.fetched_at.clone(),
                    error: q.error.clone(),
                }
            })
            .collect();
        Ok(Json(QuotaResponse { quota }))
    }
}

/// Parse an optional `YYYY-MM-DD` tool parameter, rejecting malformed input
/// with a descriptive error instead of silently matching nothing.
fn parse_date_param(date: Option<&str>) -> Result<Option<NaiveDate>, String> {
    date.map(|s| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|e| format!("date must be YYYY-MM-DD: {e}"))
    })
    .transpose()
}

/// Report date keys are compacted; convert a parsed parameter once.
fn compact_date(date: NaiveDate) -> crate::state::CompactDate {
    crate::state::CompactDate::new(date.year() as u16, date.month() as u8, date.day() as u8)
}

fn select_tool_ops(
    ops: &stats::ToolOpsView,
    date: Option<crate::state::CompactDate>,
) -> Option<&stats::ToolOpsView> {
    let Some(date) = date else {
        return Some(ops);
    };
    ops.by_date
        .iter()
        .find(|(day, _)| **day == date)
        .map(|(_, ops)| ops)
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AumMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_server_info(
            Implementation::new("aum", env!("CARGO_PKG_VERSION")).with_title("agent-usage-monitor"),
        )
        .with_instructions(
            "aum is a usage monitor for AI coding agents. Use get_daily_stats, \
             get_model_usage, get_cost_breakdown, get_file_operations, get_session_stats \
             for usage queries. get_quota returns live quota for Claude Code and Codex.",
        )
    }

    async fn list_resources(
        &self,
        _req: Option<PaginatedRequestParams>,
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
        req: ReadResourceRequestParams,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        match req.uri.as_str() {
            resource_uris::SUMMARY => {
                let report = self.collect(false).await?;
                let body = serde_json::to_string(&report.totals)
                    .map_err(|e| McpError::internal_error(format!("serialize: {e}"), None))?;
                Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(body, req.uri).with_mime_type("application/json"),
                ]))
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
                Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(body, req.uri).with_mime_type("application/json"),
                ]))
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

    #[test]
    fn file_operations_date_selects_only_that_bucket() {
        let mut all = stats::ToolOpsView {
            files_read: 9,
            ..Default::default()
        };
        all.by_date.insert(
            crate::state::CompactDate::new(2026, 8, 7),
            stats::ToolOpsView {
                files_read: 2,
                ..Default::default()
            },
        );
        assert_eq!(select_tool_ops(&all, None).unwrap().files_read, 9);
        let hit = parse_date_param(Some("2026-08-07"))
            .unwrap()
            .map(compact_date);
        assert_eq!(select_tool_ops(&all, hit).unwrap().files_read, 2);
        let miss = parse_date_param(Some("2026-08-06"))
            .unwrap()
            .map(compact_date);
        assert!(select_tool_ops(&all, miss).is_none());
    }

    #[tokio::test]
    async fn get_daily_stats_rejects_invalid_date() {
        let server = AumMcpServer::new(empty_paths());
        let req = GetDailyStatsRequest {
            analyzer: None,
            date: Some("2026/08/02".into()),
            limit: None,
        };
        let err = match server.get_daily_stats(Parameters(req)).await {
            Err(e) => e,
            Ok(_) => panic!("expected invalid-date error"),
        };
        assert!(err.contains("YYYY-MM-DD"), "unexpected error: {err}");
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
    async fn get_session_stats_filters_by_date() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("session.jsonl");
        let line = |date: &str, id: &str| {
            serde_json::json!({
                "type": "assistant",
                "timestamp": format!("{date}T12:00:00Z"),
                "sessionId": "session-1",
                "cwd": "/tmp/project",
                "message": {
                    "id": id,
                    "model": "claude-sonnet-4",
                    "usage": { "input_tokens": 10, "output_tokens": 5 }
                },
                "cost_usd": 0.01
            })
            .to_string()
        };
        std::fs::write(
            &log,
            format!(
                "{}\n{}\n",
                line("2026-08-01", "message-1"),
                line("2026-08-02", "message-2")
            ),
        )
        .unwrap();

        let paths = synthetic_paths(root.path());
        let server = AumMcpServer::new(paths);
        let result = server
            .get_session_stats(Parameters(GetSessionStatsRequest {
                analyzer: Some("claude_code".into()),
                date: Some("2026-08-02".into()),
            }))
            .await
            .unwrap();

        assert_eq!(result.0.total_sessions, 1);
        assert_eq!(result.0.sessions[0].calls, 1);
        assert_eq!(result.0.sessions[0].cost_usd, 0.01);
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
    // `Default` impl in rmcp and requires Peer/CancellationToken setup.
    // Resources are verified end-to-end by the black-box integration test
    // in `tests/mcp.rs` (Task 7).
}
