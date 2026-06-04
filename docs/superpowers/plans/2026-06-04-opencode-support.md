# opencode support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opencode tab to `aum` showing per-model and per-session usage read from opencode's local SQLite database, behind a new `UsageSource` reader abstraction, with a reserved (no-op) quota slot.

**Architecture:** A `UsageSource` trait unifies the three readers; `main.rs` spawns one uniform reader task per source. The existing JSONL readers (Claude/Codex) implement it by delegating to `JsonlReader`; a new read-only-SQLite `OpencodeReader` implements it directly, emitting one `UsageRecord` per assistant message in `opencode.db`. `AppState`/`Tab`/`Platform` gain an `OpenCode` variant; the model/session UI is reused unchanged.

**Tech Stack:** Rust 2024, ratatui, tokio, rusqlite (bundled, already a dependency), serde_json, chrono.

**Spec:** `docs/superpowers/specs/2026-06-04-opencode-support-design.md`

---

## Task 1: `UsageSource` trait + refactor existing readers and main

Pure refactor — no behavior change, still two tabs. Establishes the abstraction.

**Files:**
- Modify: `src/reader/mod.rs` (add trait + impls for Claude/Codex)
- Modify: `src/state/app_state.rs` (add `add_records` dispatch)
- Modify: `src/main.rs` (unified reader-task loop)
- Test: `src/state/app_state.rs` (dispatch test)

- [ ] **Step 1: Add the `UsageSource` trait and impls in `src/reader/mod.rs`**

Add these imports near the top (after the existing `use std::...` lines):

```rust
use crate::state::{Platform, UsageRecord};
use jsonl_reader::JsonlReader;
```

Append to the end of `src/reader/mod.rs`:

```rust
/// A source of usage records, abstracting over the backing store (JSONL files
/// for Claude/Codex, SQLite for opencode). `main.rs` drives every source the
/// same way: an initial `scan_all`, then a `poll_delta` loop.
pub trait UsageSource: Send {
    fn platform(&self) -> Platform;
    fn scan_all(&mut self) -> Vec<UsageRecord>;
    fn poll_delta(&mut self) -> Vec<UsageRecord>;
}

impl UsageSource for claude::ClaudeReader {
    fn platform(&self) -> Platform {
        Platform::ClaudeCode
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        JsonlReader::scan_all(self)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        JsonlReader::poll_delta(self)
    }
}

impl UsageSource for codex::CodexReader {
    fn platform(&self) -> Platform {
        Platform::Codex
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        JsonlReader::scan_all(self)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        JsonlReader::poll_delta(self)
    }
}
```

- [ ] **Step 2: Add the failing dispatch test in `src/state/app_state.rs`**

In the existing `#[cfg(test)] mod tests` block (which already has the `rec(...)` helper), add:

```rust
    #[test]
    fn add_records_dispatches_by_platform() {
        let mut s = AppState::with_capacity(10);
        s.add_records(Platform::ClaudeCode, vec![rec("opus-4", 100, 50, 1.0)]);
        s.add_records(Platform::Codex, vec![rec("gpt-5", 200, 80, 2.0)]);
        assert_eq!(s.claude_total_calls, 1);
        assert_eq!(s.codex_total_calls, 1);
        assert!(s.claude_sessions.contains_key("opus-4"));
        assert!(s.codex_sessions.contains_key("gpt-5"));
    }
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --quiet add_records_dispatches_by_platform`
Expected: FAIL — `no method named add_records found`.

- [ ] **Step 4: Add `add_records` to `impl AppState` in `src/state/app_state.rs`**

Add this method to the `impl AppState` block (e.g. just above `pub fn clear_claude`):

```rust
    /// Route a batch of records to the bucket for `platform`. Every batch from
    /// a single reader is one platform, so this just dispatches.
    pub fn add_records(&mut self, platform: Platform, records: Vec<UsageRecord>) {
        match platform {
            Platform::ClaudeCode => self.add_claude_records(records),
            Platform::Codex => self.add_codex_records(records),
        }
    }
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --quiet add_records_dispatches_by_platform`
Expected: PASS.

