use crate::quota::QuotaInfo;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use lasso::{Spur, ThreadedRodeo};
use std::sync::OnceLock;

pub type InternedString = Spur;
pub type RecordId = u64;
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

/// Stable, fixed-size identity for one source record.
///
/// Record ids are intentionally not interned: unlike model/session names they
/// are almost always unique, so interning them retained the full source string
/// (occasionally an entire JSON line) for the lifetime of the process. FNV-1a
/// keeps the state-side identity compact and deterministic across processes.
pub fn record_id(value: &str) -> RecordId {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    value.as_bytes().iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
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
    paths: HashMap<Platform, PathBuf>,
}

impl AgentPaths {
    pub fn new(paths: HashMap<Platform, PathBuf>) -> Self {
        Self { paths }
    }

    pub fn path_for(&self, platform: Platform) -> PathBuf {
        self.paths
            .get(&platform)
            .cloned()
            .unwrap_or_else(|| {
                tracing::warn!("AgentPaths missing {platform:?} — falling back to default path; use platforms::resolve_paths");
                platform.default_path()
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    ClaudeCode,
    Codex,
    Pi,
    Cursor,
}

impl Platform {
    /// Number of platform variants — used to size the fixed array in `AppState`.
    pub const COUNT: usize = 4;

    /// Zero-based index for array access.
    pub const fn index(self) -> usize {
        match self {
            Self::ClaudeCode => 0,
            Self::Codex => 1,
            Self::Pi => 2,
            Self::Cursor => 3,
        }
    }

    pub fn next_in(self, available: &[Self]) -> Self {
        if available.is_empty() {
            return self;
        }
        let pos = available.iter().position(|&p| p == self).unwrap_or(0);
        available[(pos + 1) % available.len()]
    }

    pub fn prev_in(self, available: &[Self]) -> Self {
        if available.is_empty() {
            return self;
        }
        let pos = available.iter().position(|&p| p == self).unwrap_or(0);
        available[(pos + available.len() - 1) % available.len()]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "CLAUDE",
            Self::Codex => "CODEX",
            Self::Pi => "PI",
            Self::Cursor => "CURSOR",
        }
    }

    /// Primary color for the tab (used for borders and accents).
    /// Sourced from each agent's official CLI theme / brand palette.
    pub fn primary_color(self) -> ratatui::style::Color {
        match self {
            Self::ClaudeCode => ratatui::style::Color::Rgb(217, 119, 87),
            Self::Codex => ratatui::style::Color::Rgb(59, 130, 246),
            Self::Pi => ratatui::style::Color::Rgb(138, 190, 183),
            Self::Cursor => ratatui::style::Color::Rgb(136, 192, 208),
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::ClaudeCode, Self::Codex, Self::Pi, Self::Cursor]
    }

    pub fn default_path(self) -> std::path::PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        match self {
            Self::ClaudeCode => home.join(".claude/projects"),
            Self::Codex => home.join(".codex"),
            Self::Pi => home.join(".pi/agent/sessions"),
            Self::Cursor => home.join(".cursor"),
        }
    }

    /// Shared detection logic: does the agent's data exist at this path?
    fn is_available_at_path(path: &std::path::Path, platform: Self) -> bool {
        match platform {
            Self::Cursor => path.join("projects").exists() || path.join("chats").exists(),
            _ => path.exists(),
        }
    }

    pub fn is_available_at(self, paths: &AgentPaths) -> bool {
        let path = paths.path_for(self);
        Self::is_available_at_path(&path, self)
    }
}

/// UI calls platforms "tabs". Keep this alias for callers without maintaining
/// a second enum and its fragile conversion/index mapping.
pub type Tab = Platform;

/// Single API call record
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
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
    /// Human-readable conversation title when the source records one (Claude's
    /// `ai-title`), shown in the sessions list in place of the `dir + id`
    /// label. Empty when the agent doesn't log a title (Codex, Cursor).
    pub title: InternedString,
    /// Stable identity used to dedup re-emitted records (e.g. after a file
    /// truncation/rewrite forces a reader to re-read from byte 0). Readers
    /// build this from the strongest identifier their source data carries —
    /// a message/event id, a database primary key, or (when nothing better
    /// exists) a hash of the record's own content. See `AppState::add_records`.
    pub id: RecordId,
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
    /// Stable, unique per-row selection key: the label records were grouped
    /// by. Unlike `session_id` (which is empty for sources that log no id, so
    /// several rows would collide on it) this is distinct for every row by
    /// construction, so the selection can always address exactly one row.
    pub key: InternedString,
    pub label: &'static str,
    pub session_id: InternedString,
    /// Absolute working dir, or empty when the source has no real path.
    pub cwd: InternedString,
    pub tokens: u64,
    pub requests: u64,
}

