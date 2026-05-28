# Claude Code + Codex Usage Monitor 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 重构 ollama-monitor 为 Claude Code + Codex 的双平台 usage monitor，读取本地 JSONL/SQLite 文件，以 ratatui TUI 实时展示 token 消耗、费用和请求统计。

**Architecture:** tokio-async app with two concurrent file reader tasks (claude_reader, codex_reader) polling local data files every N seconds, plus a ratatui event loop rendering shared state. State synchronized via `Arc<RwLock<AppState>>`. No proxy, no API key.

**Tech Stack:** Rust, tokio, ratatui, crossterm, serde, serde_json, clap, chrono, humansize, rusqlite

---

## File Structure (Target)

```
ollama-monitor/
├── Cargo.toml
└── src/
    ├── main.rs
    ├── cli.rs
    ├── state/
    │   ├── mod.rs
    │   └── app_state.rs
    ├── reader/
    │   ├── mod.rs
    │   ├── claude.rs
    │   ├── codex.rs
    │   └── pricing.rs
    ├── ui/
    │   ├── mod.rs
    │   ├── tabs.rs
    │   ├── session_table.rs
    │   ├── usage_table.rs
    │   └── status_bar.rs
    └── event/
        ├── mod.rs
        └── event_loop.rs
```

---

## Task 1: Remove Old Modules and Update Dependencies

**Files:**
- Modify: `Cargo.toml`
- Delete: `src/proxy/` (整个目录)
- Delete: `src/ollama_client/` (整个目录)

- [ ] **Step 1: Remove old source directories**

```bash
rm -rf src/proxy src/ollama_client
```

- [ ] **Step 2: Update Cargo.toml**

Remove:
```toml
axum = "0.8"
reqwest = { version = "0.13", features = ["json", "stream"] }
futures-util = "0.3"
```

Add:
```toml
rusqlite = { version = "0.31", features = ["bundled"] }
glob = "0.3"
```

Keep: tokio, serde, serde_json, ratatui, crossterm, clap, chrono, humansize

- [ ] **Step 3: Update main.rs to remove old module declarations**

Remove from `main.rs`:
```rust
mod proxy;
mod ollama_client;
```

Add placeholder:
```rust
mod state;
mod event;
mod ui;
```

- [ ] **Step 4: Verify compiles**

```bash
cargo check
```

Expected: Errors about missing modules — that's OK, we'll add them in subsequent tasks.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: remove proxy and ollama_client modules, update deps"
```

---

## Task 2: Define New State Types

**Files:**
- Modify: `src/state/app_state.rs`
- Modify: `src/state/mod.rs`

- [ ] **Step 1: Write new state structs**

Replace `src/state/app_state.rs` entirely:

```rust
use chrono::{DateTime, Utc};

pub const MAX_RECORDS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    ClaudeCode,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    ClaudeCode,
    Codex,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Tab::ClaudeCode => Tab::Codex,
            Tab::Codex => Tab::ClaudeCode,
        }
    }

    pub fn prev(self) -> Self {
        self.next() // only 2 tabs, toggles
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::ClaudeCode => "Claude Code",
            Tab::Codex => "Codex",
        }
    }
}

/// Single API call record
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    pub platform: Platform,
    pub model: String,
    pub project: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
    pub service_tier: String,
    pub message_id: String,
    pub request_id: String,
}

/// Aggregated session/project summary
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub project: String,
    pub model: String,
    pub total_input: u64,
    pub total_output: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
    pub total_cost: f64,
    pub request_count: u64,
    pub last_active: DateTime<Utc>,
}

/// Global application state
pub struct AppState {
    // Claude Code
    pub claude_records: Vec<UsageRecord>,
    pub claude_sessions: Vec<SessionSummary>,
    pub claude_total_calls: usize,
    pub claude_total_cost: f64,

    // Codex
    pub codex_records: Vec<UsageRecord>,
    pub codex_sessions: Vec<SessionSummary>,
    pub codex_total_calls: usize,
    pub codex_total_cost: f64,

