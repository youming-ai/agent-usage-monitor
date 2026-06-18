# 设计文档：aum stats --json 子命令

**日期：** 2026-06-18
**状态：** 已批准
**作者：** opencode
**关联：** Spec 1/4（splitrail 学习后的改造）

## 概述

为 `agent-usage-monitor` 新增 `aum stats [--flags]` 子命令，输出 JSON 格式的用量报告，**不启动 TUI**。与 TUI 共享 reader + 平台注册表，但走独立路径：直接 `scan_all()`，聚合，序列化，stdout，退出。

## 目标

1. 给脚本、CI、未来的 MCP server 一个可程序化消费的用量数据出口
2. 不动 TUI 主循环，零风险
3. 数据完整度对齐 splitrail 的 `stats --json`
4. 退出干净（不调 quota 任务、不起事件循环、不挂后台任务）

## 非目标

- **不做**实时增量（TUI 的事）
- **不做**自动上传到云（splitrail 有，我们不要）
- **不引入** simd-json / lasso / 等性能 crate（spec 4 的事）
- **不动** `AppState` / `UsageSource` / reader 接口

## 架构设计

### 新增文件

1. `src/stats.rs` — `StatsReport` 数据结构 + `collect()` + `write_json()`
2. `tests/stats.rs` — 集成测试（临时目录注入合成数据）

### 修改文件

1. `src/cli.rs` — `Stats { ... }` Subcommand + `StatsArgs`
2. `src/main.rs` — 路由 `Commands::Stats` → `stats::collect()` + `write_json()`
3. `src/lib.rs` — `pub mod stats;`
4. `README.md` — 新增 "JSON stats" 段
5. `Cargo.toml` — 无变更（serde_json / chrono / tokio 已存在）

## 数据 schema

### 顶层对象

```json
{
  "generated_at": "2026-06-18T12:34:56Z",
  "platforms": {
    "claude_code": { /* PlatformReport */ },
    "codex":        { /* PlatformReport */ }
  },
  "totals": {
    "total_calls": 1234,
    "total_cost_usd": 12.34,
    "platforms_with_data": 5
  }
}
```

`platforms` 用 `BTreeMap` 而非 `HashMap`：JSON key 字典序输出，diff 友好。
`platforms_with_data` 计 `available == true` 的 platform 数（已安装但零用量也算）。

### PlatformReport

```json
{
  "available": true,
  "data_path": "/Users/me/.claude/projects",
  "totals": {
    "calls": 1000,
    "cost_usd": 10.5,
    "input_tokens": 1234567,
    "output_tokens": 234567,
    "cache_read_tokens": 890123,
    "cache_creation_tokens": 0
  },
  "models": {
    "claude-sonnet-4": {
      "calls": 800,
      "cost_usd": 8.4,
      "input_tokens": 1000000,
      "output_tokens": 200000,
      "cache_read_tokens": 700000,
      "cache_creation_tokens": 0,
      "sessions": 12
    }
  },
  "sessions": [
    {
      "session": "myapp abc12345",
      "calls": 50,
      "cost_usd": 1.2,
      "input_tokens": 60000,
      "output_tokens": 12000,
      "cache_read_tokens": 40000,
      "cache_creation_tokens": 0,
      "models": ["claude-sonnet-4"]
    }
  ],
  "dates": {
    "2026-06-15": {
      "calls": 200,
      "cost_usd": 2.1,
      "input_tokens": 250000,
      "output_tokens": 50000,
      "cache_read_tokens": 180000,
      "cache_creation_tokens": 0,
      "models": {"claude-sonnet-4": 200}
    }
  },
  "quota": { /* QuotaView, 仅 --include-quota 且仅 Claude/Codex */ }
}
```

### QuotaView（仅当 `--include-quota`）

```json
{
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
```

`fetched_at`：`QuotaInfo.fetched_at` 是 `Instant`，转字符串 `"YYYY-MM-DDTHH:MM:SS.fffZ"`。

## 实现细节

### `src/stats.rs`（核心）

