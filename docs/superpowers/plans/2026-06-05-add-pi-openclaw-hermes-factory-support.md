# 添加 pi、openclaw、hermes-agent、Factory AI 支持实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 agent-usage-monitor 添加对 pi、openclaw、hermes-agent、Factory AI 四个新 agent 的支持，并实现智能 Tab 显示（只显示已安装的 agent）

**Architecture:** 为每个新 agent 创建独立的 reader 模块，复用现有 JsonlReader trait（JSONL 格式）或实现新的 SQLite reader（hermes-agent）。扩展 Platform/Tab 枚举，添加目录检测逻辑实现智能 Tab 显示。

**Tech Stack:** Rust, ratatui, rusqlite, serde_json, chrono

---

## 文件结构

### 新增文件

| 文件 | 职责 |
|------|------|
| `src/reader/pi.rs` | pi agent 的 JSONL reader |
| `src/reader/openclaw.rs` | openclaw 的 JSONL reader |
| `src/reader/hermes.rs` | hermes-agent 的 SQLite reader |
| `src/reader/factory.rs` | Factory AI 的 JSONL reader |

### 修改文件

| 文件 | 修改内容 |
|------|----------|
| `src/state/app_state.rs` | 添加 Platform/Tab 枚举变体、available_tabs 字段、detect 函数 |
| `src/reader/mod.rs` | 注册新 reader 模块、实现 UsageSource trait |
| `src/cli.rs` | 添加 CLI 参数 |
| `src/config.rs` | 添加配置键 |
| `src/ui/tabs.rs` | 修改 tab_line() 支持动态 tab 列表 |
| `src/main.rs` | 启动时检测可用 tab、注册新 reader |

---

## Task 1: 扩展 Platform 和 Tab 枚举

**Files:**
- Modify: `src/state/app_state.rs`

- [ ] **Step 1: 添加 Platform 枚举变体**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    ClaudeCode,
    Codex,
    OpenCode,
    KimiCode,
    Pi,
    OpenClaw,
    Hermes,
    Factory,
}
```

- [ ] **Step 2: 添加 Tab 枚举变体**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    ClaudeCode,
    Codex,
    OpenCode,
    KimiCode,
    Pi,
    OpenClaw,
    Hermes,
    Factory,
}
```

- [ ] **Step 3: 添加 Tab 方法**

```rust
impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::ClaudeCode => "CLAUDE",
            Tab::Codex => "CODEX",
            Tab::OpenCode => "OPENCODE",
            Tab::KimiCode => "KIMI-CODE",
            Tab::Pi => "PI",
            Tab::OpenClaw => "OPENCLAW",
            Tab::Hermes => "HERMES",
            Tab::Factory => "FACTORY",
        }
    }

    pub fn primary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(255, 165, 0),
            Tab::Codex => ratatui::style::Color::Rgb(59, 130, 246),
            Tab::OpenCode => ratatui::style::Color::Rgb(16, 185, 129),
            Tab::KimiCode => ratatui::style::Color::Rgb(139, 92, 246),
            Tab::Pi => ratatui::style::Color::Rgb(236, 72, 153),
            Tab::OpenClaw => ratatui::style::Color::Rgb(234, 88, 12),
            Tab::Hermes => ratatui::style::Color::Rgb(168, 85, 247),
            Tab::Factory => ratatui::style::Color::Rgb(34, 197, 94),
        }
    }

    pub fn secondary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(255, 200, 100),
            Tab::Codex => ratatui::style::Color::Rgb(147, 197, 253),
            Tab::OpenCode => ratatui::style::Color::Rgb(110, 231, 183),
            Tab::KimiCode => ratatui::style::Color::Rgb(196, 181, 253),
            Tab::Pi => ratatui::style::Color::Rgb(249, 168, 212),
            Tab::OpenClaw => ratatui::style::Color::Rgb(253, 186, 116),
            Tab::Hermes => ratatui::style::Color::Rgb(216, 180, 254),
            Tab::Factory => ratatui::style::Color::Rgb(134, 239, 172),
        }
    }

    /// 所有 Tab 的列表
    pub fn all() -> &'static [Tab] {
        &[
            Tab::ClaudeCode,
            Tab::Codex,
            Tab::OpenCode,
            Tab::KimiCode,
            Tab::Pi,
            Tab::OpenClaw,
            Tab::Hermes,
            Tab::Factory,
        ]
    }

    /// 对应的默认数据路径
    pub fn default_path(self) -> std::path::PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        match self {
            Tab::ClaudeCode => home.join(".claude/projects"),
            Tab::Codex => home.join(".codex"),
            Tab::OpenCode => dirs::data_dir()
                .unwrap_or_else(|| home.join(".local/share"))
                .join("opencode"),
            Tab::KimiCode => home.join(".kimi-code"),
            Tab::Pi => home.join(".pi/agent/sessions"),
            Tab::OpenClaw => home.join(".openclaw/agents"),
            Tab::Hermes => home.join(".hermes"),
            Tab::Factory => home.join(".factory/projects"),
        }
    }

    /// 检测该 agent 是否已安装（配置目录是否存在）
    pub fn is_available(self) -> bool {
        let path = self.default_path();
        match self {
            Tab::Hermes => path.join("state.db").exists(),
            _ => path.exists(),
        }
    }
}
```