- [ ] **Step 6: Refactor the reader tasks in `src/main.rs`**

Add to the imports at the top of `src/main.rs` (with the other `use crate::...` lines):

```rust
use crate::reader::UsageSource;
```

Replace the two reader-task blocks (the `// Claude reader task ...` block through the end of the `// Codex reader task ...` block — everything from `let claude_state = app_state.clone();` down to the closing `});` of the codex task) with this single block:

```rust
    // Reader tasks: one per usage source, all driven uniformly via UsageSource.
    let sources: Vec<Arc<std::sync::Mutex<Box<dyn UsageSource>>>> = vec![
        Arc::new(std::sync::Mutex::new(
            Box::new(ClaudeReader::new(claude_path.clone())) as Box<dyn UsageSource>,
        )),
        Arc::new(std::sync::Mutex::new(
            Box::new(CodexReader::new(codex_path.clone())) as Box<dyn UsageSource>,
        )),
    ];
    let mut reader_handles = Vec::new();
    for source in &sources {
        let source = source.clone();
        let reader_state = app_state.clone();
        let refresh_interval = refresh;
        let platform = source.lock().unwrap_or_else(|e| e.into_inner()).platform();
        reader_handles.push(task::spawn(async move {
            // Initial scan
            let s = source.clone();
            let initial = task::spawn_blocking(move || {
                s.lock().unwrap_or_else(|e| e.into_inner()).scan_all()
            })
            .await
            .unwrap_or_default();
            info!("{:?}: Found {} initial records", platform, initial.len());
            if !initial.is_empty()
                && let Ok(mut state) = reader_state.write() {
                    state.add_records(platform, initial);
                }

            let mut interval = tokio::time::interval(Duration::from_secs(refresh_interval));
            loop {
                interval.tick().await;
                let s = source.clone();
                let new_records = task::spawn_blocking(move || {
                    s.lock().unwrap_or_else(|e| e.into_inner()).poll_delta()
                })
                .await
                .unwrap_or_default();
                if !new_records.is_empty() {
                    info!("{:?}: Found {} new records", platform, new_records.len());
                    if let Ok(mut state) = reader_state.write() {
                        state.add_records(platform, new_records);
                    }
                }
            }
        }));
    }
```

Then, at the shutdown section near the end of `main`, replace:

```rust
    claude_handle.abort();
    codex_handle.abort();
    quota_handle.abort();
```

with:

```rust
    for handle in &reader_handles {
        handle.abort();
    }
    quota_handle.abort();
```

- [ ] **Step 7: Build and run the full test suite**

Run: `cargo build --quiet && cargo test --quiet && cargo clippy --quiet`
Expected: builds clean, all tests pass, no clippy warnings.

- [ ] **Step 8: Commit**

```bash
git add src/reader/mod.rs src/state/app_state.rs src/main.rs
git commit -m "refactor: unify readers behind a UsageSource trait

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Add `OpenCode` enum variants + all consumers (empty third tab)

After this the app shows three tabs; the opencode tab is empty (no reader yet) and its quota line reads `no quota data`.

**Files:**
- Modify: `src/state/app_state.rs` (Platform, Tab, fields, init, `add_opencode_records`, `add_records` arm, `clear_opencode`)
- Modify: `src/ui/tabs.rs` (render three tabs)
- Modify: `src/ui/quota_bar.rs` (`no_quota_source` line)
- Modify: `src/ui/mod.rs` (render data match arm + opencode quota branch + smoke test)
- Modify: `src/main.rs` (run_tui `r`-clear match arm)

- [ ] **Step 1: Extend `Platform` and `Tab` in `src/state/app_state.rs`**

Change the enums:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    ClaudeCode,
    Codex,
    OpenCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    ClaudeCode,
    Codex,
    OpenCode,
}
```

Replace `Tab::next`, `Tab::prev`, `Tab::label`, `Tab::primary_color`, and `Tab::secondary_color` with three-way versions:

```rust
    pub fn next(self) -> Self {
        match self {
            Tab::ClaudeCode => Tab::Codex,
            Tab::Codex => Tab::OpenCode,
            Tab::OpenCode => Tab::ClaudeCode,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::ClaudeCode => Tab::OpenCode,
            Tab::Codex => Tab::ClaudeCode,
            Tab::OpenCode => Tab::Codex,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::ClaudeCode => "CLAUDE",
            Tab::Codex => "CODEX",
            Tab::OpenCode => "OPENCODE",
        }
    }

    pub fn primary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(255, 165, 0), // Orange
            Tab::Codex => ratatui::style::Color::Rgb(59, 130, 246),     // Blue
            Tab::OpenCode => ratatui::style::Color::Rgb(16, 185, 129),  // Emerald
        }
    }

    #[allow(dead_code)]
    pub fn secondary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(255, 200, 100),
            Tab::Codex => ratatui::style::Color::Rgb(147, 197, 253),
            Tab::OpenCode => ratatui::style::Color::Rgb(110, 231, 183),
        }
    }
```

- [ ] **Step 2: Add opencode fields to `AppState` + init in `src/state/app_state.rs`**

In `struct AppState`, after the Codex fields block (after `pub codex_max_records: usize,`) add:

```rust
    // opencode
    pub opencode_records: VecDeque<UsageRecord>,
    pub opencode_sessions: HashMap<String, SessionSummary>,
    pub opencode_total_calls: usize,
    pub opencode_total_cost: f64,
    pub opencode_quota: Option<QuotaInfo>,
    pub opencode_max_records: usize,
```

In `with_capacity`, after the codex initializers (after `codex_max_records: max_records,`) add:

```rust
            opencode_records: VecDeque::with_capacity(max_records),
            opencode_sessions: HashMap::new(),
            opencode_total_calls: 0,
            opencode_total_cost: 0.0,
            opencode_quota: None,
            opencode_max_records: max_records,
```

- [ ] **Step 3: Add `add_opencode_records`, the `add_records` arm, and `clear_opencode`**

Add `add_opencode_records` right after `add_codex_records` (mirror it exactly):

```rust
    pub fn add_opencode_records(&mut self, records: Vec<UsageRecord>) {
        for r in records {
            // Lifetime totals stay cumulative; only the windowed per-model
            // aggregate is reversed on eviction (see add_claude_records).
            if self.opencode_records.len() >= self.opencode_max_records
                && let Some(old) = self.opencode_records.pop_front() {
                    reverse_model_aggregate(&mut self.opencode_sessions, &old);
                }
            self.opencode_total_cost += r.cost_usd;
            self.opencode_total_calls += 1;
            upsert_model_aggregate(&mut self.opencode_sessions, &r);
            self.opencode_records.push_back(r);
        }
    }
```

Add the `OpenCode` arm to `add_records`:

```rust
            Platform::OpenCode => self.add_opencode_records(records),
```

Add `clear_opencode` after `clear_codex`:

```rust
    pub fn clear_opencode(&mut self) {
        self.opencode_records.clear();
        self.opencode_sessions.clear();
        self.opencode_total_calls = 0;
        self.opencode_total_cost = 0.0;
    }
```

- [ ] **Step 4: Render three tabs in `src/ui/tabs.rs`**

Replace the body of `tab_line`:

```rust
pub fn tab_line(active: Tab) -> Paragraph<'static> {
    let line = Line::from(vec![
        tab_span(Tab::ClaudeCode, active == Tab::ClaudeCode),
        Span::raw("  "),
        tab_span(Tab::Codex, active == Tab::Codex),
        Span::raw("  "),
        tab_span(Tab::OpenCode, active == Tab::OpenCode),
    ]);
    Paragraph::new(line)
}
```

- [ ] **Step 5: Add a `no_quota_source` line in `src/ui/quota_bar.rs`**

Append to `src/ui/quota_bar.rs` (before the `#[cfg(test)]` module if present, else at end):

```rust
/// Quota line for a tab that has no quota source at all (opencode today).
/// Distinct from the `loading…` shown while a Claude/Codex quota is in flight.
pub fn no_quota_source() -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(
        " no quota data",
        Style::default().fg(Color::DarkGray),
    )))
}
```

