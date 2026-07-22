use crate::quota::QuotaInfo;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use lasso::{Spur, ThreadedRodeo};
use std::sync::OnceLock;

pub type InternedString = Spur;
pub static INTERNER: OnceLock<ThreadedRodeo> = OnceLock::new();

pub fn get_interner() -> &'static ThreadedRodeo {
    INTERNER.get_or_init(ThreadedRodeo::new)
}

pub fn intern(s: &str) -> InternedString {
    get_interner().get_or_intern(s)
}

pub fn resolve(key: InternedString) -> &'static str {
    get_interner().resolve(&key)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CompactDate(u32);

impl CompactDate {
    pub fn new(year: u16, month: u8, day: u8) -> Self {
        Self(((year as u32) << 16) | ((month as u32) << 8) | day as u32)
    }

    pub fn from_datetime(dt: chrono::DateTime<chrono::Utc>) -> Self {
        use chrono::Datelike;
        let naive = dt.date_naive();
        Self::new(naive.year() as u16, naive.month() as u8, naive.day() as u8)
    }
}
impl std::fmt::Display for CompactDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let year = (self.0 >> 16) & 0xFFFF;
        let month = (self.0 >> 8) & 0xFF;
        let day = self.0 & 0xFF;
        write!(f, "{:04}-{:02}-{:02}", year, month, day)
    }
}

impl serde::Serialize for CompactDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Resolved agent data paths (from config + CLI). Used for tab detection so
/// custom paths are honored instead of always checking defaults.
#[derive(Debug, Clone)]
pub struct AgentPaths {
    paths: HashMap<Tab, PathBuf>,
}

impl AgentPaths {
    pub fn new(paths: HashMap<Tab, PathBuf>) -> Self {
        Self { paths }
    }

    pub fn path_for(&self, tab: Tab) -> PathBuf {
        self.paths
            .get(&tab)
            .cloned()
            .unwrap_or_else(|| {
                tracing::warn!("AgentPaths missing {tab:?} — falling back to default path; use platforms::resolve_paths");
                tab.default_path()
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    ClaudeCode,
    Codex,
    Cursor,
}

impl Platform {
    /// Number of platform variants — used to size the fixed array in `AppState`.
    pub const COUNT: usize = 3;

    /// Zero-based index for array access. Must stay in sync with variant order.
    pub const fn index(self) -> usize {
        match self {
            Platform::ClaudeCode => 0,
            Platform::Codex => 1,
            Platform::Cursor => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    ClaudeCode,
    Codex,
    Cursor,
}

impl Tab {
    /// Zero-based index for array access. Mirrors `Platform::index()` by
    /// construction — the two enums share variant order.
    pub const fn index(self) -> usize {
        match self {
            Tab::ClaudeCode => 0,
            Tab::Codex => 1,
            Tab::Cursor => 2,
        }
    }

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

    pub fn label(self) -> &'static str {
        match self {
            Tab::ClaudeCode => "CLAUDE",
            Tab::Codex => "CODEX",
            Tab::Cursor => "CURSOR",
        }
    }

    /// Primary color for the tab (used for borders and accents).
    /// Sourced from each agent's official CLI theme / brand palette.
    pub fn primary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(217, 119, 87),
            Tab::Codex => ratatui::style::Color::Rgb(215, 95, 215),
            Tab::Cursor => ratatui::style::Color::Rgb(136, 192, 208),
        }
    }

    /// Whether this platform has a quota API endpoint (Claude + Codex only).
    /// Map from the corresponding `Platform` variant. The two enums share
    /// variant order, so this is a constant-time match.
    pub const fn from_platform(p: Platform) -> Self {
        match p {
            Platform::ClaudeCode => Tab::ClaudeCode,
            Platform::Codex => Tab::Codex,
            Platform::Cursor => Tab::Cursor,
        }
    }

    pub const fn has_quota_api(self) -> bool {
        matches!(self, Tab::ClaudeCode | Tab::Codex | Tab::Cursor)
    }

    pub fn all() -> &'static [Tab] {
        &[Tab::ClaudeCode, Tab::Codex, Tab::Cursor]
    }

    pub fn default_path(self) -> std::path::PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        match self {
            Tab::ClaudeCode => home.join(".claude/projects"),
            Tab::Codex => home.join(".codex"),
            Tab::Cursor => home.join(".cursor"),
        }
    }

