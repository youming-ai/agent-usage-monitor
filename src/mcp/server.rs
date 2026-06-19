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
use rmcp::{ErrorData as McpError, Json, RoleServer, ServerHandler, ServiceExt, tool_handler, tool_router};

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
    // 6 tool handlers added in Task 4.
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