```rust
use crate::cli::Cli;
use crate::platforms::{self, RegistryEntry};
use crate::quota::{self, QuotaInfo};
use crate::reader::UsageSource;
use crate::state::{Platform, UsageRecord};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

#[derive(Serialize)]
pub struct StatsReport {
    pub generated_at: DateTime<Utc>,
    pub platforms: BTreeMap<String, PlatformReport>,
    pub totals: Totals,
}

#[derive(Serialize, Default)]
pub struct Totals {
    pub total_calls: u64,
    pub total_cost_usd: f64,
    pub platforms_with_data: u32,
}

#[derive(Serialize, Default)]
pub struct PlatformTotals {
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

#[derive(Serialize, Default)]
pub struct ModelSummary {
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub sessions: u32,
}

#[derive(Serialize, Default)]
pub struct SessionSummaryView {
    pub session: String,
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub models: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct DateBucket {
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub models: BTreeMap<String, u64>,
}

#[derive(Serialize)]
pub struct PlatformReport {
    pub available: bool,
    pub data_path: PathBuf,
    pub totals: PlatformTotals,
    pub models: BTreeMap<String, ModelSummary>,
    pub sessions: Vec<SessionSummaryView>,
    pub dates: BTreeMap<String, DateBucket>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<QuotaView>,
}

#[derive(Serialize)]
pub struct QuotaView {
    pub tool_name: String,
    pub email: Option<String>,
    pub windows: Vec<quota::QuotaWindow>,
    /// quota 抓取时刻 (RFC3339, millis). Stats 一次性报告，所以用抓取结束时刻足够准确。
    pub fetched_at: String,
    pub error: Option<String>,
}

impl QuotaView {
    pub fn from_info(q: QuotaInfo, fetched_at: DateTime<Utc>) -> Self {
        Self {
            tool_name: q.tool_name,
            email: q.email,
            windows: q.windows,
            fetched_at: fetched_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            error: q.error.map(|e| e.display()),
        }
    }
}

#[derive(Default)]
pub struct Filters {
    pub platforms: Option<BTreeSet<String>>,
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
}

impl Filters {
    pub fn matches_platform(&self, key: &str) -> bool {
        match &self.platforms {
            None => true,
            Some(set) => set.contains(key),
        }
    }
    pub fn matches_date(&self, ts: DateTime<Utc>) -> bool {
        let d = ts.date_naive();
        if let Some(s) = self.since { if d < s { return false; } }
        if let Some(u) = self.until { if d > u { return false; } }
        true
    }
}

pub struct CollectOptions {
    pub include_quota: bool,
    pub filters: Filters,
}

pub async fn collect(cli: &Cli, opts: CollectOptions) -> Result<StatsReport> {
    let config = crate::config::load_config().unwrap_or_default();
    let paths = platforms::resolve_paths(cli, &config);

    // 第一遍：scan_all 收集记录（顺序，task::spawn_blocking 包 I/O）
    let mut entries: Vec<(String, Platform, PathBuf, Vec<UsageRecord>)> = Vec::new();
    for entry in platforms::entries() {
        let key = entry.config_key.trim_end_matches("_path").to_string();
        if !opts.filters.matches_platform(&key) { continue; }
        let path = paths.path_for(entry.tab);
        let mut reader = entry.build_reader(path.clone());
        let records = tokio::task::spawn_blocking(move || reader.scan_all())
            .await
            .unwrap_or_default();
        entries.push((key, entry.platform, path, records));
    }

    // Quota：仅在 --include-quota 时拉取。fetch() 是阻塞 HTTP，调完后取时间戳
    // 作为 fetched_at 写入 QuotaView（Instant 无法转 RFC3339，必须用 DateTime<Utc>）。
    let quota_fetched_at = if opts.include_quota {
        let q = tokio::task::spawn_blocking(|| {
            quota::fetchers().iter().map(|f| (f.platform(), f.fetch())).collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        let map: HashMap<Platform, QuotaView> = q
            .into_iter()
            .filter_map(|(p, q)| q.map(|qi| (p, QuotaView::from_info(qi, Utc::now()))))
            .collect();
        Some(map)
    } else {
        None
    };

    // 第二遍：聚合
    let mut report = StatsReport {
        generated_at: Utc::now(),
        platforms: BTreeMap::new(),
        totals: Totals::default(),
    };
    for (key, platform, path, records) in entries {
        let filtered: Vec<UsageRecord> = records
            .into_iter()
            .filter(|r| opts.filters.matches_date(r.timestamp))
            .collect();
        let available = path.exists();
        let pr = build_platform_report(platform, &path, available, filtered, quota_fetched_at.as_ref().and_then(|m| m.get(&platform).cloned()));
        if pr.available { report.totals.platforms_with_data += 1; }
        report.totals.total_calls += pr.totals.calls;
        report.totals.total_cost_usd += pr.totals.cost_usd;
        report.platforms.insert(key, pr);
    }
    Ok(report)
}

fn build_platform_report(
    _platform: Platform,
    path: &PathBuf,
    available: bool,
    records: Vec<UsageRecord>,
    quota: Option<QuotaView>,
) -> PlatformReport {
    let mut totals = PlatformTotals::default();
    let mut models: BTreeMap<String, ModelSummary> = BTreeMap::new();
    let mut session_map: BTreeMap<String, SessionSummaryView> = BTreeMap::new();
    let mut dates: BTreeMap<String, DateBucket> = BTreeMap::new();

    for r in records {
        totals.calls += 1;
        totals.cost_usd += r.cost_usd;
        totals.input_tokens += r.input_tokens;
        totals.output_tokens += r.output_tokens;
        totals.cache_read_tokens += r.cache_read_tokens;
        totals.cache_creation_tokens += r.cache_creation_tokens;

        let m = models.entry(r.model.clone()).or_default();
        m.calls += 1;
        m.cost_usd += r.cost_usd;
        m.input_tokens += r.input_tokens;
        m.output_tokens += r.output_tokens;
        m.cache_read_tokens += r.cache_read_tokens;
        m.cache_creation_tokens += r.cache_creation_tokens;

        let s = session_map.entry(r.session.clone()).or_insert_with(|| SessionSummaryView {
            session: r.session.clone(),
            ..Default::default()
        });
        s.calls += 1;
        s.cost_usd += r.cost_usd;
        s.input_tokens += r.input_tokens;
        s.output_tokens += r.output_tokens;
        s.cache_read_tokens += r.cache_read_tokens;
        s.cache_creation_tokens += r.cache_creation_tokens;
        if !s.models.contains(&r.model) { s.models.push(r.model.clone()); }

        let date_key = r.timestamp.format("%Y-%m-%d").to_string();
        let d = dates.entry(date_key).or_default();
        d.calls += 1;
        d.cost_usd += r.cost_usd;
        d.input_tokens += r.input_tokens;
        d.output_tokens += r.output_tokens;
        d.cache_read_tokens += r.cache_read_tokens;
        d.cache_creation_tokens += r.cache_creation_tokens;
        *d.models.entry(r.model).or_insert(0) += 1;
    }

    // 补 model.sessions
    for (model_name, m) in models.iter_mut() {
        m.sessions = session_map.values().filter(|s| s.models.contains(model_name)).count() as u32;
    }

    PlatformReport {
        available,
        data_path: path.clone(),
        totals,
        models,
        sessions: session_map.into_values().collect(),
        dates,
        quota,
    }
}

pub fn write_json<W: Write>(report: &StatsReport, pretty: bool, out: W) -> Result<()> {
    let writer = std::io::BufWriter::new(out);
    if pretty {
        serde_json::to_writer_pretty(writer, report).context("serialize pretty json")?;
    } else {
        serde_json::to_writer(writer, report).context("serialize compact json")?;
    }
    Ok(())
}

/// 解析 `--platform` 参数；支持三种 key：
/// - config_key 去 _path 后缀（"claude_code"）
/// - Tab 枚举 variant 名（"ClaudeCode"）
/// - log_name（"Claude Code"）
pub fn resolve_platform_filter(raw: &[String]) -> Result<BTreeSet<String>> {
    let mut set = BTreeSet::new();
    for r in raw {
        let normalized = r.trim();
        if normalized.is_empty() { continue; }
        for entry in platforms::entries() {
            let stripped = entry.config_key.trim_end_matches("_path");
            let tab_name = format!("{:?}", entry.tab);
            if stripped == normalized
                || tab_name == normalized
                || entry.log_name == normalized
            {
                set.insert(stripped.to_string());
                break;
            }
        }
        anyhow::bail!("unknown platform: {normalized}; run `aum config set` to list keys");
    }
    Ok(set)
}
```