- [ ] **Step 6: Add the opencode arm + quota branch in `src/ui/mod.rs`**

In `render`, extend the active-tab data `match active` with an opencode arm (after the `Codex` arm):

```rust
        crate::state::Tab::OpenCode => (
            state.opencode_quota.as_ref(),
            &state.opencode_sessions,
            &state.opencode_records,
            state.opencode_total_calls,
            state.opencode_total_cost,
        ),
```

Replace the quota-panel render line:

```rust
    frame.render_widget(quota_bar::quota_panel(active, quota), chunks[1]);
```

with:

```rust
    let quota_widget = match active {
        crate::state::Tab::OpenCode => quota_bar::no_quota_source(),
        _ => quota_bar::quota_panel(active, quota),
    };
    frame.render_widget(quota_widget, chunks[1]);
```

- [ ] **Step 7: Add the `r`-clear arm in `src/main.rs` run_tui**

In the `KeyCode::Char('r')` handler, extend the match:

```rust
                            match state.active_tab {
                                state::Tab::ClaudeCode => state.clear_claude(),
                                state::Tab::Codex => state.clear_codex(),
                                state::Tab::OpenCode => state.clear_opencode(),
                            }
```

- [ ] **Step 8: Add a render smoke test in `src/ui/mod.rs`**

In the `#[cfg(test)] mod tests` block, after `renders_without_panicking`, add:

```rust
    #[test]
    fn renders_opencode_tab_empty_with_no_quota() {
        let mut s = sample_state();
        s.active_tab = crate::state::Tab::OpenCode;
        let out = dump(80, 18, s);
        assert!(out.contains("OPENCODE")); // third tab is present
        assert!(out.contains("no quota data")); // opencode has no quota source
    }
```

- [ ] **Step 9: Build, test, clippy**

Run: `cargo build --quiet && cargo test --quiet && cargo clippy --quiet`
Expected: builds clean, all tests pass (including the two new ones), no clippy warnings.

- [ ] **Step 10: Commit**

```bash
git add src/state/app_state.rs src/ui/tabs.rs src/ui/quota_bar.rs src/ui/mod.rs src/main.rs
git commit -m "feat: add OpenCode tab scaffolding (Platform/Tab/state/UI)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `OpencodeReader` — read opencode.db (TDD, in-memory)

The reader is standalone and fully tested here; it is wired into `main` in Task 4.

**Files:**
- Create: `src/reader/opencode.rs`
- Modify: `src/reader/mod.rs` (`pub mod opencode;`)

- [ ] **Step 1: Register the module in `src/reader/mod.rs`**

Add near the other `pub mod` lines at the top:

```rust
pub mod opencode;
```

- [ ] **Step 2: Write `src/reader/opencode.rs` with the failing tests first**

Create the file with the full implementation AND tests (write it in one go; the tests are the spec for the parser):

```rust
use crate::reader::{basename, session_label, UsageSource};
use crate::state::{Platform, UsageRecord};
use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::PathBuf;

/// Reads per-call usage from opencode's local SQLite DB (`opencode.db`). Each
/// assistant `message` row becomes one `UsageRecord`. Opened read-only; if the
/// DB is missing or unreadable, every method returns empty (opencode absent).
pub struct OpencodeReader {
    conn: Option<Connection>,
    /// Max `message.time_created` (epoch ms) seen so far — the poll cursor.
    cursor: i64,
}