    /// Shared detection logic: does the agent's data exist at this path?
    fn is_available_at_path(path: &std::path::Path, tab: Tab) -> bool {
        match tab {
            Tab::Cursor => path.join("projects").exists() || path.join("chats").exists(),
            _ => path.exists(),
        }
    }

    pub fn is_available_at(self, paths: &AgentPaths) -> bool {
        let path = paths.path_for(self);
        Self::is_available_at_path(&path, self)
    }
}

/// Single API call record
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    #[allow(dead_code)]
    pub platform: Platform,
    pub model: InternedString,
    pub session: InternedString,
    /// Full session/conversation id used to resume the agent CLI (Claude
    /// `sessionId`, Codex `session_meta.id`, Cursor `conversation_id`). The
    /// `session` field above is only a display label; this is the real key.
    pub session_id: InternedString,
    /// Absolute working directory the session ran in, so a resume launches in
    /// the right project. Empty when the source doesn't record a real path
    /// (Cursor exposes a display-only project label, not a filesystem path).
    pub cwd: InternedString,
    /// Stable identity used to dedup re-emitted records (e.g. after a file
    /// truncation/rewrite forces a reader to re-read from byte 0). Readers
    /// build this from the strongest identifier their source data carries —
    /// a message/event id, a database primary key, or (when nothing better
    /// exists) a hash of the record's own content. See `AppState::add_records`.
    pub id: InternedString,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
}

/// Aggregated per-model totals.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub model: InternedString,
    pub total_input: u64,
    pub total_output: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
    pub total_cost: f64,
    pub request_count: u64,
}

/// One selectable/launchable session row: display label plus the real
/// identifiers needed to resume the agent CLI. Built by aggregating the recent
/// records; the single source of truth for both the sessions table render and
/// the launch action, so the highlighted row and the resumed session can never
/// disagree.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub label: &'static str,
    pub session_id: InternedString,
    /// Absolute working dir, or empty when the source has no real path.
    pub cwd: InternedString,
    pub tokens: u64,
    pub requests: u64,
}

/// Per-platform state held in a `[PlatformState; Platform::COUNT]` array.
#[derive(Debug)]
pub struct PlatformState {
    pub records: VecDeque<UsageRecord>,
    pub sessions: HashMap<InternedString, SessionSummary>,
    pub total_calls: usize,
    pub total_cost: f64,
    pub quota: Option<QuotaInfo>,
    pub max_records: usize,
    /// Ids of every record ever ingested for this platform, so a reader
    /// re-emitting the same record (e.g. after a truncation-triggered
    /// re-read from byte 0) doesn't double-count it.
    // ponytail: grows for the process lifetime rather than tracking only the
    // bounded `records` window — bounding it would let a record evicted from
    // `records` be double-counted if the same file is re-read later. A Spur
    // is 4 bytes, so even a long-running session ingesting millions of
    // records stays well under the ceiling where this would matter.
    seen_ids: HashSet<InternedString>,
}

impl PlatformState {
    /// Borrow references for the UI render pass.
    pub fn refs(
        &self,
    ) -> (
        Option<&QuotaInfo>,
        &HashMap<InternedString, SessionSummary>,
        &VecDeque<UsageRecord>,
        usize,
        f64,
    ) {
        (
            self.quota.as_ref(),
            &self.sessions,
            &self.records,
            self.total_calls,
            self.total_cost,
        )
    }

    /// Sessions aggregated from the recent records, highest usage first
    /// (ties broken by label so the order is stable across live refreshes —
    /// the cursor won't jump when equal-usage rows re-sort).
    pub fn session_entries(&self) -> Vec<SessionEntry> {
        struct Acc {
            tokens: u64,
            requests: u64,
            session_id: InternedString,
            cwd: InternedString,
        }
        let mut by: HashMap<InternedString, Acc> = HashMap::new();
        for r in &self.records {
            let e = by.entry(r.session).or_insert(Acc {
                tokens: 0,
                requests: 0,
                session_id: r.session_id,
                cwd: r.cwd,
            });
            e.tokens +=
                r.input_tokens + r.output_tokens + r.cache_read_tokens + r.cache_creation_tokens;
            e.requests += 1;
            // Prefer a real id/path if a later record in the group carries one.
            if resolve(e.session_id).is_empty() {
                e.session_id = r.session_id;
            }
            if resolve(e.cwd).is_empty() {
                e.cwd = r.cwd;
            }
        }
        let mut entries: Vec<SessionEntry> = by
            .into_iter()
            .map(|(label, a)| SessionEntry {
                label: resolve(label),
                session_id: a.session_id,
                cwd: a.cwd,
                tokens: a.tokens,
                requests: a.requests,
            })
            .collect();
        entries.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.label.cmp(b.label)));
        entries
    }
}