- [ ] **Step 4: 修改 Tab::next() 和 Tab::prev() 支持可用 tab 列表**

```rust
impl Tab {
    pub fn next_in(self, available: &[Tab]) -> Self {
        if available.is_empty() {
            return self;
        }
        let pos = available.iter().position(|&t| t == self).unwrap_or(0);
        available[(pos + 1) % available.len()]
    }

    pub fn prev_in(self, available: &[Tab]) -> Self {
        if available.is_empty() {
            return self;
        }
        let pos = available.iter().position(|&t| t == self).unwrap_or(0);
        available[(pos + available.len() - 1) % available.len()]
    }
}
```

- [ ] **Step 5: 添加 AppState 字段**

在 `AppState` 结构体中添加：
```rust
pub available_tabs: Vec<Tab>,
```

- [ ] **Step 6: 实现 detect_available_tabs()**

```rust
impl AppState {
    pub fn detect_available_tabs(&mut self) {
        self.available_tabs = Tab::all()
            .iter()
            .filter(|tab| tab.is_available())
            .copied()
            .collect();

        // 如果当前 active_tab 不在可用列表中，切换到第一个可用 tab
        if !self.available_tabs.contains(&self.active_tab) {
            self.active_tab = self.available_tabs.first().copied().unwrap_or(Tab::ClaudeCode);
        }
    }
}
```

- [ ] **Step 7: 添加状态字段**

在 `AppState` 中为每个新 agent 添加：
```rust
// pi
pub pi_records: VecDeque<UsageRecord>,
pub pi_sessions: HashMap<String, SessionSummary>,
pub pi_total_calls: usize,
pub pi_total_cost: f64,
pub pi_quota: Option<QuotaInfo>,
pub pi_max_records: usize,

// openclaw
pub openclaw_records: VecDeque<UsageRecord>,
pub openclaw_sessions: HashMap<String, SessionSummary>,
pub openclaw_total_calls: usize,
pub openclaw_total_cost: f64,
pub openclaw_quota: Option<QuotaInfo>,
pub openclaw_max_records: usize,

// hermes
pub hermes_records: VecDeque<UsageRecord>,
pub hermes_sessions: HashMap<String, SessionSummary>,
pub hermes_total_calls: usize,
pub hermes_total_cost: f64,
pub hermes_quota: Option<QuotaInfo>,
pub hermes_max_records: usize,

// factory
pub factory_records: VecDeque<UsageRecord>,
pub factory_sessions: HashMap<String, SessionSummary>,
pub factory_total_calls: usize,
pub factory_total_cost: f64,
pub factory_quota: Option<QuotaInfo>,
pub factory_max_records: usize,
```

- [ ] **Step 8: 添加 add_records 分支**

在 `add_records()` 方法中添加新 platform 的分支。

- [ ] **Step 9: 添加 clear 方法**

```rust
pub fn clear_pi(&mut self) { ... }
pub fn clear_openclaw(&mut self) { ... }
pub fn clear_hermes(&mut self) { ... }
pub fn clear_factory(&mut self) { ... }
```

- [ ] **Step 10: 运行测试**

```bash
cargo test
```