### `src/cli.rs`（增量）

```rust
#[derive(Args, Debug)]
pub struct StatsArgs {
    /// 仅输出指定平台 (逗号分隔, 支持 config_key / Tab / log_name)
    #[arg(long, value_delimiter = ',')]
    pub platform: Vec<String>,

    /// 起始日期（含） YYYY-MM-DD
    #[arg(long)]
    pub since: Option<String>,

    /// 结束日期（含） YYYY-MM-DD
    #[arg(long)]
    pub until: Option<String>,

    /// 拉取 quota (Claude/Codex 需本地凭据)
    #[arg(long)]
    pub include_quota: bool,

    /// Pretty-print JSON (默认: stdout 是 TTY 时 pretty)
    #[arg(long)]
    pub pretty: bool,

    /// 显式 compact 输出 (反 --pretty)
    #[arg(long, conflicts_with = "pretty")]
    pub compact: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Update { /* 已有 */ },
    Config { /* 已有 */ },
    /// 输出 JSON 用量报告（不启动 TUI）
    Stats(StatsArgs),
}
```

### `src/main.rs`（增量）

```rust
Some(cli::Commands::Stats(args)) => {
    let since = args.since.as_deref().map(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose().context("--since must be YYYY-MM-DD")?;
    let until = args.until.as_deref().map(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose().context("--until must be YYYY-MM-DD")?;
    let platforms = if args.platform.is_empty() { None } else {
        Some(stats::resolve_platform_filter(&args.platform)?)
    };
    let filters = stats::Filters { platforms, since, until };
    let opts = stats::CollectOptions { include_quota: args.include_quota, filters };
    let report = stats::collect(&args, opts).await.context("failed to collect stats")?;
    let pretty = args.pretty || (!args.compact && std::io::stdout().is_terminal());
    let stdout = std::io::stdout().lock();
    stats::write_json(&report, pretty, stdout)?;
    return Ok(());
}
```

