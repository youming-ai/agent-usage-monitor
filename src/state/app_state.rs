use crate::quota::QuotaInfo;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    ClaudeCode,
    Codex,
    OpenCode,
    KimiCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    ClaudeCode,
    Codex,
    OpenCode,
    KimiCode,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Tab::ClaudeCode => Tab::Codex,
            Tab::Codex => Tab::OpenCode,
            Tab::OpenCode => Tab::KimiCode,
            Tab::KimiCode => Tab::ClaudeCode,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::ClaudeCode => Tab::KimiCode,
            Tab::Codex => Tab::ClaudeCode,
            Tab::OpenCode => Tab::Codex,
            Tab::KimiCode => Tab::OpenCode,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::ClaudeCode => "CLAUDE",
            Tab::Codex => "CODEX",
            Tab::OpenCode => "OPENCODE",
            Tab::KimiCode => "KIMI-CODE",
        }
    }

    /// Primary color for the tab (used for borders and accents)
    pub fn primary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(255, 165, 0),
            Tab::Codex => ratatui::style::Color::Rgb(59, 130, 246),
            Tab::OpenCode => ratatui::style::Color::Rgb(16, 185, 129),
            Tab::KimiCode => ratatui::style::Color::Rgb(139, 92, 246),
        }
    }

    /// Secondary color (lighter, for backgrounds)
    #[allow(dead_code)]
    pub fn secondary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(255, 200, 100),
            Tab::Codex => ratatui::style::Color::Rgb(147, 197, 253),
            Tab::OpenCode => ratatui::style::Color::Rgb(110, 231, 183),
            Tab::KimiCode => ratatui::style::Color::Rgb(196, 181, 253),
        }
    }
}

/// Single API call record
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    #[allow(dead_code)]
    pub platform: Platform,
    pub model: String,
    /// Conversation/session label (basename of the working directory).
    pub session: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
}

/// Aggregated per-model totals. Stored in a `HashMap` keyed by model name so
/// the model aggregate can be updated incrementally (O(1) per new record)
/// instead of being rebuilt on every batch insert.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub model: String,
    pub total_input: u64,
    pub total_output: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
    pub total_cost: f64,
    pub request_count: u64,
}

/// Global application state
pub struct AppState {
    // Claude Code
    pub claude_records: VecDeque<UsageRecord>,
    pub claude_sessions: HashMap<String, SessionSummary>,
    pub claude_total_calls: usize,
    pub claude_total_cost: f64,
    pub claude_quota: Option<QuotaInfo>,
    pub claude_max_records: usize,

    // Codex
    pub codex_records: VecDeque<UsageRecord>,
    pub codex_sessions: HashMap<String, SessionSummary>,
    pub codex_total_calls: usize,
    pub codex_total_cost: f64,
    pub codex_quota: Option<QuotaInfo>,
    pub codex_max_records: usize,

    // opencode
    pub opencode_records: VecDeque<UsageRecord>,
    pub opencode_sessions: HashMap<String, SessionSummary>,
    pub opencode_total_calls: usize,
    pub opencode_total_cost: f64,
    pub opencode_quota: Option<QuotaInfo>,
    pub opencode_max_records: usize,

    // Kimi Code
    pub kimi_code_records: VecDeque<UsageRecord>,
    pub kimi_code_sessions: HashMap<String, SessionSummary>,
    pub kimi_code_total_calls: usize,
    pub kimi_code_total_cost: f64,
    pub kimi_code_quota: Option<QuotaInfo>,
    pub kimi_code_max_records: usize,

    // Shared
    pub active_tab: Tab,
}

impl AppState {
    /// Build an `AppState` sized for `max_records` entries per platform.
    /// Picked up from `Config::max_records` so the user-configured cap is
    /// actually honored (previously this used a hard-coded `MAX_RECORDS = 100`).
    pub fn with_capacity(max_records: usize) -> Self {
        // A zero (or accidentally tiny) cap makes the bounded ring degenerate
        // — the eviction guard would keep exactly one record forever — so keep
        // at least one slot.
        let max_records = max_records.max(1);
        Self {
            claude_records: VecDeque::with_capacity(max_records),
            claude_sessions: HashMap::new(),
            claude_total_calls: 0,
            claude_total_cost: 0.0,
            claude_quota: None,
            codex_records: VecDeque::with_capacity(max_records),
            codex_sessions: HashMap::new(),
            codex_total_calls: 0,
            codex_total_cost: 0.0,
            codex_quota: None,
            claude_max_records: max_records,
            codex_max_records: max_records,
            opencode_records: VecDeque::with_capacity(max_records),
            opencode_sessions: HashMap::new(),
            opencode_total_calls: 0,
            opencode_total_cost: 0.0,
            opencode_quota: None,
            opencode_max_records: max_records,
            kimi_code_records: VecDeque::with_capacity(max_records),
            kimi_code_sessions: HashMap::new(),
            kimi_code_total_calls: 0,
            kimi_code_total_cost: 0.0,
            kimi_code_quota: None,
            kimi_code_max_records: max_records,
            active_tab: Tab::ClaudeCode,
        }
    }

    /// Capacity used when no config is available (matches `Config::default()`).
    pub fn new() -> Self {
        Self::with_capacity(100)
    }

    pub fn add_claude_records(&mut self, records: Vec<UsageRecord>) {
        for r in records {
            // Evict the oldest record if we're at capacity, reversing its
            // contribution to the per-model (windowed) aggregate. The lifetime
            // claude_total_cost/claude_total_calls counters are cumulative and
            // intentionally NOT decremented on eviction.
            if self.claude_records.len() >= self.claude_max_records
                && let Some(old) = self.claude_records.pop_front() {
                    reverse_model_aggregate(&mut self.claude_sessions, &old);
                }
            self.claude_total_cost += r.cost_usd;
            self.claude_total_calls += 1;
            upsert_model_aggregate(&mut self.claude_sessions, &r);
            self.claude_records.push_back(r);
        }
    }