- [ ] **Step 11: 提交**

```bash
git add src/state/app_state.rs
git commit -m "feat: extend Platform/Tab enums for pi, openclaw, hermes, factory"
```

---

## Task 2: 实现 pi Reader

**Files:**
- Create: `src/reader/pi.rs`
- Modify: `src/reader/mod.rs`

- [ ] **Step 1: 创建 pi.rs 基本结构**

```rust
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use super::find_recursive;
use super::jsonl_reader::JsonlReader;

pub struct PiReader {
    data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
}

impl PiReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            file_positions: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pi/agent/sessions")
    }
}

impl JsonlReader for PiReader {
    fn file_positions(&mut self) -> &mut HashMap<PathBuf, u64> {
        &mut self.file_positions
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false)
            });
        }
        files
    }

    fn parse_line(&self, line: &str) -> Option<UsageRecord> {
        parse_pi_line(line)
    }
}
```

- [ ] **Step 2: 实现 parse_pi_line()**

```rust
fn parse_pi_line(line: &str) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    // pi 的 JSONL 格式：type == "message"，message.role == "assistant"
    if v.get("type")?.as_str()? != "message" {
        return None;
    }

    let message = v.get("message")?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }

    let usage = message.get("usage")?;

    let input = usage.get("input")?.as_u64()?;
    let output = usage.get("output")?.as_u64().unwrap_or(0);
    let cache_read = usage
        .get("cacheRead")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write = usage
        .get("cacheWrite")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input == 0 && output == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let dir = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .map(crate::reader::basename)
        .unwrap_or_else(|| "unknown".to_string());
    let session_id = v.get("id").and_then(|s| s.as_str()).unwrap_or("");
    let session = crate::reader::session_label(&dir, session_id);

    let cost_usd = usage
        .get("cost")
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    Some(UsageRecord {
        timestamp,
        platform: Platform::Pi,
        model,
        session,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        cost_usd,
    })
}
```

- [ ] **Step 3: 添加单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn sample_jsonl() -> String {
        [
            r#"{"type":"session","version":3,"id":"abc123","timestamp":"2026-06-05T10:00:00Z","cwd":"/Users/me/project"}"#,
            r#"{"type":"message","id":"msg1","parentId":null,"timestamp":"2026-06-05T10:00:01Z","message":{"role":"user","content":"hello"}}"#,
            r#"{"type":"message","id":"msg2","parentId":"msg1","timestamp":"2026-06-05T10:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"Hi!"}],"provider":"anthropic","model":"claude-sonnet-4-5","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"totalTokens":165,"cost":{"input":0.015,"output":0.0075,"cacheRead":0.00015,"cacheWrite":0.0001875,"total":0.0228375}},"stopReason":"stop","timestamp":1717584002000}}"#,
            "",
        ]
        .join("\n")
    }

    fn write_file(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn parses_assistant_messages() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = PiReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "claude-sonnet-4-5");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].output_tokens, 50);
        assert_eq!(records[0].cache_read_tokens, 10);
        assert_eq!(records[0].cache_creation_tokens, 5);
        assert_eq!(records[0].platform, Platform::Pi);
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = PiReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = PiReader::new(PathBuf::from("/nonexistent/pi"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }
}
```

- [ ] **Step 4: 在 mod.rs 中注册**

在 `src/reader/mod.rs` 中添加：
```rust
pub mod pi;
```

并添加 UsageSource 实现：
```rust
impl UsageSource for pi::PiReader {
    fn platform(&self) -> Platform {
        Platform::Pi
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        JsonlReader::scan_all(self)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        JsonlReader::poll_delta(self)
    }
}
```

- [ ] **Step 5: 运行测试**

```bash
cargo test reader::pi
```

- [ ] **Step 6: 提交**

```bash
git add src/reader/pi.rs src/reader/mod.rs
git commit -m "feat: add PiReader for pi agent"
```

---

## Task 3: 实现 openclaw Reader

**Files:**
- Create: `src/reader/openclaw.rs`
- Modify: `src/reader/mod.rs`

- [ ] **Step 1: 创建 openclaw.rs 基本结构**

```rust
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use super::find_recursive;
use super::jsonl_reader::JsonlReader;