/// Global application state
pub struct AppState {
    pub platforms: [PlatformState; Platform::COUNT],
    pub active_tab: Tab,
    pub available_tabs: Vec<Tab>,
    /// Whether keyboard focus is on the sessions list (arrow keys move the
    /// selection and Enter launches, instead of switching tabs).
    pub sessions_focused: bool,
    /// The selected session, tracked by `session_id` rather than row index so
    /// live re-sorting of the list never moves the cursor onto a different
    /// session between selecting and pressing Enter.
    pub selected_session: Option<InternedString>,
}

impl AppState {
    pub fn with_capacity(max_records: usize) -> Self {
        let max_records = max_records.max(1);
        let make = || PlatformState {
            records: VecDeque::with_capacity(max_records),
            sessions: HashMap::new(),
            total_calls: 0,
            total_cost: 0.0,
            quota: None,
            max_records,
            seen_ids: HashSet::new(),
        };
        Self {
            // ponytail: size-agnostic init, tracks Platform::COUNT automatically
            platforms: std::array::from_fn(|_| make()),
            active_tab: Tab::ClaudeCode,
            available_tabs: Vec::new(),
            sessions_focused: false,
            selected_session: None,
        }
    }

    /// Sessions of the active tab, highest usage first.
    pub fn active_session_entries(&self) -> Vec<SessionEntry> {
        self.platform(self.active_tab).session_entries()
    }

    /// Enter session-selection mode, ensuring a valid row is highlighted.
    pub fn focus_sessions(&mut self) {
        let entries = self.active_session_entries();
        if entries.is_empty() {
            self.selected_session = None;
            return;
        }
        self.sessions_focused = true;
        let valid = self
            .selected_session
            .is_some_and(|sid| entries.iter().any(|e| e.session_id == sid));
        if !valid {
            self.selected_session = Some(entries[0].session_id);
        }
    }

    /// Leave session-selection mode; the selection is remembered.
    pub fn unfocus_sessions(&mut self) {
        self.sessions_focused = false;
    }

    /// Move the selection by `delta` rows within the active tab, wrapping.
    pub fn move_selection(&mut self, delta: i32) {
        let entries = self.active_session_entries();
        if entries.is_empty() {
            self.selected_session = None;
            return;
        }
        let cur = self
            .selected_session
            .and_then(|sid| entries.iter().position(|e| e.session_id == sid));
        let next = match cur {
            Some(i) => (i as i32 + delta).rem_euclid(entries.len() as i32) as usize,
            None => 0,
        };
        self.selected_session = Some(entries[next].session_id);
    }

    /// The currently selected session's `(session_id, cwd)`, if any. Empty
    /// strings mean "unknown" (Cursor has no real cwd).
    pub fn selected_launch(&self) -> Option<(InternedString, InternedString)> {
        let sid = self.selected_session?;
        self.active_session_entries()
            .iter()
            .find(|e| e.session_id == sid)
            .map(|e| (e.session_id, e.cwd))
    }

    /// Reset session focus/selection — call when the visible list changes
    /// wholesale (tab switch, tab clear) so a stale selection can't launch.
    pub fn reset_session_focus(&mut self) {
        self.sessions_focused = false;
        self.selected_session = None;
    }

    pub fn new() -> Self {
        Self::with_capacity(100)
    }

    pub fn platform(&self, tab: Tab) -> &PlatformState {
        &self.platforms[tab.index()]
    }

    pub fn platform_mut(&mut self, tab: Tab) -> &mut PlatformState {
        &mut self.platforms[tab.index()]
    }

