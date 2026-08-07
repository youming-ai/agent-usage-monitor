use crate::quota::QuotaInfo;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
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

    pub fn year(self) -> u16 {
        ((self.0 >> 16) & 0xFFFF) as u16
    }

    pub fn month(self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }

    pub fn day(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    /// Calendar date `days` before this one, or `None` if the date is invalid
    /// / would underflow the proleptic Gregorian range we care about.
    pub fn checked_sub_days(self, days: i64) -> Option<Self> {
        use chrono::NaiveDate;
        let date =
            NaiveDate::from_ymd_opt(self.year() as i32, self.month() as u32, self.day() as u32)?;
        let earlier = date.checked_sub_signed(chrono::Duration::days(days))?;
        use chrono::Datelike;
        Some(Self::new(
            earlier.year() as u16,
            earlier.month() as u8,
            earlier.day() as u8,
        ))
    }

    /// Weekday where Sunday = 0 … Saturday = 6 (GitHub contribution graph).
    pub fn weekday_sun0(self) -> u8 {
        use chrono::{Datelike, NaiveDate};
        let date =
            NaiveDate::from_ymd_opt(self.year() as i32, self.month() as u32, self.day() as u32)
                .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
        date.weekday().num_days_from_sunday() as u8
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

/// Resolved agent data paths (from config + CLI). Used for availability
/// detection so custom paths are honored instead of always checking defaults.
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
}

impl Platform {
    /// Number of platform variants — used to size the fixed array in `AppState`.
    pub const COUNT: usize = 2;

    /// Zero-based index for array access.
    pub const fn index(self) -> usize {
        match self {
            Self::ClaudeCode => 0,
            Self::Codex => 1,
        }
    }

    pub fn label(self) -> &'static str {
        crate::platforms::entry_for_platform(self).label
    }

    /// Primary color for the platform section header.
    /// Sourced from each agent's official CLI theme / brand palette.
    pub fn primary_color(self) -> ratatui::style::Color {
        crate::platforms::entry_for_platform(self).primary_color
    }

    pub fn all() -> &'static [Self] {
        &[Self::ClaudeCode, Self::Codex]
    }

    pub fn default_path(self) -> std::path::PathBuf {
        crate::platforms::entry_for_platform(self).default_path()
    }

    pub fn is_available_at(self, paths: &AgentPaths) -> bool {
        let path = paths.path_for(self);
        crate::platforms::entry_for_platform(self).is_available_at(&path)
    }
}

/// Single API call record
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    pub model: InternedString,
    /// Human-readable session label (working-dir basename + short id) used
    /// for per-session aggregation in `aum stats --json`.
    pub session: InternedString,
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
    /// Reasoning / thinking tokens when the source reports them separately
    /// (Codex). Folded into the OUT column for display and priced as output.
    pub reasoning_tokens: u64,
    /// Optional session title (e.g. Claude `aiTitle`). Empty when unknown.
    pub session_title: InternedString,
    /// Working-dir basename when known; empty when the reader has nothing better.
    pub project: InternedString,
    pub cost_usd: f64,
}

/// Aggregated per-model totals.
#[derive(Debug, Clone)]
pub struct ModelTotals {
    pub model: InternedString,
    pub total_input: u64,
    pub total_output: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
    pub total_reasoning: u64,
    pub total_cost: f64,
    pub request_count: u64,
}

/// Per-day totals for the contribution heatmap. Kept in step with the
/// sliding record window (eviction reverses the same way model totals do)
/// and pruned past ~370 days so memory stays bounded.
#[derive(Debug, Clone, Default)]
pub struct DayTotals {
    pub cost_usd: f64,
    pub tokens: u64,
    pub calls: u64,
}

/// Per-session aggregate for the top-N session table.
#[derive(Debug, Clone)]
pub struct SessionTotals {
    pub session: InternedString,
    pub title: InternedString,
    pub project: InternedString,
    pub first_ts: DateTime<Utc>,
    pub last_ts: DateTime<Utc>,
    pub cost_usd: f64,
    pub tokens: u64,
    pub calls: u64,
}