    pub fn add_codex_records(&mut self, records: Vec<UsageRecord>) {
        for r in records {
            // Lifetime totals stay cumulative; only the windowed per-model
            // aggregate is reversed on eviction (see add_claude_records).
            if self.codex_records.len() >= self.codex_max_records
                && let Some(old) = self.codex_records.pop_front() {
                    reverse_model_aggregate(&mut self.codex_sessions, &old);
                }
            self.codex_total_cost += r.cost_usd;
            self.codex_total_calls += 1;
            upsert_model_aggregate(&mut self.codex_sessions, &r);
            self.codex_records.push_back(r);
        }
    }

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

    pub fn add_kimi_code_records(&mut self, records: Vec<UsageRecord>) {
        for r in records {
            if self.kimi_code_records.len() >= self.kimi_code_max_records
                && let Some(old) = self.kimi_code_records.pop_front() {
                    reverse_model_aggregate(&mut self.kimi_code_sessions, &old);
                }
            self.kimi_code_total_cost += r.cost_usd;
            self.kimi_code_total_calls += 1;
            upsert_model_aggregate(&mut self.kimi_code_sessions, &r);
            self.kimi_code_records.push_back(r);
        }
    }

    /// Route a batch of records to the bucket for `platform`. Every batch from
    /// a single reader is one platform, so this just dispatches.
    pub fn add_records(&mut self, platform: Platform, records: Vec<UsageRecord>) {
        match platform {
            Platform::ClaudeCode => self.add_claude_records(records),
            Platform::Codex => self.add_codex_records(records),
            Platform::OpenCode => self.add_opencode_records(records),
            Platform::KimiCode => self.add_kimi_code_records(records),
        }
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

    pub fn clear_opencode(&mut self) {
        self.opencode_records.clear();
        self.opencode_sessions.clear();
        self.opencode_total_calls = 0;
        self.opencode_total_cost = 0.0;
    }

    pub fn clear_kimi_code(&mut self) {
        self.kimi_code_records.clear();
        self.kimi_code_sessions.clear();
        self.kimi_code_total_calls = 0;
        self.kimi_code_total_cost = 0.0;
    }
}

/// Add (or sum into) a record's contribution to the per-model aggregate.
fn upsert_model_aggregate(map: &mut HashMap<String, SessionSummary>, r: &UsageRecord) {
    let entry = map.entry(r.model.clone()).or_insert_with(|| SessionSummary {
        model: r.model.clone(),
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

/// Subtract a record's contribution (called when the record is evicted from
/// the bounded ring). Removes the entry entirely once `request_count` hits
/// zero so the model table doesn't show empty rows for stale models.
fn reverse_model_aggregate(map: &mut HashMap<String, SessionSummary>, r: &UsageRecord) {
    if let Some(entry) = map.get_mut(&r.model) {
        entry.total_input = entry.total_input.saturating_sub(r.input_tokens);
        entry.total_output = entry.total_output.saturating_sub(r.output_tokens);
        entry.total_cache_read = entry.total_cache_read.saturating_sub(r.cache_read_tokens);
        entry.total_cache_creation = entry.total_cache_creation.saturating_sub(r.cache_creation_tokens);
        // Floating-point cost; could drift slightly negative under heavy
        // churn but `clear_*` is the recovery path so it's not user-visible.
        entry.total_cost -= r.cost_usd;
        entry.request_count = entry.request_count.saturating_sub(1);
        if entry.request_count == 0 {
            map.remove(&r.model);
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

    fn rec(model: &str, input: u64, output: u64, cost: f64) -> UsageRecord {
        UsageRecord {
            timestamp: Utc::now(),
            platform: Platform::ClaudeCode,
            model: model.into(),
            session: "test".into(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: cost,
        }
    }

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

    #[test]
    fn eviction_reverses_aggregate() {
        // Three records, capacity 2: the first must be evicted when the third
        // arrives, and its contribution must be subtracted from the model
        // aggregate — not silently double-counted.
        let mut s = AppState::with_capacity(2);
        s.add_claude_records(vec![
            rec("opus-4", 100, 50, 1.0),
            rec("opus-4", 200, 80, 2.0),
            rec("opus-4", 300, 90, 3.0),
        ]);
        let m = s.claude_sessions.get("opus-4").expect("model present");
        // Records 2 and 3 remain in the bounded ring (200+300, 80+90, 2+3);
        // record 1 was evicted and its contribution reversed.
        assert_eq!(m.total_input, 500);
        assert_eq!(m.total_output, 170);
        assert_eq!(m.total_cost, 5.0);
        assert_eq!(m.request_count, 2);
        // Lifetime totals are cumulative across all three records, not just the
        // two still held in the bounded ring.
        assert_eq!(s.claude_total_cost, 6.0);
        assert_eq!(s.claude_total_calls, 3);
    }

    #[test]
    fn evictions_drop_models_at_zero_count() {
        // Capacity 1, two different models: the first model's entry must be
        // removed from the map once its only record is evicted.
        let mut s = AppState::with_capacity(1);
        s.add_claude_records(vec![rec("opus-4", 100, 50, 1.0)]);
        s.add_claude_records(vec![rec("sonnet-4", 200, 80, 2.0)]);
        assert!(!s.claude_sessions.contains_key("opus-4"));
        assert!(s.claude_sessions.contains_key("sonnet-4"));
        assert_eq!(s.claude_sessions.len(), 1);
    }
}