    pub fn add_records(&mut self, platform: Platform, records: Vec<UsageRecord>) {
        let p = &mut self.platforms[platform.index()];
        for r in records {
            // A reader re-emitting a record it already delivered (e.g. after a
            // truncation forced a re-read from byte 0) must not be counted
            // twice — see `UsageRecord::id` and `PlatformState::seen_ids`.
            if !p.seen_ids.insert(r.id) {
                continue;
            }
            if p.records.len() >= p.max_records
                && let Some(old) = p.records.pop_front()
            {
                reverse_model_aggregate(&mut p.sessions, &old);
            }
            p.total_cost += r.cost_usd;
            p.total_calls += 1;
            upsert_model_aggregate(&mut p.sessions, &r);
            p.records.push_back(r);
        }
    }

    pub fn clear_tab(&mut self, tab: Tab) {
        let p = self.platform_mut(tab);
        p.records.clear();
        p.sessions.clear();
        p.total_calls = 0;
        p.total_cost = 0.0;
        p.seen_ids.clear();
    }

    pub fn detect_available_tabs(&mut self, paths: &AgentPaths) {
        self.available_tabs = Tab::all()
            .iter()
            .filter(|tab| tab.is_available_at(paths))
            .copied()
            .collect();

        if !self.available_tabs.contains(&self.active_tab) {
            self.active_tab = self
                .available_tabs
                .first()
                .copied()
                .unwrap_or(Tab::ClaudeCode);
        }
    }
}

fn upsert_model_aggregate(map: &mut HashMap<InternedString, SessionSummary>, r: &UsageRecord) {
    let entry = map
        .entry(r.model) // r.model is Spur (Copy), no clone needed!
        .or_insert_with(|| SessionSummary {
            model: r.model,
            total_input: 0,
            total_output: 0,
            total_cache_read: 0,
            total_cache_creation: 0,
            total_cost: 0.0,
            request_count: 0,
        });
    entry.total_input += r.input_tokens;
    entry.total_output += r.output_tokens;
    entry.total_cache_read += r.cache_read_tokens;
    entry.total_cache_creation += r.cache_creation_tokens;
    entry.total_cost += r.cost_usd;
    entry.request_count += 1;
}