/// Tool / file-operation counters (MCP `get_file_operations`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolOps {
    pub files_read: u64,
    pub files_edited: u64,
    pub files_added: u64,
    pub files_deleted: u64,
    pub terminal_commands: u64,
    pub lines_read: u64,
    pub lines_edited: u64,
}

impl ToolOps {
    pub fn accumulate(&mut self, other: &ToolOps) {
        self.files_read += other.files_read;
        self.files_edited += other.files_edited;
        self.files_added += other.files_added;
        self.files_deleted += other.files_deleted;
        self.terminal_commands += other.terminal_commands;
        self.lines_read += other.lines_read;
        self.lines_edited += other.lines_edited;
    }
}

/// Per-platform state held in a `[PlatformState; Platform::COUNT]` array.
#[derive(Debug)]
pub struct PlatformState {
    pub records: VecDeque<UsageRecord>,
    /// Per-model totals for the same bounded record window represented by
    /// `records`, the TUI model table, and `window_calls`/`window_cost`.
    pub models: HashMap<InternedString, ModelTotals>,
    /// Per-session totals for the same window (top-N session table).
    pub sessions: HashMap<InternedString, SessionTotals>,
    /// Per-day totals for the contribution heatmap.
    pub daily: BTreeMap<CompactDate, DayTotals>,
    /// Cumulative tool ops observed by the platform reader (not window-bounded).
    pub tool_ops: ToolOps,
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
        &HashMap<InternedString, ModelTotals>,
        usize,
        f64,
    ) {
        (
            self.quota.as_ref(),
            &self.models,
            self.window_calls,
            self.window_cost,
        )
    }

    /// Top sessions by cost (then tokens), highest first.
    pub fn top_sessions(&self, n: usize) -> Vec<&SessionTotals> {
        let mut entries: Vec<&SessionTotals> = self.sessions.values().collect();
        entries.sort_by(|a, b| {
            b.cost_usd
                .partial_cmp(&a.cost_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.tokens.cmp(&a.tokens))
                .then_with(|| resolve(a.session).cmp(resolve(b.session)))
        });
        entries.truncate(n);
        entries
    }
}

/// Global application state
pub struct AppState {
    pub platforms: [PlatformState; Platform::COUNT],
    /// Platforms with live readers, maintained by the background discovery
    /// loop so rendering never performs filesystem I/O.
    pub available_platforms: Vec<Platform>,
}

impl AppState {
    pub fn with_capacity(max_records: usize) -> Self {
        let max_records = max_records.max(1);
        let make = || PlatformState {
            records: VecDeque::with_capacity(max_records),
            models: HashMap::new(),
            sessions: HashMap::new(),
            daily: BTreeMap::new(),
            tool_ops: ToolOps::default(),
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
            available_platforms: Vec::new(),
        }
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
                reverse_model_aggregate(&mut p.models, &old);
                reverse_session_aggregate(&mut p.sessions, &old);
                reverse_day_aggregate(&mut p.daily, &old);
            }
            p.window_cost += r.cost_usd;
            p.window_calls += 1;
            upsert_model_aggregate(&mut p.models, &r);
            upsert_session_aggregate(&mut p.sessions, &r);
            upsert_day_aggregate(&mut p.daily, &r);
            p.records.push_back(r);
        }
        prune_daily(&mut p.daily);
    }

    pub fn add_tool_ops(&mut self, platform: Platform, ops: ToolOps) {
        self.platforms[platform.index()].tool_ops.accumulate(&ops);
    }

    pub fn mark_reader_success(&mut self, platform: Platform) {
        self.platform_mut(platform).reader_error = None;
    }

    pub fn mark_reader_error(&mut self, platform: Platform, error: impl Into<String>) {
        self.platform_mut(platform).reader_error = Some(error.into());
    }

    pub fn set_available_platforms(&mut self, platforms: impl IntoIterator<Item = Platform>) {
        let available: HashSet<_> = platforms.into_iter().collect();
        self.available_platforms = Platform::all()
            .iter()
            .copied()
            .filter(|platform| available.contains(platform))
            .collect();
    }
}