pub struct OpenClawReader {
    data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
}

impl OpenClawReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            file_positions: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".openclaw/agents")
    }
}

impl JsonlReader for OpenClawReader {
    fn file_positions(&mut self) -> &mut HashMap<PathBuf, u64> {
        &mut self.file_positions
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false)
            });
        }
        files
    }

    fn parse_line(&self, line: &str) -> Option<UsageRecord> {
        parse_openclaw_line(line)
    }
}
```

- [ ] **Step 2: 实现 parse_openclaw_line()**

openclaw 的 JSONL 格式与 pi 类似：

```rust
fn parse_openclaw_line(line: &str) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    if v.get("type")?.as_str()? != "message" {
        return None;
    }

    let message = v.get("message")?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }

    let usage = message.get("usage")?;

    let input = usage.get("input")?.as_u64()?;
    let output = usage.get("output")?.as_u64().unwrap_or(0);
    let cache_read = usage
        .get("cacheRead")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write = usage
        .get("cacheWrite")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input == 0 && output == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let dir = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .map(crate::reader::basename)
        .unwrap_or_else(|| "unknown".to_string());
    let session_id = v.get("sessionId").and_then(|s| s.as_str()).unwrap_or("");
    let session = crate::reader::session_label(&dir, session_id);

    let cost_usd = usage
        .get("cost")
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    Some(UsageRecord {
        timestamp,
        platform: Platform::OpenClaw,
        model,
        session,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        cost_usd,
    })
}
```

- [ ] **Step 3: 添加单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn sample_jsonl() -> String {
        [
            r#"{"type":"session","id":"abc123","timestamp":"2026-06-05T10:00:00Z","cwd":"/Users/me/project"}"#,
            r#"{"type":"message","id":"msg1","timestamp":"2026-06-05T10:00:01Z","message":{"role":"assistant","model":"claude-opus-4","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5},"cost":{"total":0.02}}}"#,
            "",
        ]
        .join("\n")
    }

    #[test]
    fn parses_assistant_messages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(sample_jsonl().as_bytes()).unwrap();

        let mut reader = OpenClawReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "claude-opus-4");
        assert_eq!(records[0].platform, Platform::OpenClaw);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = OpenClawReader::new(PathBuf::from("/nonexistent/openclaw"));
        assert!(reader.scan_all().is_empty());
    }
}
```

- [ ] **Step 4: 在 mod.rs 中注册**

```rust
pub mod openclaw;

impl UsageSource for openclaw::OpenClawReader {
    fn platform(&self) -> Platform {
        Platform::OpenClaw
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        JsonlReader::scan_all(self)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        JsonlReader::poll_delta(self)
    }
}
```

- [ ] **Step 5: 运行测试**

```bash
cargo test reader::openclaw
```

- [ ] **Step 6: 提交**

```bash
git add src/reader/openclaw.rs src/reader/mod.rs
git commit -m "feat: add OpenClawReader for openclaw agent"
```

---

## Task 4: 实现 hermes-agent Reader (SQLite)

**Files:**
- Create: `src/reader/hermes.rs`
- Modify: `src/reader/mod.rs`

- [ ] **Step 1: 创建 hermes.rs 基本结构**

