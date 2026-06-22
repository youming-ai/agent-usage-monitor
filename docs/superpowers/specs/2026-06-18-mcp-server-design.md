# 设计文档：aum mcp server

**日期：** 2026-06-18
**状态：** 已批准
**作者：** opencode
**关联：** Spec 2/4（splitrail 学习后的改造）

## 概述

为 `agent-usage-monitor` 新增 `aum mcp` 子命令，启动一个 **MCP（Model Context Protocol）服务器**，通过 stdio 暴露 6 个 tools + 2 个 resources，让 agent 工具（Claude Code、Cursor、Copilot 等）能够查询自己的用量、cost、quota。

服务器**不启动 TUI**，与 spec 1 的 `aum stats --json` 共享数据源（`stats::collect()`）。每个 tool call 触发一次完整 re-scan，输出与 spec 1 的 JSON schema 兼容的子集。

## 目标

1. 让 agent 工具能够自查用量（"我今天花了多少"、"剩余 quota 是多少"）
2. 复用 spec 1 的 `stats::collect()`，零数据重复
3. 零网络暴露（stdio only），符合 MCP 习惯
4. 工具 schema 对齐 splitrail 的 MCP server（让迁移/参考容易）
5. 退出干净（agent 断开 → server 退出）

## 非目标

- **不做** HTTP / SSE 传输（spec 2 只 stdio）
- **不做** 鉴权（本地 trusted client 假设）
- **不做** 持久化 cache（每次 re-scan，13 个平台 < 500ms）
- **不做** 实时推送（spec 3 notify watcher 的事；spec 2 是请求-响应）
- **不引入** simd-json / lasso / 等性能 crate（spec 4 的事）

## 架构设计

### 新增文件

| 文件 | 职责 |
|---|---|
| `src/mcp/mod.rs` | `pub mod server; pub mod types;` + `pub use server::AumMcpServer;` |
| `src/mcp/server.rs` | `AumMcpServer` 结构 + 6 个 `#[tool]` handler + 2 个 `#[resource]` handler + `run_mcp_server()` 入口 |
| `src/mcp/types.rs` | 6 个 Request 类型（`GetDailyStatsRequest`, `GetModelUsageRequest`, ...）+ 6 个 Response 类型 |
| `tests/mcp.rs` | 黑盒 stdio 测试（spawn `aum mcp`，JSON-RPC initialize + tools/list） |

### 修改文件

| 文件 | 改动 |
|---|---|
| `Cargo.toml` | `rmcp = { version = "0.12", features = ["server", "macros", "transport-io"] }`（自动拉 `schemars`） |
| `src/lib.rs` | `pub mod mcp;` |
| `src/cli.rs` | `Commands::Mcp` 变体（无 args） |
| `src/main.rs` | `Mcp` match arm → `mcp::run_mcp_server().await` |
| `README.md` | `## MCP server` 段 + `aum mcp` 例子 + 客户端配置示例 |

## 工具 schema

### Tool 1: `get_daily_stats`

**用途：** 每日汇总（calls、cost、tokens、file ops）。可按 agent 或日期过滤。

```json
// Request
{
  "analyzer": "claude_code",      // optional, default: 全部
  "date": "2026-06-15",           // optional, default: 全部日期
  "limit": 30                     // optional, default: 30
}

// Response
{
  "results": [
    {
      "date": "2026-06-15",
      "stats": {
        "calls": 200,
        "cost_usd": 2.10,
        "input_tokens": 250000,
        "output_tokens": 50000,
        "cache_read_tokens": 180000,
        "cache_creation_tokens": 0
      },
      "models": { "claude-sonnet-4": 200 },
      "file_ops": {
        "files_read": 50,
        "files_edited": 10,
        "files_added": 2,
        "files_deleted": 0,
        "terminal_commands": 30
      }
    }
  ]
}
```

排序：date 降序。`limit` 截断。

### Tool 2: `get_model_usage`

```json
// Request
{ "analyzer": "claude_code", "date": "2026-06-15" }

// Response
{
  "models": [
    { "model": "claude-sonnet-4", "message_count": 800 }
  ],
  "total_messages": 1000
}
```

排序：message_count 降序。

### Tool 3: `get_cost_breakdown`

```json
// Request
{
  "analyzer": "claude_code",
  "start_date": "2026-06-01",
  "end_date": "2026-06-30"
}

// Response
{
  "total_cost": 12.34,
  "daily_costs": [
    { "date": "2026-06-01", "cost": 2.10 },
    { "date": "2026-06-02", "cost": 1.50 }
  ],
  "average_daily_cost": 0.41
}
```