fn record_tokens(r: &UsageRecord) -> u64 {
    r.input_tokens
        + r.output_tokens
        + r.cache_read_tokens
        + r.cache_creation_tokens
        + r.reasoning_tokens
}

fn upsert_model_aggregate(map: &mut HashMap<InternedString, ModelTotals>, r: &UsageRecord) {
    let entry = map
        .entry(r.model) // r.model is Spur (Copy), no clone needed!
        .or_insert_with(|| ModelTotals {
            model: r.model,
            total_input: 0,
            total_output: 0,
            total_cache_read: 0,
            total_cache_creation: 0,
            total_reasoning: 0,
            total_cost: 0.0,
            request_count: 0,
        });
    entry.total_input += r.input_tokens;
    entry.total_output += r.output_tokens;
    entry.total_cache_read += r.cache_read_tokens;
    entry.total_cache_creation += r.cache_creation_tokens;
    entry.total_reasoning += r.reasoning_tokens;
    entry.total_cost += r.cost_usd;
    entry.request_count += 1;
}

fn reverse_model_aggregate(map: &mut HashMap<InternedString, ModelTotals>, r: &UsageRecord) {
    if let std::collections::hash_map::Entry::Occupied(mut entry) = map.entry(r.model) {
        let s = entry.get_mut();
        s.total_input = s.total_input.saturating_sub(r.input_tokens);
        s.total_output = s.total_output.saturating_sub(r.output_tokens);
        s.total_cache_read = s.total_cache_read.saturating_sub(r.cache_read_tokens);
        s.total_cache_creation = s
            .total_cache_creation
            .saturating_sub(r.cache_creation_tokens);
        s.total_reasoning = s.total_reasoning.saturating_sub(r.reasoning_tokens);
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

fn upsert_session_aggregate(map: &mut HashMap<InternedString, SessionTotals>, r: &UsageRecord) {
    let tokens = record_tokens(r);
    let entry = map.entry(r.session).or_insert_with(|| SessionTotals {
        session: r.session,
        title: r.session_title,
        project: r.project,
        first_ts: r.timestamp,
        last_ts: r.timestamp,
        cost_usd: 0.0,
        tokens: 0,
        calls: 0,
    });
    if resolve(entry.title).is_empty() && !resolve(r.session_title).is_empty() {
        entry.title = r.session_title;
    }
    if resolve(entry.project).is_empty() && !resolve(r.project).is_empty() {
        entry.project = r.project;
    }
    if r.timestamp < entry.first_ts {
        entry.first_ts = r.timestamp;
    }
    if r.timestamp > entry.last_ts {
        entry.last_ts = r.timestamp;
    }
    entry.cost_usd += r.cost_usd;
    entry.tokens += tokens;
    entry.calls += 1;
}

fn reverse_session_aggregate(map: &mut HashMap<InternedString, SessionTotals>, r: &UsageRecord) {
    if let std::collections::hash_map::Entry::Occupied(mut entry) = map.entry(r.session) {
        let s = entry.get_mut();
        s.cost_usd = (s.cost_usd - r.cost_usd).max(0.0);
        s.tokens = s.tokens.saturating_sub(record_tokens(r));
        s.calls = s.calls.saturating_sub(1);
        if s.calls == 0 {
            entry.remove();
        }
    }
}

fn upsert_day_aggregate(map: &mut BTreeMap<CompactDate, DayTotals>, r: &UsageRecord) {
    let day = CompactDate::from_datetime(r.timestamp);
    let entry = map.entry(day).or_default();
    entry.cost_usd += r.cost_usd;
    entry.tokens += record_tokens(r);
    entry.calls += 1;
}

fn reverse_day_aggregate(map: &mut BTreeMap<CompactDate, DayTotals>, r: &UsageRecord) {
    let day = CompactDate::from_datetime(r.timestamp);
    if let std::collections::btree_map::Entry::Occupied(mut entry) = map.entry(day) {
        let d = entry.get_mut();
        d.cost_usd = (d.cost_usd - r.cost_usd).max(0.0);
        d.tokens = d.tokens.saturating_sub(record_tokens(r));
        d.calls = d.calls.saturating_sub(1);
        if d.calls == 0 {
            entry.remove();
        }
    }
}

/// Drop day buckets older than ~370 days so the heatmap can't grow forever.
fn prune_daily(map: &mut BTreeMap<CompactDate, DayTotals>) {
    let today = CompactDate::from_datetime(Utc::now());
    let cutoff = today
        .checked_sub_days(370)
        .unwrap_or_else(|| CompactDate::new(1970, 1, 1));
    while let Some((&first, _)) = map.iter().next() {
        if first < cutoff {
            map.remove(&first);
        } else {
            break;
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
        id: record_id("test-id"),
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        reasoning_tokens: 0,
        session_title: intern(""),
        project: intern(""),
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

    #[test]
    fn add_records_dispatches_by_platform() {
        let mut s = AppState::with_capacity(10);
        s.add_records(Platform::ClaudeCode, vec![rec("opus-4", 100, 50, 1.0)]);
        s.add_records(Platform::Codex, vec![rec("gpt-5", 200, 80, 2.0)]);
        assert_eq!(s.platforms[claude_idx()].window_calls, 1);
        assert_eq!(s.platforms[codex_idx()].window_calls, 1);
        assert!(
            s.platforms[claude_idx()]
                .models
                .contains_key(&intern("opus-4"))
        );
        assert!(
            s.platforms[codex_idx()]
                .models
                .contains_key(&intern("gpt-5"))
        );
    }

    #[test]
    fn eviction_reverses_daily_and_session_aggregates() {
        let mut s = AppState::with_capacity(1);
        let a = UsageRecord {
            session: intern("sess-a"),
            session_title: intern("Title A"),
            project: intern("proj"),
            cost_usd: 1.0,
            input_tokens: 100,
            ..rec("opus-4", 100, 0, 1.0)
        };
        let b = UsageRecord {
            session: intern("sess-b"),
            session_title: intern("Title B"),
            project: intern("proj"),
            cost_usd: 2.0,
            input_tokens: 200,
            ..rec("opus-4", 200, 0, 2.0)
        };
        s.add_records(Platform::ClaudeCode, vec![a, b]);
        let p = &s.platforms[claude_idx()];
        assert_eq!(p.sessions.len(), 1, "first session evicted with the record");
        assert!(p.sessions.contains_key(&intern("sess-b")));
        assert_eq!(p.window_calls, 1);
        // Day bucket for remaining record only.
        let day_calls: u64 = p.daily.values().map(|d| d.calls).sum();
        assert_eq!(day_calls, 1);
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
            .models
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
        assert!(!c.models.contains_key(&intern("opus-4")));
        assert!(c.models.contains_key(&intern("sonnet-4")));
        assert_eq!(c.models.len(), 1);
    }

    #[test]
    fn availability_uses_configured_paths() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join("claude-custom");
        std::fs::create_dir_all(&claude).unwrap();
        let missing = dir.path().join("missing");
        let paths = AgentPaths::new(HashMap::from([
            (Platform::ClaudeCode, claude),
            (Platform::Codex, missing.join("codex")),
        ]));

        assert!(Platform::ClaudeCode.is_available_at(&paths));
        assert!(!Platform::Codex.is_available_at(&paths));
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
            ModelTotals {
                model: intern("opus-4"),
                total_input: 0,
                total_output: 0,
                total_cache_read: 0,
                total_cache_creation: 0,
                total_reasoning: 0,
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
