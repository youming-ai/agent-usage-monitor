# aum stats --json Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `aum stats [--flags]` subcommand that scans all 13 registered agent readers, aggregates usage, and prints a JSON report to stdout without launching the TUI.

**Architecture:** New `src/stats.rs` module. For each registered platform, drive the reader's `scan_all()` inside `tokio::task::spawn_blocking`, then aggregate into a `StatsReport` (BTreeMap-keyed for stable JSON output). Quota fetch is opt-in via `--include-quota` and reuses the existing `quota::fetchers()`. Independent path from the TUI — no `AppState`, no event loop, no background tasks. Exits cleanly after writing JSON to stdout.

**Tech Stack:** `serde_json` (existing), `chrono` (existing), `tokio` (existing `task::spawn_blocking`), `std::io::IsTerminal` (stable since Rust 1.70). **No new dependencies.**

## Global Constraints

- No new `Cargo.toml` dependencies **EXCEPT** `anyhow = "1"` (already present transitively in `Cargo.lock`; promoted to direct dep for ergonomic `?` and `bail!` in `src/stats.rs`). Dev-deps for `serde_json` only if test failures require it.
- No changes to `AppState`, `UsageSource`, `Platform`, `Tab`, `Platforms::REGISTRY`, or `quota::fetchers()`
- All reader types read-only; stats module consumes their public API
- Aggregator logic uses BTreeMap, not HashMap, for stable JSON key order
- Float costs: `f64`. No string-formatted money; consumers do their own formatting
- Bypass the TUI entirely: no ratatui, no event loop, no ratatui backend init
- Exit code 0 on success, non-zero on panic only
- Errors to stderr (eprintln!), JSON to stdout

## File Structure

### New files

| File | Responsibility |
|---|---|
| `src/stats.rs` | Data types (`StatsReport`, `PlatformReport`, etc.), `collect()`, `write_json()`, `resolve_platform_filter()` |
| `tests/stats.rs` | Black-box CLI integration tests via `std::process::Command` against `target/debug/aum` |

### Modified files

| File | Change |
|---|---|
| `src/lib.rs` | Add `pub mod stats;` |
| `src/cli.rs` | Add `StatsArgs` + `Commands::Stats(StatsArgs)` |
| `src/main.rs` | Add `Some(Commands::Stats(args))` match arm + `handle_stats` |
| `README.md` | Add `## JSON stats` section |

---

## Task 1: Stats module stub + data types

**Files:**
- Create: `src/stats.rs`
- Modify: `src/lib.rs:1-9` (add `pub mod stats;`)

**Interfaces:**
- Exports: `StatsReport`, `Totals`, `PlatformReport`, `PlatformTotals`, `ModelSummary`, `SessionSummaryView`, `DateBucket`, `QuotaView`, `Filters`, `CollectOptions`
- (Functions and methods are added in later tasks; this task only adds types)

- [ ] **Step 1: Create `src/stats.rs` with the data types only (no functions yet)**

Create the file `src/stats.rs` with this content:

```rust
//! `aum stats --json` subcommand implementation.
//!
//! Produces a JSON usage report by driving each registered reader's
//! `scan_all()` and aggregating into a `StatsReport`. Independent from
//! the TUI: no event loop, no ratatui, no background tasks.

use crate::quota::{QuotaInfo, QuotaWindow};
use crate::state::UsageRecord;
use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
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
    pub windows: Vec<QuotaWindow>,
    pub fetched_at: String,
    pub error: Option<String>,
}

#[derive(Default, Debug)]
pub struct Filters {
    pub platforms: Option<std::collections::BTreeSet<String>>,
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
}

pub struct CollectOptions {
    pub include_quota: bool,
    pub filters: Filters,
}
```

- [ ] **Step 1.5: Add `anyhow` to `Cargo.toml`**

In `Cargo.toml`, add a new line under `[dependencies]`:

```toml
anyhow = "1"
```

Run: `cargo build --quiet`
Expected: success, no warnings. `anyhow` is already in `Cargo.lock` transitively (pulled in by `wit-*` crates), so this just promotes it to a direct dep.

- [ ] **Step 2: Add `pub mod stats;` to `src/lib.rs`**


Replace `src/lib.rs` with:

```rust
pub mod cli;
pub mod config;
pub mod event;
pub mod platforms;
pub mod quota;
pub mod reader;
pub mod state;
pub mod stats;
pub mod ui;
pub mod updater;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build --quiet`
Expected: success, no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/stats.rs src/lib.rs
git commit -m "feat(stats): add stats module with data types"
```

---

## Task 2: `build_platform_report` aggregator + unit tests (TDD)

**Files:**
- Modify: `src/stats.rs` (add `build_platform_report` and unit test module)

**Interfaces:**
- `pub fn build_platform_report(path: &PathBuf, available: bool, records: Vec<UsageRecord>, quota: Option<QuotaView>) -> PlatformReport`
- Behavior: aggregates records into totals, models (BTreeMap), sessions (Vec), dates (BTreeMap), and merges quota

- [ ] **Step 1: Write failing unit test**

Append to `src/stats.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Platform;
    use chrono::TimeZone;

    fn rec(model: &str, session: &str, day: u32, input: u64, output: u64, cost: f64) -> UsageRecord {
        UsageRecord {
            timestamp: Utc.with_ymd_and_hms(2026, 6, day, 12, 0, 0).unwrap(),
            platform: Platform::ClaudeCode,
            model: model.to_string(),
            session: session.to_string(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: cost,
        }
    }

    #[test]
    fn build_platform_report_aggregates_per_model() {
        let path = PathBuf::from("/tmp/.claude/projects");
        let records = vec![
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
            rec("claude-opus-4", "s1", 15, 200, 100, 0.50),
        ];
        let pr = build_platform_report(&path, true, records, None);
        assert_eq!(pr.totals.calls, 3);
        assert!((pr.totals.cost_usd - 0.70).abs() < 1e-9);
        assert_eq!(pr.models.len(), 2);
        let sonnet = pr.models.get("claude-sonnet-4").unwrap();
        assert_eq!(sonnet.calls, 2);
        assert_eq!(sonnet.input_tokens, 200);
        assert_eq!(sonnet.sessions, 1);
    }

    #[test]
    fn build_platform_report_aggregates_per_session() {
        let path = PathBuf::from("/tmp/.claude/projects");
        let records = vec![
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
            rec("claude-sonnet-4", "s2", 15, 200, 100, 0.20),
        ];
        let pr = build_platform_report(&path, true, records, None);
        assert_eq!(pr.sessions.len(), 2);
        let s1 = pr.sessions.iter().find(|s| s.session == "s1").unwrap();
        assert_eq!(s1.calls, 1);
        assert_eq!(s1.input_tokens, 100);
    }

    #[test]
    fn build_platform_report_aggregates_per_date() {
        let path = PathBuf::from("/tmp/.claude/projects");
        let records = vec![
            rec("claude-sonnet-4", "s1", 14, 100, 50, 0.10),
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
        ];
        let pr = build_platform_report(&path, true, records, None);
        assert_eq!(pr.dates.len(), 2);
        let day_15 = pr.dates.get("2026-06-15").unwrap();
        assert_eq!(day_15.calls, 2);
        assert_eq!(day_15.models.get("claude-sonnet-4"), Some(&2));
    }

    #[test]
    fn build_platform_report_session_lists_models_distinct() {
        let path = PathBuf::from("/tmp/.claude/projects");
        let records = vec![
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
            rec("claude-opus-4", "s1", 15, 200, 100, 0.50),
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
        ];
        let pr = build_platform_report(&path, true, records, None);
        let s1 = pr.sessions.iter().find(|s| s.session == "s1").unwrap();
        assert_eq!(s1.calls, 3);
        let mut models = s1.models.clone();
        models.sort();
        assert_eq!(models, vec!["claude-opus-4", "claude-sonnet-4"]);
    }
}
```

- [ ] **Step 2: Run tests, verify they fail to compile**

Run: `cargo test --lib --quiet 2>&1 | head -30`
Expected: compile error — `build_platform_report` not defined.

- [ ] **Step 3: Implement `build_platform_report`**

Insert before the `#[cfg(test)]` block in `src/stats.rs`:

```rust
pub fn build_platform_report(
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
        if !s.models.contains(&r.model) {
            s.models.push(r.model.clone());
        }

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

    for (model_name, m) in models.iter_mut() {
        m.sessions = session_map
            .values()
            .filter(|s| s.models.contains(model_name))
            .count() as u32;
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
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test --lib stats::tests --quiet`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stats.rs
git commit -m "feat(stats): add build_platform_report aggregator with tests"
```

---

## Task 3: `Filters` struct + unit tests (TDD)

**Files:**
- Modify: `src/stats.rs` (add `impl Filters` and tests)

**Interfaces:**
- `Filters::matches_platform(&self, key: &str) -> bool`
- `Filters::matches_date(&self, ts: DateTime<Utc>) -> bool`

- [ ] **Step 1: Write failing unit test**

Append to the `mod tests` block:

```rust
    #[test]
    fn filters_matches_platform_none_accepts_all() {
        let f = Filters::default();
        assert!(f.matches_platform("claude_code"));
        assert!(f.matches_platform("codex"));
    }

    #[test]
    fn filters_matches_platform_set_filters_correctly() {
        let f = Filters {
            platforms: Some(BTreeSet::from(["claude_code".to_string()])),
            ..Default::default()
        };
        assert!(f.matches_platform("claude_code"));
        assert!(!f.matches_platform("codex"));
    }

    #[test]
    fn filters_matches_date_handles_since_until() {
        use chrono::TimeZone;
        let f = Filters {
            since: NaiveDate::from_ymd_opt(2026, 6, 15),
            until: NaiveDate::from_ymd_opt(2026, 6, 20),
            ..Default::default()
        };
        let day_14 = Utc.with_ymd_and_hms(2026, 6, 14, 12, 0, 0).unwrap();
        let day_15 = Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap();
        let day_20 = Utc.with_ymd_and_hms(2026, 6, 20, 12, 0, 0).unwrap();
        let day_21 = Utc.with_ymd_and_hms(2026, 6, 21, 12, 0, 0).unwrap();
        assert!(!f.matches_date(day_14));
        assert!(f.matches_date(day_15));
        assert!(f.matches_date(day_20));
        assert!(!f.matches_date(day_21));
    }
```

- [ ] **Step 2: Run tests, verify they fail to compile**

Run: `cargo test --lib stats::tests::filters --quiet 2>&1 | head -20`
Expected: compile error — `matches_platform` not defined.

- [ ] **Step 3: Implement `Filters` methods**

Insert after the `Filters` struct definition:

```rust
impl Filters {
    pub fn matches_platform(&self, key: &str) -> bool {
        match &self.platforms {
            None => true,
            Some(set) => set.contains(key),
        }
    }