### Tool 4: `get_file_operations`

```json
// Request
{ "analyzer": "claude_code", "date": "2026-06-15" }

// Response
{
  "files_read": 50,
  "files_edited": 10,
  "files_added": 2,
  "files_deleted": 0,
  "terminal_commands": 30,
  "lines_read": 5000,
  "lines_edited": 200
}
```

注：`UsageRecord` 当前没有 file_ops 字段。spec 1 的 `TuiStats` 也没有。这是 spec 2 引入的新增数据：需要修改 `UsageRecord` 加上 `files_read/files_edited/files_added/files_deleted/terminal_commands/lines_read/lines_edited` 字段，默认 0；`collect()` 阶段从原始日志读取填充（如果原始日志没有这些字段则保持 0）。这是 spec 2 唯一需要扩展 `UsageRecord` 的地方。

### Tool 5: `get_session_stats`

```json
// Request
{ "analyzer": "claude_code", "date": "2026-06-15" }

// Response
{
  "sessions": [
    {
      "session_id": "myapp abc12345",
      "first_timestamp": "2026-06-15T10:30:00Z",
      "analyzer": "claude_code",
      "stats": { /* same as TuiStats */ },
      "models": ["claude-sonnet-4"]
    }
  ],
  "total_sessions": 12
}
```

### Tool 6: `get_quota`

```json
// Request: {}

// Response
{
  "quota": [
    {
      "platform": "claude_code",
      "tool_name": "Claude Code",
      "email": "me@example.com",
      "windows": [
        {
          "label": "5h",
          "remaining_percent": 0.85,
          "resets_at": "2026-06-18T18:00:00Z",
          "reset_in": "3h 25m"
        }
      ],
      "fetched_at": "2026-06-18T14:35:00Z",
      "error": null
    }
  ]
}
```

注：与 `aum stats --include-quota` 的 `quota` 字段同源。`quota::fetchers()` 已经实现。

## Resources

### Resource 1: `aum://summary`

整体 totals（与 `aum stats --json` 顶层 `totals` 同构）：

```json
{
  "total_calls": 1234,
  "total_cost_usd": 12.34,
  "platforms_with_data": 5
}
```

### Resource 2: `aum://platforms`

13 个 platform 的索引（available 状态 + data_path）：

```json
{
  "platforms": [
    {
      "key": "claude_code",
      "display_name": "Claude Code",
      "available": true,
      "data_path": "/Users/me/.claude/projects"
    },
    { "key": "codex", "display_name": "Codex", "available": false, "data_path": "~/.codex" }
  ]
}
```

## 实现细节

### `src/mcp/server.rs`（骨架）