/// The selected session and platform needed to build a resume command. This
/// keeps selection lookup inside `AppState`; callers only hand the result to
/// the launcher rather than reassembling tab, id, and working directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeSelection {
    pub(crate) platform: Platform,
    pub(crate) session_id: InternedString,
    pub(crate) cwd: InternedString,
}

/// Per-platform state held in a `[PlatformState; Platform::COUNT]` array.
#[derive(Debug)]
pub struct PlatformState {
    pub records: VecDeque<UsageRecord>,
    pub sessions: HashMap<InternedString, SessionSummary>,
    /// Totals for the same bounded record window represented by `records`,
    /// `sessions`, and the TUI tables.
    pub window_calls: usize,
    pub window_cost: f64,
    pub quota: Option<QuotaInfo>,
    pub account_email: Option<String>,
    pub max_records: usize,
    /// Last reader failure for this platform. Cleared by the next successful
    /// scan, including a successful scan that produces no new records.
    pub reader_error: Option<String>,
    /// Ids of every record ever ingested for this platform, so a reader
    /// re-emitting the same record (e.g. after a truncation-triggered re-read
    /// from byte 0) doesn't disturb the window.
    // ponytail: deliberately NOT bounded to the `records` window. Bounding it
    // lets a re-read of one file re-add records already evicted from the
    // window, which displaces *other* files' recent records — and those never
    // come back, because their own read cursors are already past them. A
    // RecordId is 8 bytes, so millions of records stay far below the ceiling
    // where this would matter; shrink it by hashing into a Bloom filter only
    // if that ever stops being true.
    seen_ids: HashSet<RecordId>,
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
            self.window_calls,
            self.window_cost,
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
            title: InternedString,
        }
        let mut by: HashMap<InternedString, Acc> = HashMap::new();
        for r in &self.records {
            let e = by.entry(r.session).or_insert(Acc {
                tokens: 0,
                requests: 0,
                session_id: r.session_id,
                cwd: r.cwd,
                title: r.title,
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
            // The title can be set/renamed mid-session; the latest wins.
            if !resolve(r.title).is_empty() {
                e.title = r.title;
            }
        }
        let mut entries: Vec<SessionEntry> = by
            .into_iter()
            .map(|(session_key, a)| {
                // Show the conversation title when one exists, else fall back
                // to the `dir + short-id` label.
                let title = resolve(a.title);
                let label = if title.is_empty() {
                    resolve(session_key)
                } else {
                    title
                };
                SessionEntry {
                    key: session_key,
                    label,
                    session_id: a.session_id,
                    cwd: a.cwd,
                    tokens: a.tokens,
                    requests: a.requests,
                }
            })
            .collect();
        entries.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.label.cmp(b.label)));
        entries
    }
}

/// Global application state
pub struct AppState {
    pub platforms: [PlatformState; Platform::COUNT],
    pub active_tab: Platform,
    pub available_tabs: Vec<Platform>,
    /// Whether keyboard focus is on the sessions list (arrow keys move the
    /// selection and Enter launches, instead of switching tabs).
    pub sessions_focused: bool,
    /// The selected session, tracked by its stable [`SessionEntry::key`]
    /// rather than row index (so live re-sorting never moves the cursor onto
    /// a different session between selecting and pressing Enter) and rather
    /// than `session_id` (which collides across id-less rows).
    pub selected_key: Option<InternedString>,
}

impl AppState {
    pub fn with_capacity(max_records: usize) -> Self {
        let max_records = max_records.max(1);
        let make = || PlatformState {
            records: VecDeque::with_capacity(max_records),
            sessions: HashMap::new(),
            window_calls: 0,
            window_cost: 0.0,
            quota: None,
            account_email: None,
            max_records,
            reader_error: None,
            seen_ids: HashSet::new(),
        };
        Self {
            // ponytail: size-agnostic init, tracks Platform::COUNT automatically
            platforms: std::array::from_fn(|_| make()),
            active_tab: Platform::ClaudeCode,
            available_tabs: Vec::new(),
            sessions_focused: false,
            selected_key: None,
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
            self.selected_key = None;
            return;
        }
        self.sessions_focused = true;
        let valid = self
            .selected_key
            .is_some_and(|key| entries.iter().any(|e| e.key == key));
        if !valid {
            self.selected_key = Some(entries[0].key);
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
            self.selected_key = None;
            return;
        }
        let cur = self
            .selected_key
            .and_then(|key| entries.iter().position(|e| e.key == key));
        let next = match cur {
            Some(i) => (i as i32 + delta).rem_euclid(entries.len() as i32) as usize,
            None => 0,
        };
        self.selected_key = Some(entries[next].key);
    }