```rust
use crate::reader::{basename, session_label, UsageSource};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use tracing::warn;

pub struct HermesReader {
    conn: Option<Connection>,
    cursor: i64,
}

impl HermesReader {
    pub fn new(data_dir: PathBuf) -> Self {
        let db_path = data_dir.join("state.db");
        let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
        Self { conn, cursor: 0 }
    }

    #[cfg(test)]
    fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Some(conn),
            cursor: 0,
        }
    }

    fn query_since(&mut self, cursor: i64) -> Vec<UsageRecord> {
        let Some(conn) = self.conn.as_ref() else {
            return Vec::new();
        };

        let mut stmt = match conn.prepare(
            "SELECT id, model, started_at, ended_at, input_tokens, output_tokens,
                    cache_read_tokens, cache_write_tokens, estimated_cost_usd, cwd
             FROM sessions
             WHERE started_at > ?1
               AND (input_tokens > 0 OR output_tokens > 0)
             ORDER BY started_at",
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("hermes: failed to prepare usage query: {e}");
                return Vec::new();
            }
        };

        let rows = stmt.query_map([cursor], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, Option<f64>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, Option<f64>>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        });

        let mut records = Vec::new();
        let mut max_seen = cursor;

        if let Ok(rows) = rows {
            for row in rows.flatten() {
                let (id, model, started_at, _ended_at, input, output, cache_read, cache_write, cost, cwd) = row;

                let timestamp_ms = (started_at * 1000.0) as i64;
                if timestamp_ms > max_seen {
                    max_seen = timestamp_ms;
                }

                let timestamp = Utc.timestamp_millis_opt(timestamp_ms).single()?;
                let model = model.unwrap_or_else(|| "unknown".to_string());
                let dir = cwd
                    .as_deref()
                    .map(basename)
                    .unwrap_or_else(|| "unknown".to_string());
                let session = session_label(&dir, &id);
                let cost_usd = cost.unwrap_or(0.0);

                records.push(UsageRecord {
                    timestamp,
                    platform: Platform::Hermes,
                    model,
                    session,
                    input_tokens: input as u64,
                    output_tokens: output as u64,
                    cache_read_tokens: cache_read as u64,
                    cache_creation_tokens: cache_write as u64,
                    cost_usd,
                });
            }
        }

        self.cursor = max_seen;
        records
    }
}

impl UsageSource for HermesReader {
    fn platform(&self) -> Platform {
        Platform::Hermes
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.cursor = 0;
        self.query_since(0)
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let cursor = self.cursor;
        self.query_since(cursor)
    }
}
```

- [ ] **Step 2: 添加单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                model TEXT,
                started_at REAL NOT NULL,
                ended_at REAL,
                input_tokens INTEGER DEFAULT 0,
                output_tokens INTEGER DEFAULT 0,
                cache_read_tokens INTEGER DEFAULT 0,
                cache_write_tokens INTEGER DEFAULT 0,
                estimated_cost_usd REAL,
                cwd TEXT
            );",
        )
        .unwrap();
        conn
    }

    fn insert_session(conn: &Connection, id: &str, model: &str, started_at: f64, input: i64, output: i64) {
        conn.execute(
            "INSERT INTO sessions (id, model, started_at, input_tokens, output_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, model, started_at, input, output],
        )
        .unwrap();
    }

    #[test]
    fn scan_all_parses_sessions() {
        let conn = setup();
        insert_session(&conn, "s1", "claude-sonnet-4-5", 1000.0, 100, 50);
        insert_session(&conn, "s2", "gpt-4o", 2000.0, 200, 80);
        let mut reader = HermesReader::from_connection(conn);

        let records = reader.scan_all();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].model, "claude-sonnet-4-5");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].platform, Platform::Hermes);
    }

    #[test]
    fn poll_delta_returns_only_new_sessions() {
        let conn = setup();
        insert_session(&conn, "s1", "claude-sonnet-4-5", 1000.0, 100, 50);
        let mut reader = HermesReader::from_connection(conn);

        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);

        let conn2 = reader.conn.as_ref().unwrap();
        insert_session(conn2, "s2", "gpt-4o", 2000.0, 200, 80);

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].model, "gpt-4o");
    }

    #[test]
    fn missing_db_yields_empty() {
        let mut reader = HermesReader::new(PathBuf::from("/nonexistent/hermes"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }
}
```

- [ ] **Step 3: 在 mod.rs 中注册**

```rust
pub mod hermes;

impl UsageSource for hermes::HermesReader {
    fn platform(&self) -> Platform {
        Platform::Hermes
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        UsageSource::scan_all(self)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        UsageSource::poll_delta(self)
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cargo test reader::hermes
```

- [ ] **Step 5: 提交**

```bash
git add src/reader/hermes.rs src/reader/mod.rs
git commit -m "feat: add HermesReader for hermes-agent (SQLite)"
```

---

## Task 5: 实现 Factory AI Reader

**Files:**
- Create: `src/reader/factory.rs`
- Modify: `src/reader/mod.rs`

- [ ] **Step 1: 创建 factory.rs 基本结构**

```rust
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use super::find_recursive;
use super::jsonl_reader::JsonlReader;

pub struct FactoryReader {
    data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
}

impl FactoryReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            file_positions: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".factory/projects")
    }
}

