# Claude Code + Codex Usage Monitor 设计文档

**日期**: 2026-05-26
**项目**: usage-monitor (原 ollama-monitor)
**技术栈**: Rust + ratatui + tokio

## 1. 概述

`usage-monitor` 是一个基于终端的实时监控工具，用于展示 Claude Code 和 Codex 两个 AI 编程助手的 API usage 数据。它读取两款工具在本地自动写入的 JSONL/SQLite 文件，提取 token 消耗、费用和请求统计，并以 ratatui TUI 实时呈现。

**不在范围内**：Ollama 监控（官方无 SDK/API）、Opencode 监控（无本地 usage 数据）、代理/API 拦截、数据持久化。

## 2. 数据源

### 2.1 Claude Code

- **路径**: `~/.claude/projects/<project-path>/<uuid>.jsonl`
- **格式**: 每条 `type: "assistant"` 的 JSONL 条目包含 `message.usage.*`
- **关键字段**:
  - `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`
  - `model` (e.g., "claude-opus-4-7")
  - `timestamp`, `requestId`, `sessionId`, `cwd`
  - `cost_usd`（可选，如有则优先使用）

### 2.2 Codex

- **Rollout JSONL**: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`
  - `event_msg(type=token_count)` → `total_token_usage.{input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens}`
  - `turn_context` → `model`, `turn_id`
- **Session SQLite**: `~/.codex/state_*.sqlite` → `threads` 表
  - `model`, `tokens_used`, `thread_name`, `created_at`

### 2.3 数据获取方式

**读取本地文件，不拦截 API，不调用远程 API。** 每次启动全量扫描，之后增量轮询。

## 3. 架构设计

```
ollama-monitor (rename: usage-monitor)
├── state/
│   └── app_state.rs     # AppState, UsageRecord, SessionSummary, Tab, Platform
├── reader/
│   ├── mod.rs
│   ├── claude.rs        # 解析 ~/.claude/projects/**/*.jsonl
│   ├── codex.rs         # 解析 ~/.codex/sessions/**/rollout-*.jsonl
│   └── pricing.rs       # Anthropic + OpenAI 定价表
├── ui/
│   ├── mod.rs           # render() 入口
│   ├── tabs.rs          # 顶部 tab bar
│   ├── session_table.rs # 会话/项目汇总表 (40%)
│   ├── usage_table.rs   # 详细请求表 (55%)
│   └── status_bar.rs    # 底部状态栏
├── event/
│   └── event_loop.rs    # 键盘事件 + tick（基本不动）
├── cli.rs               # CLI 参数
└── main.rs              # tokio runtime 启动
```

**删除的模块**: `proxy/`、`ollama_client/`

## 4. 数据模型

```rust
enum Platform { ClaudeCode, Codex }
enum Tab { ClaudeCode, Codex }

struct UsageRecord {
    timestamp: DateTime<Utc>,
    platform: Platform,
    model: String,
    project: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    cost_usd: f64,
    service_tier: String,
    message_id: String,
    request_id: String,
}

struct SessionSummary {
    project: String,
    model: String,
    total_input: u64,
    total_output: u64,
    total_cache_read: u64,
    total_cache_creation: u64,
    total_cost: f64,
    request_count: u64,
    last_active: DateTime<Utc>,
}

struct AppState {
    claude_records: Vec<UsageRecord>,
    claude_sessions: Vec<SessionSummary>,
    claude_total_calls: usize,
    claude_total_cost: f64,
    codex_records: Vec<UsageRecord>,
    codex_sessions: Vec<SessionSummary>,
    codex_total_calls: usize,
    codex_total_cost: f64,
    active_tab: Tab,
    last_error: Option<String>,
}
```

## 5. Reader 设计

### 5.1 Trait

```rust
// 每个 reader 实现以下方法:
fn initial_scan(&mut self) -> Vec<UsageRecord>;  // 全量扫描
fn poll_delta(&mut self) -> Vec<UsageRecord>;     // 增量轮询
```

### 5.2 增量机制

维护 `HashMap<PathBuf, u64>` 记录每个文件的已读行数。每次 poll 时，只读取新增行。

### 5.3 Cost 计算

- Claude Code: 优先读 JSONL 的 `cost_usd` 字段，无则用 pricing table 计算
- Codex: 始终用 pricing table 计算

### 5.4 Polling 周期

默认 2 秒轮询一次文件变更（可通过 `--refresh` 配置）。

## 6. UI 布局

```
┌─ Usage Monitor ──── [Claude Code] [Codex] ──────────────────────────┐
│                                                                      │
│  Sessions          Input    Output   Cache    Cost    #              │
│  ├─ ollama-monitor 125k     48k      80k     $2.34   12             │
│  └─ elepay-admin   892k     310k     450k    $18.20  67             │
│                                                                      │
│  Recent Calls (27/100)   Time     Model          In    Out          │
│  ├─ 12:33:45              opus-4   14.2k          42   $0.12        │
│  └─ 12:33:32              opus-4   14.2k         402   $0.45        │
│                                                                      │
│  [Claude Code] ✓ 12 projects | 342 calls | $24.35   q:quit         │
└──────────────────────────────────────────────────────────────────────┘
```

- `Tab`/`←`/`→`: 切换 Claude Code / Codex
- `r`: 清空当前 tab 历史
- `q`/`Esc`: 退出
- `↑`/`↓`: 滚动详细表格

## 7. CLI 参数

```
usage-monitor [OPTIONS]

Options:
  --claude-path <PATH>   Claude Code 数据目录 [default: ~/.claude/projects]
  --codex-path <PATH>    Codex 数据目录 [default: ~/.codex]
  -r, --refresh <SECS>   轮询间隔秒数 [default: 2]
  -h, --help             打印帮助信息
```

## 8. 定价表

硬编码在 `pricing.rs` 中，按模型名模糊匹配：

| 模型 | Input ($/1M) | Output ($/1M) | Cache Read ($/1M) |
|------|-------------|---------------|-------------------|
| claude-opus-4 | 15.00 | 75.00 | 1.50 |
| claude-sonnet-4 | 3.00 | 15.00 | 0.30 |
| claude-haiku-4 | 1.00 | 5.00 | 0.10 |
| gpt-5.3-codex | 2.50 | 10.00 | 0.25 |
| gpt-5.4-mini | 0.15 | 0.60 | 0.015 |
| gpt-5.5 | 1.25 | 5.00 | 0.125 |

## 9. 错误处理

| 场景 | 处理 |
|------|------|
| 数据目录不存在 | 跳过该平台，日志警告 |
| JSONL 解析失败 | 跳过该行，继续处理 |
| SQLite 只读失败 | 跳过 SQLite 汇总，仅用 rollout 数据 |
| 未知模型 | cost 显示 $0.00，模型名显示 "unknown" |

## 10. 范围边界

**当前版本内**:
- Claude Code JSONL 读取
- Codex rollout JSONL 读取
- 双 tab 切换
- 实时 polling
- Cost 计算（pricing table + JSONL fallback）

**不在当前版本内**:
- Opencode 监控
- 数据持久化/导出
- 历史趋势图表
- 多实例/远程监控
- OpenTelemetry 集成