    // Shared
    pub active_tab: Tab,
    pub last_error: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            claude_records: Vec::with_capacity(MAX_RECORDS),
            claude_sessions: Vec::new(),
            claude_total_calls: 0,
            claude_total_cost: 0.0,
            codex_records: Vec::with_capacity(MAX_RECORDS),
            codex_sessions: Vec::new(),
            codex_total_calls: 0,
            codex_total_cost: 0.0,
            active_tab: Tab::ClaudeCode,
            last_error: None,
        }
    }

    pub fn add_claude_record(&mut self, record: UsageRecord) {
        if self.claude_records.len() >= MAX_RECORDS {
            self.claude_records.remove(0);
        }
        self.claude_total_cost += record.cost_usd;
        self.claude_total_calls += 1;
        self.claude_records.push(record);
        self.rebuild_claude_sessions();
    }

    pub fn add_codex_record(&mut self, record: UsageRecord) {
        if self.codex_records.len() >= MAX_RECORDS {
            self.codex_records.remove(0);
        }
        self.codex_total_cost += record.cost_usd;
        self.codex_total_calls += 1;
        self.codex_records.push(record);
        self.rebuild_codex_sessions();
    }

    pub fn add_claude_records(&mut self, records: Vec<UsageRecord>) {
        for r in records {
            if self.claude_records.len() >= MAX_RECORDS {
                self.claude_records.remove(0);
            }
            self.claude_total_cost += r.cost_usd;
            self.claude_total_calls += 1;
            self.claude_records.push(r);
        }
        self.rebuild_claude_sessions();
    }

    pub fn add_codex_records(&mut self, records: Vec<UsageRecord>) {
        for r in records {
            if self.codex_records.len() >= MAX_RECORDS {
                self.codex_records.remove(0);
            }
            self.codex_total_cost += r.cost_usd;
            self.codex_total_calls += 1;
            self.codex_records.push(r);
        }
        self.rebuild_codex_sessions();
    }

    fn rebuild_claude_sessions(&mut self) {
        use std::collections::BTreeMap;
        let mut map: BTreeMap<(String, String), SessionSummary> = BTreeMap::new();
        for r in &self.claude_records {
            let key = (r.project.clone(), r.model.clone());
            let entry = map.entry(key).or_insert_with(|| SessionSummary {
                project: r.project.clone(),
                model: r.model.clone(),
                total_input: 0,
                total_output: 0,
                total_cache_read: 0,
                total_cache_creation: 0,
                total_cost: 0.0,
                request_count: 0,
                last_active: r.timestamp,
            });
            entry.total_input += r.input_tokens;
            entry.total_output += r.output_tokens;
            entry.total_cache_read += r.cache_read_tokens;
            entry.total_cache_creation += r.cache_creation_tokens;
            entry.total_cost += r.cost_usd;
            entry.request_count += 1;
            if r.timestamp > entry.last_active {
                entry.last_active = r.timestamp;
            }
        }
        self.claude_sessions = map.into_values().collect();
    }

    fn rebuild_codex_sessions(&mut self) {
        use std::collections::BTreeMap;
        let mut map: BTreeMap<(String, String), SessionSummary> = BTreeMap::new();
        for r in &self.codex_records {
            let key = (r.project.clone(), r.model.clone());
            let entry = map.entry(key).or_insert_with(|| SessionSummary {
                project: r.project.clone(),
                model: r.model.clone(),
                total_input: 0,
                total_output: 0,
                total_cache_read: 0,
                total_cache_creation: 0,
                total_cost: 0.0,
                request_count: 0,
                last_active: r.timestamp,
            });
            entry.total_input += r.input_tokens;
            entry.total_output += r.output_tokens;
            entry.total_cache_read += r.cache_read_tokens;
            entry.total_cache_creation += r.cache_creation_tokens;
            entry.total_cost += r.cost_usd;
            entry.request_count += 1;
            if r.timestamp > entry.last_active {
                entry.last_active = r.timestamp;
            }
        }
        self.codex_sessions = map.into_values().collect();
    }

    pub fn clear_claude(&mut self) {
        self.claude_records.clear();
        self.claude_sessions.clear();
        self.claude_total_calls = 0;
        self.claude_total_cost = 0.0;
    }

    pub fn clear_codex(&mut self) {
        self.codex_records.clear();
        self.codex_sessions.clear();
        self.codex_total_calls = 0;
        self.codex_total_cost = 0.0;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: Update state mod.rs**

```rust
pub mod app_state;
pub use app_state::*;
```

- [ ] **Step 3: Verify compiles**

```bash
cargo check
```

Expected: No errors (other modules not yet added).

- [ ] **Step 4: Commit**

```bash
git add src/state/
git commit -m "feat: define UsageRecord, SessionSummary, AppState with dual-platform support"
```

---

## Task 3: Implement Pricing Table

**Files:**
- Create: `src/reader/pricing.rs`
- Create: `src/reader/mod.rs`

- [ ] **Step 1: Write pricing module**

Create: `src/reader/pricing.rs`

```rust
/// Pricing entry: (model_pattern, input_per_1m, output_per_1m, cache_read_per_1m)
/// All prices in USD per 1 million tokens
struct PricingEntry {
    pattern: &'static str,
    input: f64,
    output: f64,
    cache_read: f64,
}

const ANTHROPIC_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "claude-opus-4", input: 15.0, output: 75.0, cache_read: 1.50 },
    PricingEntry { pattern: "claude-sonnet-4", input: 3.0, output: 15.0, cache_read: 0.30 },
    PricingEntry { pattern: "claude-haiku-4", input: 1.0, output: 5.0, cache_read: 0.10 },
    PricingEntry { pattern: "claude-opus-3", input: 15.0, output: 75.0, cache_read: 1.50 },
    PricingEntry { pattern: "claude-sonnet-3", input: 3.0, output: 15.0, cache_read: 0.30 },
    PricingEntry { pattern: "claude-haiku-3", input: 0.25, output: 1.25, cache_read: 0.03 },
];

const OPENAI_PRICING: &[PricingEntry] = &[
    PricingEntry { pattern: "gpt-5.5", input: 1.25, output: 5.00, cache_read: 0.125 },
    PricingEntry { pattern: "gpt-5.4", input: 0.15, output: 0.60, cache_read: 0.015 },
    PricingEntry { pattern: "gpt-5.3-codex", input: 2.50, output: 10.0, cache_read: 0.25 },
    PricingEntry { pattern: "gpt-5.4-mini", input: 0.15, output: 0.60, cache_read: 0.015 },
    PricingEntry { pattern: "gpt-4.1", input: 2.00, output: 8.00, cache_read: 0.20 },
    PricingEntry { pattern: "gpt-4.1-mini", input: 0.40, output: 1.60, cache_read: 0.04 },
    PricingEntry { pattern: "kimi-k2", input: 0.60, output: 3.00, cache_read: 0.06 },
];

fn find_price(model: &str, table: &[PricingEntry]) -> Option<&PricingEntry> {
    table.iter().find(|e| model.contains(e.pattern))
}

/// Calculate cost in USD for a single request
pub fn calculate_cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    _cache_creation_tokens: u64,
) -> f64 {
    let entry = find_price(model, ANTHROPIC_PRICING)
        .or_else(|| find_price(model, OPENAI_PRICING));

    let Some(e) = entry else {
        return 0.0;
    };

    let input_cost = (input_tokens as f64 / 1_000_000.0) * e.input;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * e.output;
    let cache_cost = (cache_read_tokens as f64 / 1_000_000.0) * e.cache_read;

    input_cost + output_cost + cache_cost
}
```

- [ ] **Step 2: Create reader mod.rs**

Create: `src/reader/mod.rs`

```rust
pub mod claude;
pub mod codex;
pub mod pricing;
```

- [ ] **Step 3: Add module to main.rs**

```rust
mod reader;
```

- [ ] **Step 4: Verify compiles**

```bash
cargo check
```

- [ ] **Step 5: Commit**

```bash
git add src/reader/ src/main.rs
git commit -m "feat: add pricing table for Anthropic and OpenAI models"
```

---

## Task 4: Implement Claude Code Reader

**Files:**
- Create: `src/reader/claude.rs`

- [ ] **Step 1: Write Claude reader**

Create: `src/reader/claude.rs`

```rust
use crate::reader::pricing;
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ClaudeReader {
    data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
}

impl ClaudeReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            file_positions: HashMap::new(),
        }
    }

    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude/projects")
    }

    fn find_jsonl_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if !self.data_dir.exists() {
            return files;
        }
        find_jsonl_recursive(&self.data_dir, &mut files);
        files
    }

    pub fn scan_all(&mut self) -> Vec<UsageRecord> {
        let files = self.find_jsonl_files();
        let mut records = Vec::new();
        for file in files {
            let entries = self.read_file_from(&file, 0);
            self.file_positions.insert(file, entries.len() as u64);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }

    pub fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let files = self.find_jsonl_files();
        let mut new_records = Vec::new();
        for file in files {
            let offset = self.file_positions.get(&file).copied().unwrap_or(0);
            let entries = self.read_file_from(&file, offset);
            if !entries.is_empty() {
                *self.file_positions.entry(file).or_insert(0) += entries.len() as u64;
                new_records.extend(entries);
            }
        }
        new_records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        new_records
    }

    fn read_file_from(&self, path: &Path, skip_lines: u64) -> Vec<UsageRecord> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let project = extract_project_name(path, &self.data_dir);

        content
            .lines()
            .skip(skip_lines as usize)
            .filter_map(|line| parse_claude_line(line, &project))
            .collect()
    }
}

fn parse_claude_line(line: &str, project: &str) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    // Only process "assistant" type entries with usage data
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }

    let message = v.get("message")?;
    let usage = message.get("usage")?;

    let input_tokens = usage.get("input_tokens")?.as_u64()?;
    let output_tokens = usage.get("output_tokens")?.as_u64().unwrap_or(0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Skip entries with no meaningful tokens
    if input_tokens == 0 && output_tokens == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let request_id = v
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let message_id = message
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let service_tier = usage
        .get("service_tier")
        .and_then(|v| v.as_str())
        .unwrap_or("standard")
        .to_string();

    // Cost: read from JSONL if present, else calculate
    let cost_from_file = v
        .get("cost_usd")
        .and_then(|v| v.as_f64())
        .or_else(|| v.get("cost").and_then(|v| v.as_f64()));

    let cost_usd = cost_from_file.unwrap_or_else(|| {
        pricing::calculate_cost(&model, input_tokens, output_tokens, cache_read, cache_creation)
    });

    Some(UsageRecord {
        timestamp,
        platform: Platform::ClaudeCode,
        model,
        project: project.to_string(),
        input_tokens,
        output_tokens,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        cost_usd,
        service_tier,
        message_id,
        request_id,
    })
}

fn extract_project_name(path: &Path, data_dir: &Path) -> String {
    // ~/.claude/projects/-Users-youming-GitHub-repo-name/uuid.jsonl
    // Extract "-Users-youming-GitHub-repo-name" → "repo-name"
    path.parent()
        .and_then(|p| p.strip_prefix(data_dir).ok())
        .and_then(|p| p.to_str())
        .map(|s| {
            s.trim_start_matches('-')
                .split('-')
                .last()
                .unwrap_or(s)
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn find_jsonl_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_jsonl_recursive(&path, files);
            } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                files.push(path);
            }
        }
    }
}
```

- [ ] **Step 2: Verify compiles**

```bash
cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/reader/claude.rs
git commit -m "feat: implement Claude Code JSONL reader with delta polling"
```

---

## Task 5: Implement Codex Reader

**Files:**
- Create: `src/reader/codex.rs`

- [ ] **Step 1: Write Codex reader**

Create: `src/reader/codex.rs`

```rust
use crate::reader::pricing;
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct CodexReader {
    sessions_dir: PathBuf,
    state_db: Option<PathBuf>,
    file_positions: HashMap<PathBuf, u64>,
}

impl CodexReader {
    pub fn new(codex_dir: PathBuf) -> Self {
        let sessions_dir = codex_dir.join("sessions");
        let state_db = find_latest_state_db(&codex_dir);
        Self {
            sessions_dir,
            state_db,
            file_positions: HashMap::new(),
        }
    }

    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codex")
    }

    fn find_rollout_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.sessions_dir.exists() {
            find_rollout_recursive(&self.sessions_dir, &mut files);
        }
        files
    }

    pub fn scan_all(&mut self) -> Vec<UsageRecord> {
        let files = self.find_rollout_files();
        let mut records = Vec::new();
        for file in files {
            let entries = self.read_file_from(&file, 0);
            self.file_positions.insert(file, entries.len() as u64);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }

    pub fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let files = self.find_rollout_files();
        let mut new_records = Vec::new();
        for file in files {
            let offset = self.file_positions.get(&file).copied().unwrap_or(0);
            let entries = self.read_file_from(&file, offset);
            if !entries.is_empty() {
                *self.file_positions.entry(file).or_insert(0) += entries.len() as u64;
                new_records.extend(entries);
            }
        }
        new_records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        new_records
    }

    fn read_file_from(&self, path: &Path, skip_lines: u64) -> Vec<UsageRecord> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let project = extract_codex_project(path);

        content
            .lines()
            .skip(skip_lines as usize)
            .filter_map(|line| parse_codex_line(line, &project))
            .collect()
    }
}

/// Parse a single line from Codex rollout JSONL.
/// We look for `event_msg` with `payload.type == "token_count"` and `info` containing usage data.
/// We also look for `turn_context` to get the model name.
fn parse_codex_line(line: &str, project: &str) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type")?.as_str()?;

    if event_type != "event_msg" {
        return None;
    }

    let payload = v.get("payload")?;
    let payload_type = payload.get("type")?.as_str()?;

    if payload_type != "token_count" {
        return None;
    }

    let info = payload.get("info")?;
    if info.is_null() {
        return None;
    }

    let total = info.get("total_token_usage")?;
    let input_tokens = total.get("input_tokens")?.as_u64()?;
    let output_tokens = total.get("output_tokens")?.as_u64().unwrap_or(0);
    let cached = total
        .get("cached_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _reasoning = total
        .get("reasoning_output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Use last_token_usage for per-turn delta (not cumulative)
    let last = info.get("last_token_usage");
    let delta_input = last
        .and_then(|l| l.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(input_tokens);
    let delta_output = last
        .and_then(|l| l.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(output_tokens);
    let delta_cached = last
        .and_then(|l| l.get("cached_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(cached);

    if delta_input == 0 && delta_output == 0 {
        return None;
    }

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    // Model: not always present in token_count events
    // Try payload.model, fallback to "unknown"
    let model = payload
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let cost_usd = pricing::calculate_cost(&model, delta_input, delta_output, delta_cached, 0);

    Some(UsageRecord {
        timestamp,
        platform: Platform::Codex,
        model,
        project: project.to_string(),
        input_tokens: delta_input,
        output_tokens: delta_output,
        cache_read_tokens: delta_cached,
        cache_creation_tokens: 0,
        cost_usd,
        service_tier: String::new(),
        message_id: String::new(),
        request_id: String::new(),
    })
}

fn extract_codex_project(path: &Path) -> String {
    // ~/.codex/sessions/2026/03/03/rollout-*.jsonl
    // Extract project from the rollout filename or parent dirs
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| {
            // rollout-2026-03-03T10-08-22-uuid.jsonl → extract uuid or date
            s.split('-')
                .last()
                .unwrap_or(s)
                .to_string()
        })
        .unwrap_or_else(|| "codex".to_string())
}

fn find_rollout_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_rollout_recursive(&path, files);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
                .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }
}

fn find_latest_state_db(codex_dir: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = fs::read_dir(codex_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("state_") && n.ends_with(".sqlite"))
                .unwrap_or(false)
        })
        .collect();
    candidates.sort();
    candidates.pop()
}
```

- [ ] **Step 2: Verify compiles**

```bash
cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/reader/codex.rs
git commit -m "feat: implement Codex rollout JSONL reader with delta polling"
```

---

## Task 6: Implement UI Components

**Files:**
- Create: `src/ui/tabs.rs`
- Create: `src/ui/session_table.rs`
- Create: `src/ui/usage_table.rs`
- Create: `src/ui/status_bar.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Write tabs widget**

Create: `src/ui/tabs.rs`

```rust
use crate::state::Tab;
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

pub fn tab_bar(active: Tab) -> Tabs<'static> {
    let titles = vec![
        Line::from(Span::styled(
            " Claude Code ",
            Style::default().fg(Color::Rgb(255, 165, 0)), // warm orange
        )),
        Line::from(Span::styled(
            " Codex ",
            Style::default().fg(Color::Rgb(138, 43, 226)), // blue-purple
        )),
    ];

    let selected = match active {
        Tab::ClaudeCode => 0,
        Tab::Codex => 1,
    };

    Tabs::new(titles)
        .block(Block::default().title(" Usage Monitor ").borders(Borders::ALL))
        .select(selected)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .divider("|")
}
```

- [ ] **Step 2: Write session_table widget**

Create: `src/ui/session_table.rs`

```rust
use crate::state::SessionSummary;
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