impl JsonlReader for FactoryReader {
    fn file_positions(&mut self) -> &mut HashMap<PathBuf, u64> {
        &mut self.file_positions
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false)
            });
        }
        files
    }

    fn parse_line(&self, line: &str) -> Option<UsageRecord> {
        parse_factory_line(line)
    }
}
```

- [ ] **Step 2: 实现 parse_factory_line()**

Factory AI 使用 snake_case 字段名：

```rust
fn parse_factory_line(line: &str) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    if v.get("type")?.as_str()? != "message" {
        return None;
    }

    let message = v.get("message")?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }

    let usage = message.get("usage")?;

    let input = usage.get("input_tokens")?.as_u64()?;
    let output = usage.get("output_tokens")?.as_u64().unwrap_or(0);
    let cache_read = usage
        .get("cache_read_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write = usage
        .get("cache_write_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input == 0 && output == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let dir = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .map(crate::reader::basename)
        .unwrap_or_else(|| "unknown".to_string());
    let session_id = v.get("sessionId").and_then(|s| s.as_str()).unwrap_or("");
    let session = crate::reader::session_label(&dir, session_id);

    let cost_usd = 0.0; // Factory AI 不在 JSONL 中提供 cost

    Some(UsageRecord {
        timestamp,
        platform: Platform::Factory,
        model,
        session,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        cost_usd,
    })
}
```

- [ ] **Step 3: 添加单元测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn sample_jsonl() -> String {
        [
            r#"{"type":"session","id":"abc123","timestamp":"2026-06-05T10:00:00Z","cwd":"/Users/me/project"}"#,
            r#"{"type":"message","id":"msg1","timestamp":"2026-06-05T10:00:01Z","message":{"role":"assistant","model":"claude-sonnet-4-5","usage":{"input_tokens":100,"output_tokens":50,"cache_read_tokens":10,"cache_write_tokens":5}}}"#,
            "",
        ]
        .join("\n")
    }

    #[test]
    fn parses_assistant_messages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(sample_jsonl().as_bytes()).unwrap();

        let mut reader = FactoryReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "claude-sonnet-4-5");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].platform, Platform::Factory);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = FactoryReader::new(PathBuf::from("/nonexistent/factory"));
        assert!(reader.scan_all().is_empty());
    }
}
```

- [ ] **Step 4: 在 mod.rs 中注册**

```rust
pub mod factory;

impl UsageSource for factory::FactoryReader {
    fn platform(&self) -> Platform {
        Platform::Factory
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        JsonlReader::scan_all(self)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        JsonlReader::poll_delta(self)
    }
}
```

- [ ] **Step 5: 运行测试**

```bash
cargo test reader::factory
```

- [ ] **Step 6: 提交**

```bash
git add src/reader/factory.rs src/reader/mod.rs
git commit -m "feat: add FactoryReader for Factory AI"
```

---

## Task 6: 更新 UI 支持智能 Tab 显示

**Files:**
- Modify: `src/ui/tabs.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: 修改 tab_line() 支持动态 tab 列表**

```rust
pub fn tab_line(active: Tab, available: &[Tab]) -> Paragraph<'static> {
    let mut spans = Vec::new();
    for (i, &tab) in available.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(tab_span(tab, active == tab));
    }
    Paragraph::new(Line::from(spans))
}
```

- [ ] **Step 2: 更新 ui/mod.rs 中的 tab_line 调用**

```rust
// 在 render() 函数中
let tab_line = tabs::tab_line(state.active_tab, &state.available_tabs);
```

- [ ] **Step 3: 更新 active_tab 的 next/prev 调用**

在 `main.rs` 中：
```rust
KeyCode::Tab | KeyCode::Right => {
    if let Ok(mut state) = app_state.write() {
        state.active_tab = state.active_tab.next_in(&state.available_tabs);
    }
}
KeyCode::Left => {
    if let Ok(mut state) = app_state.write() {
        state.active_tab = state.active_tab.prev_in(&state.available_tabs);
    }
}
```

- [ ] **Step 4: 更新 clear 操作**

```rust
KeyCode::Char('r') => {
    if let Ok(mut state) = app_state.write() {
        match state.active_tab {
            state::Tab::ClaudeCode => state.clear_claude(),
            state::Tab::Codex => state.clear_codex(),
            state::Tab::OpenCode => state.clear_opencode(),
            state::Tab::KimiCode => state.clear_kimi_code(),
            state::Tab::Pi => state.clear_pi(),
            state::Tab::OpenClaw => state.clear_openclaw(),
            state::Tab::Hermes => state.clear_hermes(),
            state::Tab::Factory => state.clear_factory(),
        }
    }
}
```

- [ ] **Step 5: 运行测试**

```bash
cargo test
```

- [ ] **Step 6: 提交**

```bash
git add src/ui/tabs.rs src/ui/mod.rs src/main.rs
git commit -m "feat: implement smart tab display based on installed agents"
```

---

## Task 7: 添加 CLI 参数和配置支持

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/config.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: 添加 CLI 参数**

在 `src/cli.rs` 中：
```rust
#[derive(Parser)]
pub struct Cli {
    // ... existing fields ...