```rust
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
use crate::platforms::{self, AgentPaths};
use crate::quota;
use crate::stats;

/// Resource URI constants
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

    /// Drive a fresh collect() (no cache) and return the StatsReport.
    /// Each tool handler invokes this; matches spec 1's "re-scan per call" decision.
    async fn collect(&self, opts: stats::CollectOptions) -> Result<stats::StatsReport, McpError> {
        stats::collect(&self.paths, opts)
            .await
            .map_err(|e| McpError::internal_error(format!("collect failed: {e}"), None))
    }

    /// Helper: run stats::collect() with include_quota=false, no filters, then attach quotas.
    async fn collect_with_quota(&self, include_quota: bool) -> Result<stats::StatsReport, McpError> {
        let opts = stats::CollectOptions {
            include_quota,
            filters: stats::Filters::default(),
        };
        self.collect(opts).await
    }
}

#[tool_router]
impl AumMcpServer {
    #[tool(name = "get_daily_stats", description = "Get daily usage statistics...")]
    async fn get_daily_stats(
        &self,
        Parameters(req): Parameters<GetDailyStatsRequest>,
    ) -> Result<Json<DailyStatsResponse>, String> {
        let report = self.collect_with_quota(false).await.map_err(|e| e.to_string())?;
        // extract daily stats, filter by analyzer+date, sort desc, apply limit
        // ...
    }

    #[tool(name = "get_model_usage", description = "Get breakdown of AI model usage...")]
    async fn get_model_usage(
        &self,
        Parameters(req): Parameters<GetModelUsageRequest>,
    ) -> Result<Json<ModelUsageResponse>, String> { ... }

    #[tool(name = "get_cost_breakdown", description = "Get cost breakdown over a date range...")]
    async fn get_cost_breakdown(
        &self,
        Parameters(req): Parameters<GetCostBreakRequest>,
    ) -> Result<Json<CostBreakdownResponse>, String> { ... }

    #[tool(name = "get_file_operations", description = "Get file operation statistics...")]
    async fn get_file_operations(
        &self,
        Parameters(req): Parameters<GetFileOpsRequest>,
    ) -> Result<Json<FileOpsResponse>, String> { ... }

    #[tool(name = "get_session_stats", description = "Get per-session summary...")]
    async fn get_session_stats(
        &self,
        Parameters(req): Parameters<GetSessionStatsRequest>,
    ) -> Result<Json<SessionStatsResponse>, String> { ... }

    #[tool(name = "get_quota", description = "Get live quota for Claude/Codex...")]
    async fn get_quota(&self) -> Result<Json<QuotaResponse>, String> { ... }
}

#[tool_handler]
impl ServerHandler for AumMcpServer {
    fn server_info(&self) -> ServerInfo {
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
                    annotations: None,
                }.no_annotation(),
                RawResource {
                    uri: resource_uris::PLATFORMS.to_string(),
                    name: "platforms".to_string(),
                    title: Some("Supported platforms".to_string()),
                    description: Some("13-platform index with availability status".to_string()),
                    mime_type: None,
                    size: None,
                    icons: None,
                    annotations: None,
                }.no_annotation(),
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
                let report = self.collect_with_quota(false).await?;
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
                let report = self.collect_with_quota(false).await?;
                // serialize platform index (key, display_name, available, data_path)
                ...
            }
            _ => Err(McpError::resource_not_found(
                format!("unknown resource: {}", req.uri),
                None,
            )),
        }
    }
}

pub async fn run_mcp_server(paths: AgentPaths) -> Result<()> {
    use rmcp::transport::io::stdio;
    let server = AumMcpServer::new(paths);
    let transport = stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
```

### `src/cli.rs`（增量）

```rust
#[derive(Subcommand, Debug)]
pub enum Commands {
    Update { /* 已有 */ },
    Config { /* 已有 */ },
    Stats(StatsArgs),  // spec 1
    /// Run as an MCP (Model Context Protocol) server over stdio
    Mcp,
}
```

### `src/main.rs`（增量）

```rust
Some(cli::Commands::Mcp) => {
    return handle_mcp(&config).await;
}
// ...
async fn handle_mcp(config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = cli::Cli::parse();
    let paths = platforms::resolve_paths(&cli, config);
    mcp::run_mcp_server(paths).await?;
    Ok(())
}
```

## UsageRecord 扩展（spec 2 唯一需要扩展 `UsageRecord` 的地方）

为了让 `get_file_operations` 真的有数据，扩展 `UsageRecord`：

```rust
// src/state/app_state.rs
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    pub platform: Platform,
    pub model: String,
    pub session: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
    // NEW (spec 2):
    pub files_read: u64,
    pub files_edited: u64,
    pub files_added: u64,
    pub files_deleted: u64,
    pub terminal_commands: u64,
    pub lines_read: u64,
    pub lines_edited: u64,
}
```

每个 reader 的 `parse_line` 暂时填 0（spec 1 没要求 file ops）。后续任务可以从日志扩展（如果日志包含这些字段）。spec 2 **不**改 reader 的 parse 逻辑——只让 `UsageRecord` 容纳这些字段，默认 0。

## 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| MCP SDK | `rmcp 0.12` | Rust 生态标准，splitrail 同款 |
| Transport | stdio only | MCP 习惯，无网络暴露 |
| 挂载点 | `aum mcp` 子命令 | 与 splitrail 同构 |
| 数据源 | 每次 call 重 scan `stats::collect()` | spec 1 决策延伸；<500ms 可接受 |
| Cache | 无 | 简单；spec 4 性能优化可加 |
| 鉴权 | 无 | 本地 trusted client |
| Tool 数量 | 6 + 2 resources | 对齐 splitrail |
| `UsageRecord` 扩展 | 加 7 个 file_ops 字段（默认 0） | spec 2 唯一需要的数据扩展 |
| Reader parse 逻辑 | 不动 | spec 2 范围仅 server；reader 增强留给后续 |
| 错误格式 | `rmcp::ErrorData` 包装 anyhow | 标准 MCP 错误 |
| 并发 | 每个 tool call 独立 | MCP request/response 模型 |
| 进程模型 | 一个 stdio server 一个进程 | MCP 标准 |

## 测试

### `tests/mcp.rs`（新建，黑盒）