impl OpencodeReader {
    pub fn new(data_dir: PathBuf) -> Self {
        let db_path = data_dir.join("opencode.db");
        // Read-only: we never write opencode's DB. WAL mode permits this
        // concurrent reader to see committed rows (verified against a live
        // install).
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

    /// Query assistant messages with `time_created > cursor`, advancing the
    /// cursor to the max timestamp seen.
    fn query_since(&mut self, cursor: i64) -> Vec<UsageRecord> {
        let Some(conn) = self.conn.as_ref() else {
            return Vec::new();
        };
        let mut stmt = match conn.prepare(
            "SELECT m.data, s.directory, m.session_id, m.time_created \
             FROM message m JOIN session s ON m.session_id = s.id \
             WHERE json_extract(m.data, '$.role') = 'assistant' \
               AND m.time_created > ?1 \
             ORDER BY m.time_created",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([cursor], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        });
        let mut records = Vec::new();
        let mut max_seen = cursor;
        if let Ok(rows) = rows {
            for (data, directory, session_id, time_created) in rows.flatten() {
                if time_created > max_seen {
                    max_seen = time_created;
                }
                if let Some(rec) =
                    parse_opencode_message(&data, &directory, &session_id, time_created)
                {
                    records.push(rec);
                }
            }
        }
        self.cursor = max_seen;
        records
    }
}

impl UsageSource for OpencodeReader {
    fn platform(&self) -> Platform {
        Platform::OpenCode
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

/// Parse one assistant `message.data` JSON blob into a `UsageRecord`. Returns
/// `None` for rows without token usage (errors, aborted calls).
fn parse_opencode_message(
    data: &str,
    directory: &str,
    session_id: &str,
    time_created_ms: i64,
) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(data).ok()?;
    let tokens = v.get("tokens")?;
    let u64_at = |obj: &Value, key: &str| obj.get(key).and_then(|x| x.as_u64()).unwrap_or(0);

    let input = u64_at(tokens, "input");
    let output = u64_at(tokens, "output");
    let reasoning = u64_at(tokens, "reasoning");
    let (cache_read, cache_write) = match tokens.get("cache") {
        Some(cache) => (u64_at(cache, "read"), u64_at(cache, "write")),
        None => (0, 0),
    };

    // Skip no-op rows, mirroring the Claude parser.
    if input == 0 && output == 0 && reasoning == 0 && cache_read == 0 && cache_write == 0 {
        return None;
    }

    let model_id = v.get("modelID").and_then(|x| x.as_str()).unwrap_or("unknown");
    let provider_id = v
        .get("providerID")
        .and_then(|x| x.as_str())
        .unwrap_or("opencode");
    let model = format!("{provider_id}/{model_id}");
    let cost = v.get("cost").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let timestamp = Utc.timestamp_millis_opt(time_created_ms).single()?;
    let session = session_label(&basename(directory), session_id);

    Some(UsageRecord {
        timestamp,
        platform: Platform::OpenCode,
        model,
        session,
        input_tokens: input,
        // opencode tracks reasoning separately; fold it into output (it is
        // generated/billed as output) so the OUTPUT column reflects all of it.
        output_tokens: output + reasoning,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        cost_usd: cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASSISTANT_1: &str = r#"{"role":"assistant","modelID":"kimi-k2.6:cloud","providerID":"ollama","cost":0.0,"tokens":{"input":100,"output":40,"reasoning":10,"cache":{"read":5,"write":2}},"time":{"created":1000}}"#;
    const ASSISTANT_2: &str = r#"{"role":"assistant","modelID":"minimax-m3","providerID":"opencode-go","cost":1.5,"tokens":{"input":200,"output":80,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":2000}}"#;
    const USER_MSG: &str = r#"{"role":"user","time":{"created":1500}}"#;
    const ASSISTANT_NO_TOKENS: &str = r#"{"role":"assistant","modelID":"x","providerID":"y","cost":0.0,"tokens":{"input":0,"output":0,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1700}}"#;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT NOT NULL);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);
             INSERT INTO session VALUES ('ses_abc', '/Users/me/myproject');",
        )
        .unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, t: i64, data: &str) {
        conn.execute(
            "INSERT INTO message VALUES (?1, 'ses_abc', ?2, ?3)",
            rusqlite::params![id, t, data],
        )
        .unwrap();
    }

    #[test]
    fn scan_all_parses_assistant_messages_and_skips_others() {
        let conn = setup();
        insert(&conn, "m1", 1000, ASSISTANT_1);
        insert(&conn, "m2", 1500, USER_MSG);
        insert(&conn, "m3", 1700, ASSISTANT_NO_TOKENS);
        insert(&conn, "m4", 2000, ASSISTANT_2);
        let mut reader = OpencodeReader::from_connection(conn);

        let records = reader.scan_all();
        assert_eq!(records.len(), 2); // user + tokenless rows skipped

        let r1 = &records[0];
        assert_eq!(r1.model, "ollama/kimi-k2.6:cloud");
        assert_eq!(r1.input_tokens, 100);
        assert_eq!(r1.output_tokens, 50); // 40 output + 10 reasoning
        assert_eq!(r1.cache_read_tokens, 5);
        assert_eq!(r1.cache_creation_tokens, 2);
        assert_eq!(r1.cost_usd, 0.0);
        assert_eq!(r1.session, "myproject ses_abc");
        assert_eq!(r1.platform, Platform::OpenCode);

        let r2 = &records[1];
        assert_eq!(r2.model, "opencode-go/minimax-m3");
        assert_eq!(r2.cost_usd, 1.5);
    }

    #[test]
    fn poll_delta_returns_only_new_rows() {
        let conn = setup();
        insert(&conn, "m1", 1000, ASSISTANT_1);
        let mut reader = OpencodeReader::from_connection(conn);

        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0); // nothing new

        // A newer assistant message arrives.
        let conn2 = reader.conn.as_ref().unwrap();
        conn2
            .execute(
                "INSERT INTO message VALUES ('m2', 'ses_abc', 2000, ?1)",
                rusqlite::params![ASSISTANT_2],
            )
            .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].model, "opencode-go/minimax-m3");
    }

    #[test]
    fn missing_db_yields_empty() {
        let mut reader = OpencodeReader::new(PathBuf::from("/nonexistent/path"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }
}
```

- [ ] **Step 3: Run the reader tests to verify they pass**

Run: `cargo test --quiet reader::opencode`
Expected: PASS — `scan_all_parses_assistant_messages_and_skips_others`, `poll_delta_returns_only_new_rows`, `missing_db_yields_empty`.

(If `from_connection`'s `conn` field access in the second test triggers a private-field warning, it is in the same module so it is allowed; no change needed.)

- [ ] **Step 4: Build, test, clippy**

Run: `cargo build --quiet && cargo test --quiet && cargo clippy --quiet`
Expected: clean. `OpencodeReader` is not yet referenced by `main`, so clippy may flag `OpencodeReader::new` as unused — that is resolved in Task 4. If clippy fails the build on this in the interim, proceed to Task 4 before the final clippy gate.

- [ ] **Step 5: Commit**

```bash
git add src/reader/opencode.rs src/reader/mod.rs
git commit -m "feat: add OpencodeReader (read-only SQLite usage source)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Config + CLI `opencode_path` + wire reader into main

Now opencode usage actually flows into the tab.

**Files:**
- Modify: `src/config.rs` (field + default)
- Modify: `src/cli.rs` (`--opencode-path`)
- Modify: `src/main.rs` (merge path, add reader to sources, config show/set)

- [ ] **Step 1: Add `opencode_path` to `Config` in `src/config.rs`**

In `struct Config`, after the `codex_path` field add:

```rust
    /// Path to opencode data directory (contains opencode.db)
    #[serde(default = "default_opencode_path")]
    pub opencode_path: PathBuf,
```

In `impl Default for Config`, after `codex_path: default_codex_path(),` add:

```rust
            opencode_path: default_opencode_path(),
```

After `default_codex_path()` add:

```rust
fn default_opencode_path() -> PathBuf {
    // opencode uses the XDG data dir on every platform (NOT macOS's
    // ~/Library/Application Support), so we resolve it ourselves rather than
    // via dirs::data_dir().
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/share")
        })
        .join("opencode")
}
```

- [ ] **Step 2: Add a default-path test in `src/config.rs`**

In the `#[cfg(test)] mod tests`, add:

```rust
    #[test]
    fn default_opencode_path_ends_with_opencode() {
        let p = default_opencode_path();
        assert!(p.ends_with("opencode"), "got {p:?}");
    }
```

Run: `cargo test --quiet default_opencode_path_ends_with_opencode`
Expected: PASS.

- [ ] **Step 3: Add `--opencode-path` to `src/cli.rs`**

In `struct Cli`, after the `codex_path` arg add:

```rust
    /// Path to opencode data directory
    #[arg(long)]
    pub opencode_path: Option<PathBuf>,
```

- [ ] **Step 4: Merge the path and add the reader in `src/main.rs`**

After `let codex_path = args.codex_path.unwrap_or(config.codex_path);` add:

```rust
    let opencode_path = args.opencode_path.unwrap_or(config.opencode_path);
```

Add an `info!` next to the others:

```rust
    info!("Monitoring opencode at {:?}", opencode_path);
```

In the `sources` vec, add a third entry after the Codex one:

```rust
        Arc::new(std::sync::Mutex::new(
            Box::new(reader::opencode::OpencodeReader::new(opencode_path.clone()))
                as Box<dyn UsageSource>,
        )),
```

- [ ] **Step 5: Handle `opencode_path` in config show/set in `src/main.rs`**

In `handle_config`, in the `Set { key, value }` match add the arm (next to `codex_path`):

```rust
                "opencode_path" => config.opencode_path = std::path::PathBuf::from(value),
```

Update the `Available keys:` message to include `opencode_path`:

```rust
                    eprintln!("Available keys: claude_path, codex_path, opencode_path, refresh, max_records");
```

In the `Show` branch, add a line printing the opencode path next to the claude/codex prints (match the existing format, e.g.):

```rust
            println!("opencode_path = {:?}", config.opencode_path);
```

- [ ] **Step 6: Build, test, clippy**

Run: `cargo build --quiet && cargo test --quiet && cargo clippy --quiet`
Expected: builds clean, all tests pass, no clippy warnings (the unused-`OpencodeReader::new` warning is now gone).

- [ ] **Step 7: Manual smoke check (if opencode is installed locally)**

Run: `cargo run -- --refresh 5` and press Tab twice to reach OPENCODE; confirm models/sessions populate (or empty if no data) and the quota line shows `no quota data`. Press `q` to quit.

- [ ] **Step 8: Commit**

```bash
git add src/config.rs src/cli.rs src/main.rs
git commit -m "feat: wire opencode reader via config/CLI opencode_path

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Reserve the opencode quota slot

**Files:**
- Create: `src/quota/opencode.rs`
- Modify: `src/quota/mod.rs` (`pub mod opencode;`)

- [ ] **Step 1: Create `src/quota/opencode.rs`**

```rust
use super::QuotaInfo;

/// Reserved slot for opencode-go (Zen/Go) quota. There is no public
/// balance/usage API today — see github.com/anomalyco/opencode issues
/// #16017 (Go plan usage) and #10448 (Zen balance). When one ships, build a
/// `QuotaInfo` here and call this from the quota task in `main.rs`.
#[allow(dead_code)]
pub fn fetch_quota() -> Option<QuotaInfo> {
    None
}
```

- [ ] **Step 2: Register the module in `src/quota/mod.rs`**

Add near the top with the other `pub mod` lines:

```rust
pub mod opencode;
```

- [ ] **Step 3: Build, test, clippy**

Run: `cargo build --quiet && cargo test --quiet && cargo clippy --quiet`
Expected: clean (the `#[allow(dead_code)]` suppresses the unused-`fetch_quota` warning).

- [ ] **Step 4: Commit**

```bash
git add src/quota/opencode.rs src/quota/mod.rs
git commit -m "feat: reserve opencode quota slot (no public API yet)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Done criteria

- Three tabs: CLAUDE / CODEX / OPENCODE; Tab/Left/Right cycle all three.
- The opencode tab shows per-model and per-session usage from `opencode.db`
  (per-call granularity), and `no quota data` in the quota row.
- `cargo build`, `cargo test`, `cargo clippy` all clean.
- `--opencode-path` / `config set opencode_path` override the default
  `~/.local/share/opencode` (XDG-aware).