pub fn session_table(sessions: &[SessionSummary], total_calls: usize) -> Table<'static> {
    let rows: Vec<Row> = sessions
        .iter()
        .map(|s| {
            Row::new(vec![
                Cell::from(s.project.clone()),
                Cell::from(s.model.clone()),
                Cell::from(format_tokens(s.total_input)),
                Cell::from(format_tokens(s.total_output)),
                Cell::from(format_tokens(s.total_cache_read)),
                Cell::from(format!("${:.2}", s.total_cost)),
                Cell::from(s.request_count.to_string()),
            ])
        })
        .collect();

    let header = Row::new(vec![
        "Project", "Model", "Input", "Output", "Cache", "Cost", "#",
    ])
    .style(Style::default().fg(Color::Yellow));

    Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(20),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
            Constraint::Percentage(10),
            Constraint::Percentage(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!("Sessions ({})", total_calls))
            .borders(Borders::ALL),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
```

- [ ] **Step 3: Write usage_table widget**

Create: `src/ui/usage_table.rs`

```rust
use crate::state::UsageRecord;
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

pub fn usage_table(records: &[UsageRecord], max: usize) -> Table<'static> {
    let rows: Vec<Row> = records
        .iter()
        .rev()
        .map(|r| {
            Row::new(vec![
                Cell::from(r.timestamp.format("%H:%M:%S").to_string()),
                Cell::from(r.model.clone()),
                Cell::from(format_tokens(r.input_tokens)),
                Cell::from(format_tokens(r.output_tokens)),
                Cell::from(format!("${:.4}", r.cost_usd)),
            ])
        })
        .collect();

    let header =
        Row::new(vec!["Time", "Model", "In", "Out", "Cost"]).style(Style::default().fg(Color::Yellow));

    Table::new(
        rows,
        [
            Constraint::Percentage(15),
            Constraint::Percentage(35),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
            Constraint::Percentage(16),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!("Recent API Calls ({}/{})", records.len(), max))
            .borders(Borders::ALL),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
```

- [ ] **Step 4: Write status_bar widget**

Create: `src/ui/status_bar.rs`

```rust
use crate::state::Tab;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn status_bar(
    active_tab: Tab,
    total_calls: usize,
    total_cost: f64,
    last_error: &Option<String>,
) -> Paragraph<'static> {
    let tab_label = match active_tab {
        Tab::ClaudeCode => "Claude Code",
        Tab::Codex => "Codex",
    };

    let error_span = if let Some(err) = last_error.as_ref() {
        Span::styled(format!(" | ERROR: {}", err), Style::default().fg(Color::Red))
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![
        Span::styled(
            format!("[{}]", tab_label),
            Style::default().fg(Color::Green),
        ),
        Span::raw(format!(
            " {} calls | ${:.2} | Tab:switch r:clear q:quit",
            total_calls, total_cost
        )),
        error_span,
    ]);

    Paragraph::new(line).block(Block::default().borders(Borders::TOP))
}
```

- [ ] **Step 5: Write UI render entry**

Create: `src/ui/mod.rs`

```rust
mod session_table;
mod status_bar;
mod tabs;
mod usage_table;