    /// Down/j: enter the sessions list (highlighting the first row) or move
    /// down within it once focused.
    pub fn nav_down(&mut self) {
        if self.sessions_focused {
            self.move_selection(1);
        } else {
            self.focus_sessions();
        }
    }

    /// Up/k: enter the sessions list or move up within it once focused.
    pub fn nav_up(&mut self) {
        if self.sessions_focused {
            self.move_selection(-1);
        } else {
            self.focus_sessions();
        }
    }

    /// Enter: focus the sessions list, or (if already focused) return the
    /// selected session to resume. `None` means "nothing to launch yet".
    pub fn activate_sessions(&mut self) -> Option<ResumeSelection> {
        if self.sessions_focused {
            self.selected_resume()
        } else {
            self.focus_sessions();
            None
        }
    }

    /// The selected session as a named resume request. `None` means the
    /// selection disappeared during a live refresh or lacks a usable id — a
    /// row keyed by its label but without a `session_id` (e.g. an id-less
    /// source) is selectable for display but cannot be resumed.
    pub fn selected_resume(&self) -> Option<ResumeSelection> {
        let selected_key = self.selected_key?;
        let entry = self
            .active_session_entries()
            .into_iter()
            .find(|entry| entry.key == selected_key)?;
        (!resolve(entry.session_id).is_empty()).then_some(ResumeSelection {
            platform: self.active_tab,
            session_id: entry.session_id,
            cwd: entry.cwd,
        })
    }

    /// Reset session focus/selection — call when the visible list changes
    /// wholesale (tab switch, tab clear) so a stale selection can't launch.
    pub fn reset_session_focus(&mut self) {
        self.sessions_focused = false;
        self.selected_key = None;
    }

    /// Small window, for tests and `Default` only — the running app always
    /// passes the configured `max_records` to [`Self::with_capacity`].
    pub fn new() -> Self {
        Self::with_capacity(100)
    }

    pub fn platform(&self, platform: Platform) -> &PlatformState {
        &self.platforms[platform.index()]
    }