| 测试 | 验证 |
|---|---|
| `mcp_initialize_returns_server_info` | spawn `aum mcp`，发 `initialize` JSON-RPC，验证响应有 `serverInfo.name == "aum"` |
| `mcp_tools_list_returns_six_tools` | 发 `tools/list`，验证响应有 6 个 tool 名字 |
| `mcp_resources_list_returns_two_resources` | 发 `resources/list`，验证有 `aum://summary` 和 `aum://platforms` |
| `mcp_get_quota_returns_empty_without_credentials` | tool call `get_quota`，验证返回结构（可能 error） |
| `mcp_get_daily_stats_call_does_not_panic` | tool call `get_daily_stats`，验证响应有 results 数组 |

### 单元测试 (`src/mcp/server.rs` `#[cfg(test)] mod tests`)

- `parse_daily_stats_filters_by_analyzer_and_date`
- `parse_model_usage_sorts_by_count`
- `parse_cost_breakdown_computes_average`
- `parse_session_stats_includes_analyzer_name`

## 向后兼容

- 现有 TUI 主路径 0 改动
- `aum` 无 subcommand 行为不变（启动 TUI）
- `aum update` / `aum config` / `aum stats` 行为不变
- `UsageRecord` 加字段是 additive：所有 reader 默认填 0，旧代码兼容

## 依赖项

- `rmcp = { version = "0.12", features = ["server", "macros", "transport-io"] }`
- 传递依赖：`schemars`（rmcp 用），`tokio` 已有
- `serde` / `serde_json` / `chrono` 已有

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| `rmcp 0.12` API 与 splitrail 的 0.8 不同 | 实施时先 `cargo doc --open` 验证语法；如差异大用对应版本 |
| 6 个 tool handler 大量重复 (都调 collect + extract) | 抽出 `helper: collect_for_analyzer(&self, name: &str) -> Result<...>` 减少重复 |
| `get_file_operations` 永远返回 0（reader 没填） | 文档明确：spec 2 仅暴露字段，data 来自 reader 后续增强 |
| MCP tool 错误格式不标准 | 用 `rmcp::ErrorData::internal_error` / `invalid_params` 等 |
| Server 启动失败无明显提示 | main.rs 在 handle_mcp 失败时打日志到 stderr |
| 客户端配置难懂 | README 给 Claude Code / Cursor / Copilot 三个最常见 client 的配置 snippet |

## 文档

- README.md 新增 `## MCP server` 段：
  - 简介 + 启动方式
  - 客户端配置示例（Claude Code `.mcp.json`、Cursor、Copilot CLI）
  - 6 tools + 2 resources 列表
  - 一段 "Typical queries"（agent 怎么用）
- CHANGELOG：release-please 自动，conventional commit `feat: add aum mcp server with 6 tools and 2 resources`

## 实施顺序（writing-plans 阶段细化）

1. 加 `UsageRecord` 的 7 个 file_ops 字段（默认 0）— 1 个 commit
2. 加 `Cargo.toml` 的 `rmcp` 依赖 — 1 个 commit
3. 加 `src/mcp/{mod,types,server}.rs` 骨架 + `pub mod mcp;` — 1 个 commit
4. 实现 6 个 tool handlers（按 splitrail 风格，从简单到复杂）— 2-3 commits
5. 实现 2 个 resources — 1 commit
6. 加 `aum mcp` 子命令 + main.rs 路由 — 1 commit
7. 加 `tests/mcp.rs` 黑盒测试 — 1 commit
8. README + smoke test — 1 commit
9. 最终 verification — 1 commit

## 待办

- [ ] 实施时先 `cargo doc --open` 验证 `rmcp 0.12` 的 `#[tool]` / `#[tool_router]` / `tool_handler` 宏语法（与 splitrail 0.8 可能不同）
- [ ] 决定 `get_file_operations` 是返回 0 还是从 reader 读——**spec 2 决定 0**，文档说明
- [ ] 写 `src/mcp/types.rs` 6 个 Request/Response 类型
- [ ] 写 6 个 tool handler
- [ ] 写 2 个 resource handler
- [ ] 写 `tests/mcp.rs` 黑盒测试
- [ ] 更新 README 加 MCP server 段 + 客户端配置示例
- [ ] 跑 `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`
- [ ] 端到端 smoke：用 mcp-cli 或 Claude Code 连接 `aum mcp`，调 `tools/list`，验证 6 个 tool 都在
- [ ] commit `feat: add aum mcp server with 6 tools and 2 resources`