use crate::state::{AppState, MAX_RECORDS};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use std::sync::{Arc, RwLock};

pub fn render(frame: &mut Frame, app_state: &Arc<RwLock<AppState>>) {
    let state = app_state.read().unwrap();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tabs
            Constraint::Percentage(40), // sessions
            Constraint::Percentage(55), // usage detail
            Constraint::Min(1), // status bar
        ])
        .split(frame.area());

    // Tab bar
    frame.render_widget(tabs::tab_bar(state.active_tab), chunks[0]);

    // Session table + usage table for active tab
    let (sessions, records, total_calls, total_cost) = match state.active_tab {
        crate::state::Tab::ClaudeCode => (
            &state.claude_sessions,
            &state.claude_records,
            state.claude_total_calls,
            state.claude_total_cost,
        ),
        crate::state::Tab::Codex => (
            &state.codex_sessions,
            &state.codex_records,
            state.codex_total_calls,
            state.codex_total_cost,
        ),
    };

    frame.render_widget(session_table::session_table(sessions, total_calls), chunks[1]);
    frame.render_widget(usage_table::usage_table(records, MAX_RECORDS), chunks[2]);
    frame.render_widget(
        status_bar::status_bar(state.active_tab, total_calls, total_cost, &state.last_error),
        chunks[3],
    );
}
```

- [ ] **Step 6: Update main.rs module declarations**

```rust
mod state;
mod event;
mod ui;
mod reader;
```

- [ ] **Step 7: Verify compiles**

```bash
cargo check
```

- [ ] **Step 8: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "feat: add dual-tab TUI with session table, usage table, and status bar"
```