## 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| 数据驱动 | 独立 `stats::collect()`，不通过 AppState | 退出干净，无 TUI 副作用 |
| reader 调用 | `scan_all()` 一次性 | stats 是报告，不是监控；全量聚合更简单 |
| 执行模型 | 顺序调用 13 个 reader + `task::spawn_blocking` | 13 reader 串行 I/O 通常 < 1s，复杂度低；并行优化留给 spec 4 |
| 路径解析 | 复用 `platforms::resolve_paths(cli, config)` | 与 TUI 行为一致 |
| 未安装平台 | 输出 `available: false` + 空 totals | jq 可以 `.platforms.claude_code.available` 判断 |
| `platforms_with_data` | 计 `available == true` 数 | 已安装但零用量是有效状态 |
| 单 reader 失败 | 该平台 `available: false`，不阻塞其他 | 容错 |
| Quota | 仅 `--include-quota` 触发，复用 `quota::fetchers()` | 默认不发起网络 |
| TTY 检测 | `std::io::stdout().is_terminal()` | stdlib 稳定 (Rust 1.70+)，无新 dep |
| 序列化 | `serde_json` | 不引入 simd-json（spec 4） |
| 排序 | `BTreeMap` 而非 `HashMap` | JSON key 字典序输出，diff 友好 |
| 平台 key | `config_key` 去 `_path` 后缀 | 与 fixture 目录名一致 |
| 平台过滤接受形式 | config_key 缩 / Tab variant / log_name | 用户友好 |
| 错误处理 | `anyhow::Result`，路径不存在不 panic | 与 main.rs 风格一致 |
| 退出码 | 子命令完成 → 0 | Unix 约定 |