    pub fn matches_date(&self, ts: DateTime<Utc>) -> bool {
        let d = ts.date_naive();
        if let Some(s) = self.since {
            if d < s {
                return false;
            }
        }
        if let Some(u) = self.until {
            if d > u {
                return false;
            }
        }
        true
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test --lib stats::tests::filters --quiet`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stats.rs
git commit -m "feat(stats): add Filters struct with platform/date matching"
```

---

## Task 4: `resolve_platform_filter` + unit tests (TDD)

**Files:**
- Modify: `src/stats.rs`

**Interfaces:**
- `pub fn resolve_platform_filter(raw: &[String]) -> Result<BTreeSet<String>>`
- Accepts three forms: config_key stripped (`claude_code`), Tab variant (`ClaudeCode`), log_name (`Claude Code`)
- Empty strings are skipped
- Unknown forms return `bail!` error

- [ ] **Step 1: Write failing unit test**

Append to `mod tests`:

```rust
    #[test]
    fn resolve_platform_filter_accepts_config_key() {
        let result = resolve_platform_filter(&["claude_code".to_string()]).unwrap();
        assert!(result.contains("claude_code"));
    }

    #[test]
    fn resolve_platform_filter_accepts_tab_variant() {
        let result = resolve_platform_filter(&["ClaudeCode".to_string()]).unwrap();
        assert!(result.contains("claude_code"));
    }

    #[test]
    fn resolve_platform_filter_accepts_log_name() {
        let result = resolve_platform_filter(&["Claude Code".to_string()]).unwrap();
        assert!(result.contains("claude_code"));
    }

    #[test]
    fn resolve_platform_filter_rejects_unknown() {
        let result = resolve_platform_filter(&["nonexistent_agent".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_platform_filter_dedupes() {
        let result = resolve_platform_filter(&[
            "claude_code".to_string(),
            "ClaudeCode".to_string(),
        ])
        .unwrap();
        assert_eq!(result.len(), 1);
    }
```

- [ ] **Step 2: Run tests, verify they fail to compile**

Run: `cargo test --lib stats::tests::resolve_platform --quiet 2>&1 | head -20`
Expected: compile error — `resolve_platform_filter` not defined.

- [ ] **Step 3: Implement `resolve_platform_filter`**

Insert after the `Filters` impl block:

```rust
pub fn resolve_platform_filter(raw: &[String]) -> Result<BTreeSet<String>> {
    use crate::platforms;
    let mut set = BTreeSet::new();
    for r in raw {
        let normalized = r.trim();
        if normalized.is_empty() {
            continue;
        }
        let mut matched = false;
        for entry in platforms::entries() {
            let stripped = entry.config_key.trim_end_matches("_path");
            let tab_name = format!("{:?}", entry.tab);
            if stripped == normalized || tab_name == normalized || entry.log_name == normalized
            {
                set.insert(stripped.to_string());
                matched = true;
                break;
            }
        }
        if !matched {
            anyhow::bail!("unknown platform: `{normalized}`; run `aum config set` to list valid keys");
        }
    }
    Ok(set)
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test --lib stats::tests::resolve_platform --quiet`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stats.rs
git commit -m "feat(stats): add resolve_platform_filter accepting 3 key forms"
```

---

## Task 5: `QuotaView::from_info` + unit test (TDD)

**Files:**
- Modify: `src/stats.rs`

**Interfaces:**
- `pub fn from_info(q: QuotaInfo, fetched_at: DateTime<Utc>) -> QuotaView`

**Important:** Before writing the test, read `src/quota/mod.rs` to confirm the exact field names of `QuotaError` and `QuotaInfo`. The test below assumes the fields are `kind: QuotaErrorKind` and `message: String`, with `kind` being `Display`-able. If the real struct differs, adjust the test constructor accordingly.

- [ ] **Step 1: Read `src/quota/mod.rs` to verify `QuotaError` struct shape**

Run: `grep -n "pub struct QuotaError\|pub enum QuotaErrorKind\|pub struct QuotaInfo" src/quota/mod.rs`

Adjust the test below to match. Common fields: `kind` (enum), `message` (String), and `display()` method.

- [ ] **Step 2: Write failing unit test**

Append to `mod tests`:

```rust
    #[test]
    fn quota_view_from_info_copies_fields() {
        use crate::quota::{QuotaError, QuotaWindow};
        use std::time::Instant;
        let info = QuotaInfo {
            tool_name: "Claude Code".to_string(),
            email: Some("me@example.com".to_string()),
            account_id: None,
            windows: vec![QuotaWindow {
                label: "5h".to_string(),
                remaining_percent: Some(0.85),
                resets_at: None,
                reset_in: Some("3h 25m".to_string()),
            }],
            fetched_at: Instant::now(),
            error: None,
        };
        let fetched_at = Utc::now();
        let view = QuotaView::from_info(info, fetched_at);
        assert_eq!(view.tool_name, "Claude Code");
        assert_eq!(view.email, Some("me@example.com".to_string()));
        assert_eq!(view.windows.len(), 1);
        assert_eq!(view.windows[0].label, "5h");
        assert!(view.fetched_at.contains("T"));
    }

    #[test]
    fn quota_view_from_info_captures_error_string() {
        use crate::quota::{QuotaError, QuotaErrorKind};
        use std::time::Instant;
        let info = QuotaInfo {
            tool_name: "Codex".to_string(),
            email: None,
            account_id: None,
            windows: vec![],
            fetched_at: Instant::now(),
            error: Some(QuotaError {
                kind: QuotaErrorKind::Auth,
                message: "no token".to_string(),
            }),
        };
        let view = QuotaView::from_info(info, Utc::now());
        assert!(view.error.is_some());
    }
```

- [ ] **Step 3: Run tests, verify they fail to compile**

Run: `cargo test --lib stats::tests::quota_view --quiet 2>&1 | head -30`
Expected: compile error — `from_info` not defined (or `QuotaError` field mismatches).

- [ ] **Step 4: Implement `from_info`**

Insert in the existing `impl QuotaView` block (or create a new one if none exists):

```rust
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
```

- [ ] **Step 5: Run tests, verify they pass**

Run: `cargo test --lib stats::tests::quota_view --quiet`
Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/stats.rs
git commit -m "feat(stats): add QuotaView::from_info constructor"
```

---

## Task 6: `collect()` function

**Files:**
- Modify: `src/stats.rs` (add `collect`)

**Interfaces:**
- `pub async fn collect(paths: &AgentPaths, opts: CollectOptions) -> Result<StatsReport>`

- [ ] **Step 1: Add `AgentPaths` import**

At the top of `src/stats.rs`, add to the `use` list:

```rust
use crate::platforms::AgentPaths;
```

- [ ] **Step 2: Implement `collect`**

Insert before the `#[cfg(test)]` block:

```rust
pub async fn collect(paths: &AgentPaths, opts: CollectOptions) -> Result<StatsReport> {
    use crate::platforms;
    use crate::reader::UsageSource;
    use crate::state::Platform;
    use std::collections::HashMap;

    // 第一遍：scan_all 收集记录（顺序，task::spawn_blocking 包 I/O）
    let mut entries: Vec<(String, Platform, PathBuf, Vec<UsageRecord>)> = Vec::new();
    for entry in platforms::entries() {
        let key = entry.config_key.trim_end_matches("_path").to_string();
        if !opts.filters.matches_platform(&key) {
            continue;
        }
        let path = paths.path_for(entry.tab);
        let mut reader = entry.build_reader(path.clone());
        let records = tokio::task::spawn_blocking(move || reader.scan_all())
            .await
            .unwrap_or_default();
        entries.push((key, entry.platform, path, records));
    }

    // Quota：仅在 --include-quota 时拉取。fetch() 是阻塞 HTTP，调完后取时间戳
    // 作为 fetched_at 写入 QuotaView（Instant 无法转 RFC3339，必须用 DateTime<Utc>）。
    let quota_views: Option<HashMap<Platform, QuotaView>> = if opts.include_quota {
        let q = tokio::task::spawn_blocking(|| {
            crate::quota::fetchers()
                .iter()
                .map(|f| (f.platform(), f.fetch()))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        let now = Utc::now();
        Some(
            q.into_iter()
                .filter_map(|(p, q)| q.map(|qi| (p, QuotaView::from_info(qi, now))))
                .collect(),
        )
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
        let quota = quota_views
            .as_ref()
            .and_then(|m| m.get(&platform).cloned());
        let pr = build_platform_report(&path, available, filtered, quota);
        if pr.available {
            report.totals.platforms_with_data += 1;
        }
        report.totals.total_calls += pr.totals.calls;
        report.totals.total_cost_usd += pr.totals.cost_usd;
        report.platforms.insert(key, pr);
    }
    Ok(report)
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build --quiet`
Expected: success, no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/stats.rs
git commit -m "feat(stats): add collect() function wiring readers + quota"
```

---

## Task 7: `write_json` function + unit test (TDD)

**Files:**
- Modify: `src/stats.rs`

**Interfaces:**
- `pub fn write_json<W: Write>(report: &StatsReport, pretty: bool, out: W) -> Result<()>`

- [ ] **Step 1: Write failing unit test**

Append to `mod tests`:

```rust
    #[test]
    fn write_json_produces_valid_json_compact() {
        let report = StatsReport {
            generated_at: Utc::now(),
            platforms: BTreeMap::new(),
            totals: Totals::default(),
        };
        let mut buf = Vec::new();
        write_json(&report, false, &mut buf).unwrap();
        let s = std::str::from_utf8(&buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(s).unwrap();
        assert!(parsed.get("platforms").is_some());
        assert!(parsed.get("totals").is_some());
        assert!(parsed.get("generated_at").is_some());
    }

    #[test]
    fn write_json_pretty_has_newlines() {
        let report = StatsReport {
            generated_at: Utc::now(),
            platforms: BTreeMap::new(),
            totals: Totals::default(),
        };
        let mut buf = Vec::new();
        write_json(&report, true, &mut buf).unwrap();
        let s = std::str::from_utf8(&buf).unwrap();
        assert!(s.contains('\n'), "pretty JSON should contain newlines");
    }

    #[test]
    fn write_json_skips_quota_field_when_none() {
        let mut platforms = BTreeMap::new();
        platforms.insert(
            "claude_code".to_string(),
            PlatformReport {
                available: true,
                data_path: PathBuf::from("/tmp/x"),
                totals: PlatformTotals::default(),
                models: BTreeMap::new(),
                sessions: vec![],
                dates: BTreeMap::new(),
                quota: None,
            },
        );
        let report = StatsReport {
            generated_at: Utc::now(),
            platforms,
            totals: Totals::default(),
        };
        let mut buf = Vec::new();
        write_json(&report, false, &mut buf).unwrap();
        let s = std::str::from_utf8(&buf).unwrap();
        assert!(!s.contains("\"quota\""), "quota field should be skipped when None");
    }
```

- [ ] **Step 2: Run tests, verify they fail to compile**

Run: `cargo test --lib stats::tests::write_json --quiet 2>&1 | head -20`
Expected: compile error — `write_json` not defined.

- [ ] **Step 3: Implement `write_json`**

Add to the top-of-file `use` block:

```rust
use anyhow::Context;
use std::io::Write;
```

(Remove the existing `use std::io::Write;` first to avoid duplicates.)

Insert before the `#[cfg(test)]` block:

```rust
pub fn write_json<W: Write>(report: &StatsReport, pretty: bool, out: W) -> Result<()> {
    let writer = std::io::BufWriter::new(out);
    if pretty {
        serde_json::to_writer_pretty(writer, report).context("serialize pretty json")?;
    } else {
        serde_json::to_writer(writer, report).context("serialize compact json")?;
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test --lib stats::tests::write_json --quiet`
Expected: 3 tests pass.

- [ ] **Step 5: Run all stats unit tests**

Run: `cargo test --lib stats --quiet`
Expected: 17 tests pass (4 aggregator + 3 filters + 5 platform filter + 2 quota + 3 write_json).

- [ ] **Step 6: Commit**

```bash
git add src/stats.rs
git commit -m "feat(stats): add write_json with pretty/compact modes"
```

---

## Task 8: CLI subcommand

**Files:**
- Modify: `src/cli.rs`

**Interfaces:**
- `StatsArgs { platform: Vec<String>, since: Option<String>, until: Option<String>, include_quota: bool, pretty: bool, compact: bool }`
- `Commands::Stats(StatsArgs)` variant

- [ ] **Step 1: Read current `src/cli.rs`**

Run: `cat src/cli.rs`

Identify the `enum Commands` block and the existing `#[derive(Args)]` struct pattern (Update, Config). The new `StatsArgs` follows the same pattern.

- [ ] **Step 2: Add `StatsArgs` struct**

Above the `enum Commands` definition, add:

```rust
#[derive(Args, Debug)]
pub struct StatsArgs {
    /// 仅输出指定平台 (逗号分隔, 支持 config_key / Tab variant / log_name)
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
```

- [ ] **Step 3: Add `Stats` variant to `Commands`**

In the `enum Commands` block, add:

```rust
    /// 输出 JSON 用量报告（不启动 TUI）
    Stats(StatsArgs),
```

- [ ] **Step 4: Verify it compiles and `--help` shows the new subcommand**

Run: `cargo build --quiet && ./target/debug/aum --help 2>&1 | grep -A1 "stats"`
Expected: shows `stats    输出 JSON 用量报告（不启动 TUI）`.

- [ ] **Step 5: Verify subcommand help works**

Run: `./target/debug/aum stats --help`
Expected: shows the new flags (`--platform`, `--since`, `--until`, `--include-quota`, `--pretty`, `--compact`).

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs
git commit -m "feat(stats): add stats subcommand to CLI"
```

---

## Task 9: main.rs routing

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Read the current match arm in `main()`**

Look at the match in `main()` (around line 32-42):

```rust
match args.command {
    Some(cli::Commands::Update { force, dry_run }) => { ... }
    Some(cli::Commands::Config { action }) => { ... }
    None => { /* TUI */ }
}
```

- [ ] **Step 2: Add `Stats` arm before `None`**

Insert:

```rust
        Some(cli::Commands::Stats(args)) => {
            return handle_stats(args, &config);
        }
```

(Note: `config` is the local `Config` variable already loaded earlier in `main()`. The pattern matches existing style.)

- [ ] **Step 3: Add `handle_stats` function**

Insert anywhere after `main()` in `src/main.rs`:

```rust
async fn handle_stats(
    args: cli::StatsArgs,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let since = args
        .since
        .as_deref()
        .map(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("--since must be YYYY-MM-DD: {e}"))?;
    let until = args
        .until
        .as_deref()
        .map(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("--until must be YYYY-MM-DD: {e}"))?;

    let platform_filter = if args.platform.is_empty() {
        None
    } else {
        Some(stats::resolve_platform_filter(&args.platform)?)
    };

    // Re-parse CLI to give resolve_paths a &Cli. Stats subcommand has not yet
    // consumed CLI args, so the global parse is cheap and safe.
    let cli = cli::Cli::parse();
    let paths = platforms::resolve_paths(&cli, config);

    let opts = stats::CollectOptions {
        include_quota: args.include_quota,
        filters: stats::Filters {
            platforms: platform_filter,
            since,
            until,
        },
    };
    let report = stats::collect(&paths, opts).await?;
    let pretty = args.pretty || (!args.compact && std::io::stdout().is_terminal());
    let stdout = std::io::stdout().lock();
    stats::write_json(&report, pretty, stdout)?;
    Ok(())
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build --quiet`
Expected: success, no warnings.

- [ ] **Step 5: Smoke test: run with no flags**

Run: `./target/debug/aum stats --compact 2>&1 | head -3`
Expected: valid JSON on stdout, e.g.
```
{"generated_at":"2026-06-18T...","platforms":{...},"totals":{...}}
```

- [ ] **Step 6: Smoke test: filter by claude_code**

Run: `./target/debug/aum stats --platform claude_code --compact 2>&1 | head -1 | python3 -c 'import json,sys; d=json.load(sys.stdin); print("platform count:", len(d["platforms"]))'`
Expected: `platform count: 1`.

- [ ] **Step 7: Smoke test: invalid date errors**

Run: `./target/debug/aum stats --since not-a-date --compact 2>&1; echo "exit: $?"`
Expected: non-zero exit code, error message about `--since must be YYYY-MM-DD`.

- [ ] **Step 8: Commit**

```bash
git add src/main.rs
git commit -m "feat(stats): route stats subcommand in main"
```

---

## Task 10: Integration tests (`tests/stats.rs`)

**Files:**
- Create: `tests/stats.rs`

**Interfaces:** Black-box CLI tests that invoke the built binary via `std::process::Command`. The `env!("CARGO_BIN_EXE_aum")` macro resolves to the compiled binary path during `cargo test`.

- [ ] **Step 1: Create the test file**

Create `tests/stats.rs` with this content:

```rust
//! Black-box integration tests for the `aum stats` subcommand.
//!
//! Spawn the compiled binary as a subprocess. Requires `cargo build` to have
//! been run first (CI does this; locally it's a no-op if already built).

use std::process::Command;

fn aum_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aum"));
    cmd.arg("stats");
    cmd.env("NO_COLOR", "1");
    cmd
}

#[test]
fn stats_default_produces_valid_json_with_all_platforms() {
    let output = aum_bin().arg("--compact").output().expect("run aum stats");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert!(json.get("generated_at").is_some());
    assert!(json.get("totals").is_some());
    let platforms = json.get("platforms").unwrap().as_object().unwrap();
    assert_eq!(platforms.len(), 13, "all 13 platforms should be present");
}

#[test]
fn stats_platform_filter_returns_only_matching() {
    let output = aum_bin()
        .args(["--platform", "claude_code", "--compact"])
        .output()
        .expect("run aum stats");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let platforms = json.get("platforms").unwrap().as_object().unwrap();
    assert_eq!(platforms.len(), 1, "platform filter should return 1 entry");
    assert!(platforms.contains_key("claude_code"));
}

#[test]
fn stats_unavailable_platform_has_available_field() {
    let output = aum_bin()
        .args(["--platform", "codex", "--compact"])
        .output()
        .expect("run aum stats");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let codex = json.get("platforms").unwrap().get("codex").unwrap();
    let available = codex.get("available").unwrap().as_bool().unwrap();
    let _ = available;
}

#[test]
fn stats_json_keys_are_stably_ordered() {
    let output = aum_bin().arg("--compact").output().expect("run aum stats");
    let s = String::from_utf8(output.stdout).unwrap();
    let g = s.find("\"generated_at\"").expect("generated_at present");
    let p = s.find("\"platforms\"").expect("platforms present");
    let t = s.find("\"totals\"").expect("totals present");
    assert!(g < p && p < t, "top-level keys must be ordered: generated_at < platforms < totals");
}

#[test]
fn stats_quota_field_absent_without_flag() {
    let output = aum_bin().arg("--compact").output().expect("run aum stats");
    let s = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&s).unwrap();
    let platforms = json.get("platforms").unwrap().as_object().unwrap();
    for (name, pr) in platforms {
        if name == "claude_code" || name == "codex" {
            assert!(
                pr.get("quota").is_none(),
                "{name} should not have quota when --include-quota is not set"
            );
        }
    }
}

#[test]
fn stats_unknown_platform_errors() {
    let output = aum_bin()
        .args(["--platform", "nonexistent_agent", "--compact"])
        .output()
        .expect("run aum stats");
    assert!(!output.status.success(), "unknown platform should exit non-zero");
}

#[test]
fn stats_invalid_date_errors() {
    let output = aum_bin()
        .args(["--since", "not-a-date", "--compact"])
        .output()
        .expect("run aum stats");
    assert!(!output.status.success(), "invalid date should exit non-zero");
}
```

- [ ] **Step 2: Add `serde_json` as a dev-dependency if not already transitively available**

Run: `cargo test --test stats --quiet 2>&1 | head -30`

If the test fails with "crate `serde_json` not found", add to `Cargo.toml` under `[dev-dependencies]`:

```toml
[dev-dependencies]
serde_json = "1.0.150"
```

(Check the existing `[dev-dependencies]` block first — if `serde_json` is already there as a transitive or explicit dep, no change needed.)

- [ ] **Step 3: Re-run integration tests**

Run: `cargo test --test stats --quiet`
Expected: all 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/stats.rs Cargo.toml Cargo.lock
git commit -m "test(stats): add black-box CLI integration tests"
```

---

## Task 11: README update

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Locate the right insertion point in README.md**

Run: `grep -n "^## " README.md | head -20`

Find an appropriate insertion point — after the existing usage examples, before any "Configuration" or "Build" section. The new section is `## JSON stats`.

- [ ] **Step 2: Add the `## JSON stats` section**

Insert (adapt Chinese/English tone to match existing README style; the project README is mixed):

````markdown
## JSON stats

For scripts, CI, and external monitoring, `aum` can emit a structured JSON
report without launching the TUI:

```bash
# Pretty-printed to stdout (TTY auto-detected)
aum stats

# Compact JSON, suitable for `jq`
aum stats --compact | jq '.platforms.claude_code.totals.cost_usd'

# Filter to specific agents
aum stats --platform claude_code,codex

# Time-bounded report
aum stats --since 2026-06-01 --until 2026-06-30

# Include live quota (Claude / Codex; requires local credentials)
aum stats --platform claude_code --include-quota
```

### Schema

| Field | Type | Description |
|---|---|---|
| `generated_at` | RFC 3339 timestamp | When the report was generated |
| `platforms` | object | Per-platform breakdown keyed by `config_key` (e.g. `claude_code`) |
| `platforms.<k>.available` | bool | Whether the data directory exists |
| `platforms.<k>.data_path` | path | Resolved path (CLI override > config > default) |
| `platforms.<k>.totals` | object | Aggregate: `calls`, `cost_usd`, `input_tokens`, `output_tokens`, `cache_read_tokens`, `cache_creation_tokens` |
| `platforms.<k>.models` | object | Per-model breakdown, same fields plus `sessions` |
| `platforms.<k>.sessions[]` | array | Per-session summary with `models` list |
| `platforms.<k>.dates` | object | Per-day bucket keyed by `YYYY-MM-DD` |
| `platforms.<k>.quota` | object | Only present with `--include-quota`, only for Claude / Codex |
| `totals` | object | Cross-platform: `total_calls`, `total_cost_usd`, `platforms_with_data` |

Top-level keys are alphabetically ordered (BTreeMap). Use `--compact` for
scripting, default (pretty) for human reading.
````

- [ ] **Step 3: Verify no markdown lint issues**

Run: `grep -n "## JSON stats" README.md`
Expected: 1 match.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add JSON stats section to README"
```

---

## Task 12: Final verification

**Files:** None new (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test --quiet 2>&1 | tail -20`
Expected: all tests pass (lib + integration + reader_fixtures + stats).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --all-targets --quiet -- -D warnings 2>&1 | tail -20`
Expected: no warnings.

- [ ] **Step 3: Run cargo fmt check**

Run: `cargo fmt --all -- --check 2>&1 | tail -10`
Expected: no diff (clean formatting). If diff exists, run `cargo fmt --all` and amend the previous commit.

- [ ] **Step 4: Build release binary**

Run: `cargo build --release --quiet 2>&1 | tail -10`
Expected: success.

- [ ] **Step 5: End-to-end smoke test**

Run: `./target/release/aum stats --compact 2>&1 | jq '.platforms | keys | length'`
Expected: `13`.

Run: `./target/release/aum stats --platform claude_code --compact 2>&1 | jq '.platforms.claude_code.available'`
Expected: `true` or `false` (boolean).

Run: `./target/release/aum stats --platform foo --compact 2>&1; echo "exit: $?"`
Expected: non-zero exit, error to stderr.

- [ ] **Step 6: Final commit (amend or empty commit)**

If any step required fixes, amend the relevant commit. Otherwise:

```bash
git status --short
# If clean, no further commit needed
```

If `Cargo.lock` changed during dev-dep addition, commit it:

```bash
git add Cargo.lock
git commit --allow-empty -m "chore: post-spec-1 verification clean"
```

---