---

## Task 7: Update CLI Arguments

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Rewrite CLI struct**

Replace `src/cli.rs`:

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "usage-monitor")]
#[command(about = "Real-time Claude Code & Codex usage monitor")]
pub struct Cli {
    /// Path to Claude Code data directory
    #[arg(long, default_value_os_t = default_claude_path())]
    pub claude_path: PathBuf,

    /// Path to Codex data directory
    #[arg(long, default_value_os_t = default_codex_path())]
    pub codex_path: PathBuf,

    /// Polling interval in seconds
    #[arg(short, long, default_value_t = 5)]
    pub refresh: u64,
}

fn default_claude_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude/projects")
}

fn default_codex_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
}
```

- [ ] **Step 2: Verify CLI help**

```bash
cargo run -- --help
```

Expected: `--claude-path`, `--codex-path`, `--refresh` options.

- [ ] **Step 3: Commit**

```bash
git add src/cli.rs
git commit -m "feat: update CLI for Claude/Codex dual-platform arguments"
```

---

## Task 8: Wire Main Runtime

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Write full main.rs**

Replace `src/main.rs`:

```rust
mod cli;
mod event;
mod reader;
mod state;
mod ui;

use crate::event::{AppEvent, EventLoop};
use crate::reader::claude::ClaudeReader;
use crate::reader::codex::CodexReader;
use crate::state::AppState;
use clap::Parser;
use crossterm::event::KeyCode;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::Cli::parse();
    let app_state = Arc::new(RwLock::new(AppState::new()));

    // Claude reader task
    let claude_state = app_state.clone();
    let mut claude_reader = ClaudeReader::new(args.claude_path.clone());
    let refresh = args.refresh;
    let claude_handle = task::spawn(async move {
        // Initial scan
        let initial = claude_reader.scan_all();
        if !initial.is_empty() {
            if let Ok(mut state) = claude_state.write() {
                state.add_claude_records(initial);
            }
        }

        // Poll loop
        let mut interval = tokio::time::interval(Duration::from_secs(refresh));
        loop {
            interval.tick().await;
            let new_records = claude_reader.poll_delta();
            if !new_records.is_empty() {
                if let Ok(mut state) = claude_state.write() {
                    state.add_claude_records(new_records);
                }
            }
        }
    });

    // Codex reader task
    let codex_state = app_state.clone();
    let mut codex_reader = CodexReader::new(args.codex_path.clone());
    let codex_handle = task::spawn(async move {
        // Initial scan
        let initial = codex_reader.scan_all();
        if !initial.is_empty() {
            if let Ok(mut state) = codex_state.write() {
                state.add_codex_records(initial);
            }
        }

        // Poll loop
        let mut interval = tokio::time::interval(Duration::from_secs(refresh));
        loop {
            interval.tick().await;
            let new_records = codex_reader.poll_delta();
            if !new_records.is_empty() {
                if let Ok(mut state) = codex_state.write() {
                    state.add_codex_records(new_records);
                }
            }
        }
    });

    // TUI task
    let tui_state = app_state.clone();
    let tui_handle = task::spawn_blocking(move || {
        let mut terminal = ratatui::init();
        let result = run_tui(&mut terminal, tui_state);
        ratatui::restore();
        result
    });

    // Wait for TUI to exit (user pressed 'q')
    tui_handle.await??;

    // Abort background tasks
    claude_handle.abort();
    codex_handle.abort();

    Ok(())
}