    pub fn platform_mut(&mut self, platform: Platform) -> &mut PlatformState {
        &mut self.platforms[platform.index()]
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
                p.window_calls = p.window_calls.saturating_sub(1);
                p.window_cost = (p.window_cost - old.cost_usd).max(0.0);
                reverse_model_aggregate(&mut p.sessions, &old);
            }
            p.window_cost += r.cost_usd;
            p.window_calls += 1;
            upsert_model_aggregate(&mut p.sessions, &r);
            p.records.push_back(r);
        }
    }

    pub fn mark_reader_success(&mut self, platform: Platform) {
        self.platform_mut(platform).reader_error = None;
    }

    pub fn mark_reader_error(&mut self, platform: Platform, error: impl Into<String>) {
        self.platform_mut(platform).reader_error = Some(error.into());
    }

    pub fn clear_tab(&mut self, tab: Tab) {
        let p = self.platform_mut(tab);
        p.records.clear();
        p.sessions.clear();
        p.window_calls = 0;
        p.window_cost = 0.0;
        p.seen_ids.clear();
        p.reader_error = None;
    }

    pub fn detect_available_tabs(&mut self, paths: &AgentPaths) {
        self.available_tabs = Platform::all()
            .iter()
            .filter(|platform| platform.is_available_at(paths))
            .copied()
            .collect();

        if !self.available_tabs.contains(&self.active_tab) {
            self.active_tab = self
                .available_tabs
                .first()
                .copied()
                .unwrap_or(Platform::ClaudeCode);
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

/// Shared test-record builder — the single place that lists every
/// `UsageRecord` field, so adding a field updates only this function instead
/// of every test module. Callers override the few fields they care about via
/// struct-update syntax: `UsageRecord { id: record_id("x"), ..test_record(m) }`.
#[cfg(test)]
pub(crate) fn test_record(model: &str) -> UsageRecord {
    UsageRecord {
        timestamp: Utc::now(),
        model: intern(model),
        session: intern("test"),
        session_id: intern("test-sid"),
        cwd: intern("/tmp/test"),
        title: intern(""),
        id: record_id("test-id"),
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cost_usd: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(model: &str, input: u64, output: u64, cost: f64) -> UsageRecord {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        rec_with_id(model, input, output, cost, &format!("auto-{id}"))
    }

    fn rec_with_id(model: &str, input: u64, output: u64, cost: f64, id: &str) -> UsageRecord {
        UsageRecord {
            id: record_id(id),
            input_tokens: input,
            output_tokens: output,
            cost_usd: cost,
            ..test_record(model)
        }
    }

    fn claude_idx() -> usize {
        Platform::ClaudeCode.index()
    }
    fn codex_idx() -> usize {
        Platform::Codex.index()
    }

    #[test]
    fn platform_indices_are_unique() {
        let indices: HashSet<_> = Platform::all().iter().map(|p| p.index()).collect();
        assert_eq!(indices.len(), Platform::COUNT);
    }

    fn session_rec(session: &str, sid: &str, cwd: &str, tokens: u64, id: &str) -> UsageRecord {
        session_rec_titled(session, sid, cwd, "", tokens, id)
    }

    #[allow(clippy::too_many_arguments)]
    fn session_rec_titled(
        session: &str,
        sid: &str,
        cwd: &str,
        title: &str,
        tokens: u64,
        id: &str,
    ) -> UsageRecord {
        UsageRecord {
            session: intern(session),
            session_id: intern(sid),
            cwd: intern(cwd),
            title: intern(title),
            id: record_id(id),
            input_tokens: tokens,
            ..test_record("opus-4")
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
        let entries = s.platform(Platform::ClaudeCode).session_entries();
        assert_eq!(entries.len(), 2);
        // Highest usage first: proj-b (500) before proj-a (150).
        assert_eq!(entries[0].label, "proj-b bbb");
        assert_eq!(entries[1].tokens, 150);
        assert_eq!(resolve(entries[1].cwd), "/work/a");
    }

    #[test]
    fn session_entry_label_prefers_title_when_present() {
        let mut s = AppState::with_capacity(10);
        s.add_records(
            Platform::ClaudeCode,
            vec![
                // First record has no title yet; a later one names the session.
                session_rec("proj aaa", "aaa", "/work/a", 10, "r1"),
                session_rec_titled("proj aaa", "aaa", "/work/a", "Fix login bug", 10, "r2"),
                // A different session with no title keeps the dir+id label.
                session_rec("proj bbb", "bbb", "/work/b", 5, "r3"),
            ],
        );
        let entries = s.platform(Platform::ClaudeCode).session_entries();
        let titled = entries
            .iter()
            .find(|e| resolve(e.session_id) == "aaa")
            .unwrap();
        assert_eq!(titled.label, "Fix login bug");
        let untitled = entries
            .iter()
            .find(|e| resolve(e.session_id) == "bbb")
            .unwrap();
        assert_eq!(untitled.label, "proj bbb");
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
        let resume = s.selected_resume().unwrap();
        assert_eq!(resume.platform, Platform::ClaudeCode);
        assert_eq!(resolve(resume.session_id), "bbb");
        assert_eq!(resolve(resume.cwd), "/work/b");
        // Down moves to the second row.
        s.move_selection(1);
        assert_eq!(resolve(s.selected_resume().unwrap().session_id), "aaa");
        // Down again wraps back to the top.
        s.move_selection(1);
        assert_eq!(resolve(s.selected_resume().unwrap().session_id), "bbb");
        // Up wraps to the bottom.
        s.move_selection(-1);
        assert_eq!(resolve(s.selected_resume().unwrap().session_id), "aaa");
    }

    #[test]
    fn focus_sessions_noop_when_no_sessions() {
        let mut s = AppState::with_capacity(10);
        s.focus_sessions();
        assert!(!s.sessions_focused);
        assert!(s.selected_resume().is_none());
    }

    #[test]
    fn id_less_rows_are_individually_selectable_and_not_launchable() {
        // Two sessions from a source that records no session id (empty sid).
        // Tracked by row key (their labels), they must be distinct rows the
        // cursor can address separately — the bug the key fix addresses.
        let mut s = AppState::with_capacity(10);
        s.add_records(
            Platform::ClaudeCode,
            vec![
                session_rec("proj-a", "", "", 100, "r1"),
                session_rec("proj-b", "", "", 50, "r2"),
            ],
        );
        let entries = s.active_session_entries();
        assert_eq!(entries.len(), 2, "id-less rows must not collapse into one");

        s.focus_sessions();
        let first = s.selected_key;
        s.move_selection(1);
        assert_ne!(
            s.selected_key, first,
            "moving must land on a different id-less row"
        );
        // Neither row can launch: no session id to resume.
        assert!(s.selected_resume().is_none());
        s.move_selection(1);
        assert!(s.selected_resume().is_none());
    }

    #[test]
    fn add_records_dispatches_by_platform() {
        let mut s = AppState::with_capacity(10);
        s.add_records(Platform::ClaudeCode, vec![rec("opus-4", 100, 50, 1.0)]);
        s.add_records(Platform::Codex, vec![rec("gpt-5", 200, 80, 2.0)]);
        assert_eq!(s.platforms[claude_idx()].window_calls, 1);
        assert_eq!(s.platforms[codex_idx()].window_calls, 1);
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
                rec("opus-4", 100, 50, 1.0),
                rec("opus-4", 200, 80, 2.0),
                rec("opus-4", 300, 90, 3.0),
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
        assert_eq!(s.platforms[claude_idx()].window_cost, 5.0);
        assert_eq!(s.platforms[claude_idx()].window_calls, 2);
    }

    /// A reader that re-reads one file from byte 0 (truncation, or a store.db
    /// replaced at the same path) replays that file's whole history. Those
    /// records must be recognised as already-seen: re-adding them would evict
    /// *other* files' recent records, which never return because their own
    /// read cursors have long since passed them.
    #[test]
    fn replaying_one_files_history_does_not_displace_another_files_records() {
        let mut s = AppState::with_capacity(3);
        let file_a = || {
            vec![
                rec_with_id("opus-4", 10, 5, 1.0, "a1"),
                rec_with_id("opus-4", 10, 5, 1.0, "a2"),
                rec_with_id("opus-4", 10, 5, 1.0, "a3"),
            ]
        };
        s.add_records(Platform::ClaudeCode, file_a());
        s.add_records(
            Platform::ClaudeCode,
            vec![rec_with_id("sonnet-4", 10, 5, 9.0, "b1")],
        );

        s.add_records(Platform::ClaudeCode, file_a());

        let p = &s.platforms[claude_idx()];
        assert!(
            p.records.iter().any(|r| r.id == record_id("b1")),
            "file B's record was displaced by file A's replay"
        );
        assert_eq!(p.window_calls, 3);
        assert_eq!(p.window_cost, 11.0);
    }

    #[test]
    fn evictions_drop_models_at_zero_count() {
        let mut s = AppState::with_capacity(1);
        s.add_records(Platform::ClaudeCode, vec![rec("opus-4", 100, 50, 1.0)]);
        s.add_records(Platform::ClaudeCode, vec![rec("sonnet-4", 200, 80, 2.0)]);
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
            (Platform::ClaudeCode, claude),
            (Platform::Codex, missing.join("codex")),
            (Platform::Pi, missing.join("pi")),
            (Platform::Cursor, missing.join("cursor")),
        ]));

        let mut state = AppState::new();
        state.detect_available_tabs(&paths);
        assert_eq!(state.available_tabs, vec![Platform::ClaudeCode]);
    }

    #[test]
    fn add_records_ignores_a_re_emitted_id() {
        // Simulates a reader re-delivering the same record after a file
        // truncation forced it to re-read from byte 0 (see FileScanner).
        let mut s = AppState::with_capacity(10);
        let r = rec_with_id("opus-4", 100, 50, 1.0, "same-id");
        s.add_records(Platform::ClaudeCode, vec![r.clone()]);
        s.add_records(Platform::ClaudeCode, vec![r]);
        assert_eq!(
            s.platforms[claude_idx()].window_calls,
            1,
            "re-emitted record with the same id must not be double-counted"
        );
        assert_eq!(s.platforms[claude_idx()].window_cost, 1.0);
    }

    #[test]
    fn add_records_keeps_distinct_records_with_identical_timestamp_and_tokens() {
        // Two genuinely distinct records (different ids) that happen to share
        // every other field must both survive — dedup must not collapse them.
        let mut s = AppState::with_capacity(10);
        let a = rec_with_id("opus-4", 100, 50, 1.0, "req-a");
        let b = rec_with_id("opus-4", 100, 50, 1.0, "req-b");
        s.add_records(Platform::ClaudeCode, vec![a, b]);
        assert_eq!(
            s.platforms[claude_idx()].window_calls,
            2,
            "two distinct records must both be counted even with identical content"
        );
        assert_eq!(s.platforms[claude_idx()].window_cost, 2.0);
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
        let r = rec("opus-4", 0, 0, 1.5);
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

    #[test]
    fn record_ids_are_stable_and_distinguish_source_values() {
        assert_eq!(record_id("message-1"), record_id("message-1"));
        assert_ne!(record_id("message-1"), record_id("message-2"));
    }
}