    #[arg(long, help = "Path to pi data directory")]
    pub pi_path: Option<PathBuf>,

    #[arg(long, help = "Path to openclaw data directory")]
    pub openclaw_path: Option<PathBuf>,

    #[arg(long, help = "Path to hermes-agent data directory")]
    pub hermes_path: Option<PathBuf>,

    #[arg(long, help = "Path to Factory AI data directory")]
    pub factory_path: Option<PathBuf>,
}
```

- [ ] **Step 2: 添加配置键**

在 `src/config.rs` 中：
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    // ... existing fields ...

    #[serde(default = "default_pi_path")]
    pub pi_path: PathBuf,

    #[serde(default = "default_openclaw_path")]
    pub openclaw_path: PathBuf,

    #[serde(default = "default_hermes_path")]
    pub hermes_path: PathBuf,

    #[serde(default = "default_factory_path")]
    pub factory_path: PathBuf,
}

fn default_pi_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi/agent/sessions")
}

fn default_openclaw_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".openclaw/agents")
}

fn default_hermes_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hermes")
}

fn default_factory_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".factory/projects")
}
```

- [ ] **Step 3: 更新 main.rs 中的路径合并逻辑**

```rust
let pi_path = args.pi_path.unwrap_or(config.pi_path);
let openclaw_path = args.openclaw_path.unwrap_or(config.openclaw_path);
let hermes_path = args.hermes_path.unwrap_or(config.hermes_path);
let factory_path = args.factory_path.unwrap_or(config.factory_path);
```

- [ ] **Step 4: 注册新 reader**

```rust
let sources: Vec<Arc<std::sync::Mutex<Box<dyn UsageSource>>>> = vec![
    // ... existing sources ...
    Arc::new(std::sync::Mutex::new(
        Box::new(reader::pi::PiReader::new(pi_path.clone())) as Box<dyn UsageSource>,
    )),
    Arc::new(std::sync::Mutex::new(
        Box::new(reader::openclaw::OpenClawReader::new(openclaw_path.clone())) as Box<dyn UsageSource>,
    )),
    Arc::new(std::sync::Mutex::new(
        Box::new(reader::hermes::HermesReader::new(hermes_path.clone())) as Box<dyn UsageSource>,
    )),
    Arc::new(std::sync::Mutex::new(
        Box::new(reader::factory::FactoryReader::new(factory_path.clone())) as Box<dyn UsageSource>,
    )),
];
```

- [ ] **Step 5: 启动时检测可用 tab**

```rust
let app_state = Arc::new(RwLock::new(AppState::with_capacity(config.max_records)));
{
    let mut state = app_state.write().unwrap();
    state.detect_available_tabs();
}
```

- [ ] **Step 6: 运行测试**

```bash
cargo test
```

- [ ] **Step 7: 提交**

```bash
git add src/cli.rs src/config.rs src/main.rs
git commit -m "feat: add CLI args and config for pi, openclaw, hermes, factory"
```

---

## Task 8: 更新 UI 渲染支持新 Tab

**Files:**
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: 添加新 tab 的渲染分支**

在 `ui/mod.rs` 的 `render()` 函数中：