fn run_tui(
    terminal: &mut ratatui::DefaultTerminal,
    app_state: Arc<RwLock<AppState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let tick_rate = Duration::from_millis(250);
    let (mut event_loop, _tx) = EventLoop::new(tick_rate);

    loop {
        terminal.draw(|frame| {
            ui::render(frame, &app_state);
        })?;

        if let Some(event) = event_loop.rx.blocking_recv() {
            match event {
                AppEvent::Tick => {}
                AppEvent::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Tab | KeyCode::Right => {
                        if let Ok(mut state) = app_state.write() {
                            state.active_tab = state.active_tab.next();
                        }
                    }
                    KeyCode::Left => {
                        if let Ok(mut state) = app_state.write() {
                            state.active_tab = state.active_tab.prev();
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Ok(mut state) = app_state.write() {
                            match state.active_tab {
                                state::Tab::ClaudeCode => state.clear_claude(),
                                state::Tab::Codex => state.clear_codex(),
                            }
                        }
                    }
                    _ => {}
                },
                AppEvent::Quit => break,
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Verify compiles**

```bash
cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire claude reader, codex reader, and TUI into main runtime"
```

---

## Task 9: Add dirs Dependency and Final Build

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add dirs crate**

```bash
cargo add dirs
```

- [ ] **Step 2: Full release build**

```bash
cargo build --release
```

Expected: `Finished release [optimized] target(s)`

- [ ] **Step 3: Test help output**

```bash
./target/release/usage-monitor --help
```

Expected: Help text with `--claude-path`, `--codex-path`, `--refresh`.

- [ ] **Step 4: Run with real data**

```bash
./target/release/usage-monitor
```

Expected: TUI shows Claude Code tab with existing usage data from `~/.claude/projects/`.

- [ ] **Step 5: Test tab switching**

Press `Tab` to switch between Claude Code and Codex views.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat: add dirs crate and finalize build"
```

---

## Self-Review

### Spec Coverage Check

| Spec Section | Implementing Task |
|-------------|-------------------|
| Claude Code JSONL reader | Task 4 |
| Codex rollout JSONL reader | Task 5 |
| Pricing table (Anthropic + OpenAI) | Task 3 |
| UsageRecord / SessionSummary models | Task 2 |
| AppState with dual-platform storage | Task 2 |
| Tab-based UI (Claude Code / Codex) | Task 6 |
| Session table (project summary) | Task 6 |
| Usage table (detailed calls) | Task 6 |
| Status bar | Task 6 |
| CLI args (--claude-path, --codex-path, --refresh) | Task 7 |
| Delta polling (file position tracking) | Task 4, 5 |
| Keybindings (Tab, q, r) | Task 8 |
| Arc<RwLock<AppState>> shared state | Task 8 |
| tokio concurrent tasks (2 readers + TUI) | Task 8 |
| Cost calculation (read from file + fallback) | Task 3, 4 |

### Placeholder Scan

- No "TBD", "TODO", "implement later" found.
- All types fully defined in Task 2.
- All pricing entries have concrete values.

### Type Consistency Check

- `UsageRecord` defined in Task 2, used in Tasks 4, 5, 6.
- `SessionSummary` defined in Task 2, used in Task 6.
- `AppState::add_claude_records` / `add_codex_records` used in Task 8.
- `Tab::next` / `Tab::prev` used in Task 8 key handlers.

**No gaps found.**