fn reverse_model_aggregate(map: &mut HashMap<InternedString, SessionSummary>, r: &UsageRecord) {
    if let std::collections::hash_map::Entry::Occupied(mut entry) = map.entry(r.model) {
        let s = entry.get_mut();
        s.total_input = s.total_input.saturating_sub(r.input_tokens);
        s.total_output = s.total_output.saturating_sub(r.output_tokens);
        s.total_cache_read = s.total_cache_read.saturating_sub(r.cache_read_tokens);
        s.total_cache_creation = s
            .total_cache_creation
            .saturating_sub(r.cache_creation_tokens);
        // f64 has no saturating_sub; clamp to 0 like the token fields above
        // so float drift (or evicting more cost than was ever added, which
        // shouldn't happen but isn't worth a panic/underflow over) can't push
        // this negative.
        s.total_cost = (s.total_cost - r.cost_usd).max(0.0);
        s.request_count = s.request_count.saturating_sub(1);
        if s.request_count == 0 {
            entry.remove();
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(platform: Platform, model: &str, input: u64, output: u64, cost: f64) -> UsageRecord {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        rec_with_id(platform, model, input, output, cost, &format!("auto-{id}"))
    }

    fn rec_with_id(
        platform: Platform,
        model: &str,
        input: u64,
        output: u64,
        cost: f64,
        id: &str,
    ) -> UsageRecord {
        UsageRecord {
            timestamp: Utc::now(),
            platform,
            model: intern(model),
            session: intern("test"),
            session_id: intern("test-sid"),
            cwd: intern("/tmp/test"),
            id: intern(id),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: cost,
        }
    }

    fn claude_idx() -> usize {
        Tab::ClaudeCode.index()
    }
    fn codex_idx() -> usize {
        Tab::Codex.index()
    }

    #[test]
    fn platform_and_tab_indices_are_in_sync() {
        // If these ever drift, array access will panic at runtime.
        for tab in Tab::all() {
            let p = match tab {
                Tab::ClaudeCode => Platform::ClaudeCode,
                Tab::Codex => Platform::Codex,
                Tab::Cursor => Platform::Cursor,
            };
            assert_eq!(tab.index(), p.index(), "mismatch for {tab:?} / {p:?}");
        }
    }

    fn session_rec(session: &str, sid: &str, cwd: &str, tokens: u64, id: &str) -> UsageRecord {
        UsageRecord {
            timestamp: Utc::now(),
            platform: Platform::ClaudeCode,
            model: intern("opus-4"),
            session: intern(session),
            session_id: intern(sid),
            cwd: intern(cwd),
            id: intern(id),
            input_tokens: tokens,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: 0.0,
        }
    }

    #[test]
    fn session_entries_ordered_and_carry_launch_info() {
        let mut s = AppState::with_capacity(10);
        s.add_records(
            Platform::ClaudeCode,
            vec![
                session_rec("proj-a aaa", "aaa", "/work/a", 100, "r1"),
                session_rec("proj-b bbb", "bbb", "/work/b", 500, "r2"),
                session_rec("proj-a aaa", "aaa", "/work/a", 50, "r3"),
            ],
        );
        let entries = s.platform(Tab::ClaudeCode).session_entries();
        assert_eq!(entries.len(), 2);
        // Highest usage first: proj-b (500) before proj-a (150).
        assert_eq!(entries[0].label, "proj-b bbb");
        assert_eq!(entries[1].tokens, 150);
        assert_eq!(resolve(entries[1].cwd), "/work/a");
    }

    #[test]
    fn selection_focus_wraps_and_resolves_launch_target() {
        let mut s = AppState::with_capacity(10);
        s.add_records(
            Platform::ClaudeCode,
            vec![
                session_rec("proj-b bbb", "bbb", "/work/b", 500, "r1"),
                session_rec("proj-a aaa", "aaa", "/work/a", 100, "r2"),
            ],
        );
        // Focus selects the top (highest-usage) row.
        s.focus_sessions();
        assert!(s.sessions_focused);
        let (sid, cwd) = s.selected_launch().unwrap();
        assert_eq!(resolve(sid), "bbb");
        assert_eq!(resolve(cwd), "/work/b");
        // Down moves to the second row.
        s.move_selection(1);
        assert_eq!(resolve(s.selected_launch().unwrap().0), "aaa");
        // Down again wraps back to the top.
        s.move_selection(1);
        assert_eq!(resolve(s.selected_launch().unwrap().0), "bbb");
        // Up wraps to the bottom.
        s.move_selection(-1);
        assert_eq!(resolve(s.selected_launch().unwrap().0), "aaa");
    }

    #[test]
    fn focus_sessions_noop_when_no_sessions() {
        let mut s = AppState::with_capacity(10);
        s.focus_sessions();
        assert!(!s.sessions_focused);
        assert!(s.selected_launch().is_none());
    }

    #[test]
    fn add_records_dispatches_by_platform() {
        let mut s = AppState::with_capacity(10);
        s.add_records(
            Platform::ClaudeCode,
            vec![rec(Platform::ClaudeCode, "opus-4", 100, 50, 1.0)],
        );
        s.add_records(
            Platform::Codex,
            vec![rec(Platform::Codex, "gpt-5", 200, 80, 2.0)],
        );
        assert_eq!(s.platforms[claude_idx()].total_calls, 1);
        assert_eq!(s.platforms[codex_idx()].total_calls, 1);
        assert!(
            s.platforms[claude_idx()]
                .sessions
                .contains_key(&intern("opus-4"))
        );
        assert!(
            s.platforms[codex_idx()]
                .sessions
                .contains_key(&intern("gpt-5"))
        );
    }

    #[test]
    fn eviction_reverses_aggregate() {
        let mut s = AppState::with_capacity(2);
        s.add_records(
            Platform::ClaudeCode,
            vec![
                rec(Platform::ClaudeCode, "opus-4", 100, 50, 1.0),
                rec(Platform::ClaudeCode, "opus-4", 200, 80, 2.0),
                rec(Platform::ClaudeCode, "opus-4", 300, 90, 3.0),
            ],
        );
        let m = s.platforms[claude_idx()]
            .sessions
            .get(&intern("opus-4"))
            .expect("model present");
        assert_eq!(m.total_input, 500);
        assert_eq!(m.total_output, 170);
        assert_eq!(m.total_cost, 5.0);
        assert_eq!(m.request_count, 2);
        assert_eq!(s.platforms[claude_idx()].total_cost, 6.0);
        assert_eq!(s.platforms[claude_idx()].total_calls, 3);
    }

    #[test]
    fn evictions_drop_models_at_zero_count() {
        let mut s = AppState::with_capacity(1);
        s.add_records(
            Platform::ClaudeCode,
            vec![rec(Platform::ClaudeCode, "opus-4", 100, 50, 1.0)],
        );
        s.add_records(
            Platform::ClaudeCode,
            vec![rec(Platform::ClaudeCode, "sonnet-4", 200, 80, 2.0)],
        );
        let c = &s.platforms[claude_idx()];
        assert!(!c.sessions.contains_key(&intern("opus-4")));
        assert!(c.sessions.contains_key(&intern("sonnet-4")));
        assert_eq!(c.sessions.len(), 1);
    }

    #[test]
    fn detect_available_tabs_uses_configured_paths() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join("claude-custom");
        std::fs::create_dir_all(&claude).unwrap();
        let missing = dir.path().join("missing");
        let paths = AgentPaths::new(HashMap::from([
            (Tab::ClaudeCode, claude),
            (Tab::Codex, missing.join("codex")),
            (Tab::Cursor, missing.join("cursor")),
        ]));

        let mut state = AppState::new();
        state.detect_available_tabs(&paths);
        assert_eq!(state.available_tabs, vec![Tab::ClaudeCode]);
    }

    #[test]
    fn add_records_ignores_a_re_emitted_id() {
        // Simulates a reader re-delivering the same record after a file
        // truncation forced it to re-read from byte 0 (see FileScanner).
        let mut s = AppState::with_capacity(10);
        let r = rec_with_id(Platform::ClaudeCode, "opus-4", 100, 50, 1.0, "same-id");
        s.add_records(Platform::ClaudeCode, vec![r.clone()]);
        s.add_records(Platform::ClaudeCode, vec![r]);
        assert_eq!(
            s.platforms[claude_idx()].total_calls,
            1,
            "re-emitted record with the same id must not be double-counted"
        );
        assert_eq!(s.platforms[claude_idx()].total_cost, 1.0);
    }

    #[test]
    fn add_records_keeps_distinct_records_with_identical_timestamp_and_tokens() {
        // Two genuinely distinct records (different ids) that happen to share
        // every other field must both survive — dedup must not collapse them.
        let mut s = AppState::with_capacity(10);
        let a = rec_with_id(Platform::ClaudeCode, "opus-4", 100, 50, 1.0, "req-a");
        let b = rec_with_id(Platform::ClaudeCode, "opus-4", 100, 50, 1.0, "req-b");
        s.add_records(Platform::ClaudeCode, vec![a, b]);
        assert_eq!(
            s.platforms[claude_idx()].total_calls,
            2,
            "two distinct records must both be counted even with identical content"
        );
        assert_eq!(s.platforms[claude_idx()].total_cost, 2.0);
    }

    #[test]
    fn reverse_model_aggregate_clamps_cost_at_zero() {
        // Float drift (or any accounting edge case) subtracting slightly more
        // cost than was ever added must clamp at 0, not go negative.
        let mut map = HashMap::new();
        map.insert(
            intern("opus-4"),
            SessionSummary {
                model: intern("opus-4"),
                total_input: 0,
                total_output: 0,
                total_cache_read: 0,
                total_cache_creation: 0,
                total_cost: 1.0,
                request_count: 2,
            },
        );
        let r = rec(Platform::ClaudeCode, "opus-4", 0, 0, 1.5);
        reverse_model_aggregate(&mut map, &r);
        assert_eq!(map.get(&intern("opus-4")).unwrap().total_cost, 0.0);
    }

    #[test]
    fn test_compact_date_ymd_roundtrip() {
        let cd = CompactDate::new(2026, 6, 19);
        assert_eq!(cd.to_string(), "2026-06-19");
    }

    #[test]
    fn test_compact_date_serialize() {
        let cd = CompactDate::new(2026, 6, 19);
        let s = serde_json::to_string(&cd).unwrap();
        assert_eq!(s, "\"2026-06-19\"");
    }

    #[test]
    fn test_interner_roundtrip() {
        let s = "test-string-123";
        let spur = intern(s);
        let resolved = resolve(spur);
        assert_eq!(resolved, s);
    }
}