```rust
let (records, sessions, total_calls, total_cost, quota) = match state.active_tab {
    crate::state::Tab::ClaudeCode => (
        &state.claude_records,
        &state.claude_sessions,
        state.claude_total_calls,
        state.claude_total_cost,
        state.claude_quota.as_ref(),
    ),
    crate::state::Tab::Codex => (
        &state.codex_records,
        &state.codex_sessions,
        state.codex_total_calls,
        state.codex_total_cost,
        state.codex_quota.as_ref(),
    ),
    crate::state::Tab::OpenCode => (
        &state.opencode_records,
        &state.opencode_sessions,
        state.opencode_total_calls,
        state.opencode_total_cost,
        state.opencode_quota.as_ref(),
    ),
    crate::state::Tab::KimiCode => (
        &state.kimi_code_records,
        &state.kimi_code_sessions,
        state.kimi_code_total_calls,
        state.kimi_code_total_cost,
        state.kimi_code_quota.as_ref(),
    ),
    crate::state::Tab::Pi => (
        &state.pi_records,
        &state.pi_sessions,
        state.pi_total_calls,
        state.pi_total_cost,
        state.pi_quota.as_ref(),
    ),
    crate::state::Tab::OpenClaw => (
        &state.openclaw_records,
        &state.openclaw_sessions,
        state.openclaw_total_calls,
        state.openclaw_total_cost,
        state.openclaw_quota.as_ref(),
    ),
    crate::state::Tab::Hermes => (
        &state.hermes_records,
        &state.hermes_sessions,
        state.hermes_total_calls,
        state.hermes_total_cost,
        state.hermes_quota.as_ref(),
    ),
    crate::state::Tab::Factory => (
        &state.factory_records,
        &state.factory_sessions,
        state.factory_total_calls,
        state.factory_total_cost,
        state.factory_quota.as_ref(),
    ),
};
```

- [ ] **Step 2: 更新 quota_bar 调用**

```rust
let quota_source = match state.active_tab {
    crate::state::Tab::ClaudeCode | crate::state::Tab::Codex => {
        quota_bar::quota_panel(state.active_tab, quota)
    }
    _ => quota_bar::no_quota_source(),
};
```

- [ ] **Step 3: 运行测试**

```bash
cargo test
```

- [ ] **Step 4: 提交**

```bash
git add src/ui/mod.rs
git commit -m "feat: update UI rendering for new agent tabs"
```

---

## Task 9: 更新配置命令支持新键

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: 更新 handle_config 中的 match**

```rust
match key.as_str() {
    "claude_path" => config.claude_path = std::path::PathBuf::from(value),
    "codex_path" => config.codex_path = std::path::PathBuf::from(value),
    "opencode_path" => config.opencode_path = std::path::PathBuf::from(value),
    "kimi_code_path" => config.kimi_code_path = std::path::PathBuf::from(value),
    "pi_path" => config.pi_path = std::path::PathBuf::from(value),
    "openclaw_path" => config.openclaw_path = std::path::PathBuf::from(value),
    "hermes_path" => config.hermes_path = std::path::PathBuf::from(value),
    "factory_path" => config.factory_path = std::path::PathBuf::from(value),
    "refresh" => config.refresh = value.parse().map_err(|e: std::num::ParseIntError| e.to_string())?,
    "max_records" => config.max_records = value.parse().map_err(|e: std::num::ParseIntError| e.to_string())?,
    _ => {
        eprintln!("Unknown configuration key: {}", key);
        eprintln!("Available keys: claude_path, codex_path, opencode_path, kimi_code_path, pi_path, openclaw_path, hermes_path, factory_path, refresh, max_records");
        std::process::exit(1);
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cargo test
```

- [ ] **Step 3: 提交**

```bash
git add src/main.rs
git commit -m "feat: update config command for new agent paths"
```

---

## Task 10: 最终集成测试

- [ ] **Step 1: 运行完整测试套件**

```bash
cargo test
```

- [ ] **Step 2: 检查编译**

```bash
cargo build
```

- [ ] **Step 3: 手动测试**

```bash
cargo run
```

验证：
1. 只显示已安装的 agent tab
2. Tab 切换正常工作
3. 每个 agent 的数据正确显示

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "feat: complete integration for pi, openclaw, hermes, factory support"
```