## 测试

### `tests/stats.rs`（新建）

| 测试 | 验证点 |
|---|---|
| `collect_with_synthetic_claude_records` | 临时目录塞 JSONL → totals/models/dates 数值正确 |
| `platform_filter_excludes_others` | `--platform claude_code` 只输出 1 个 platform key |
| `date_filter_excludes_older_records` | `--since 2026-06-15` 只聚合 ≥ 6/15 |
| `unavailable_platform_appears_with_available_false` | 不存在的路径 |
| `json_keys_are_stable_order` | 反序列化验证 `platforms` 的 key 顺序固定 |
| `quota_only_when_include_flag` | 默认无 `quota` 字段，`--include-quota` 时存在 |
| `sessions_aggregate_per_model_counts_correctly` | session 内多 model 时计数对 |
| `write_json_produces_valid_json` | serde_json::from_str 验证 |
| `platform_filter_accepts_log_name` | "Claude Code" 也能匹配 |
| `platform_filter_rejects_unknown` | "foo" → error |

### 单元测试 (`src/stats.rs` 内 `#[cfg(test)] mod tests`)

- `build_platform_report_aggregates_per_model_correctly`
- `build_platform_report_aggregates_per_session_correctly`
- `build_platform_report_aggregates_per_date_correctly`
- `filters_matches_platform_handles_none_and_set`
- `filters_matches_date_handles_since_until`
- `resolve_platform_filter_normalizes_three_forms`

## 向后兼容

- 现有 TUI 子命令路径 0 改动
- `aum` 无 subcommand 行为不变（启动 TUI）
- `aum update` / `aum config` 行为不变
- 配置文件 `~/.config/aum/config.toml` 0 改动
- CLI `--*-path` 行为不变（仍可用于 stats 子命令的路径覆盖）

## 依赖项

- **无新增**。`serde_json`、`chrono`、`tokio` 已存在
- `IsTerminal` 是 stdlib 稳定 API（Rust 1.70+）

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| 13 reader 串行 scan 慢 (>5s) | 测量后决定是否需要 rayon；spec 1 不预防性优化 |
| Quota 拉取阻塞 stats | `--include-quota` 走 `task::spawn_blocking` |
| JSON 体积大 | 文档提示 `--platform` / `--since` 过滤 |
| 路径存在但格式错误 | 沿用 reader 现有容错 |
| 解析 `--since` / `--until` 错误 | main.rs 显式 `parse_from_str`，错误带 context |

## 文档

- README.md 新增 `## JSON stats` 段：
  - 1 个 `aum stats --json | jq '.platforms.claude_code.totals.cost_usd'` 例子
  - 1 段 `## Typical pipelines`（GitHub Action / 成本监控）
- CHANGELOG：release-please 自动，conventional commit `feat: add aum stats --json subcommand`

## 实施顺序（writing-plans 阶段细化）

1. 加 `src/stats.rs` 骨架 + 数据结构
2. 加 `cli.rs` 子命令 + `main.rs` 路由
3. 单元测试 + 集成测试
4. README + cargo fmt + cargo clippy
5. 本地 `cargo run -- stats --platform claude_code` smoke test
6. commit + 收工

## 待办

- [ ] 写 `src/stats.rs`
- [ ] 扩展 `src/cli.rs` 加 `Stats` 子命令
- [ ] 改 `src/main.rs` 路由
- [ ] 写 `src/lib.rs` `pub mod stats;`
- [ ] 写 `tests/stats.rs`
- [ ] 更新 README.md
- [ ] 跑 `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`
- [ ] commit `feat: add aum stats --json subcommand`
