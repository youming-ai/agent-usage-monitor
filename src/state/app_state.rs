use crate::quota::QuotaInfo;
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
        self.next()
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
    #[allow(dead_code)]
    pub platform: Platform,
    pub model: String,
    #[allow(dead_code)]
    pub project: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
    #[allow(dead_code)]
    pub service_tier: String,
    #[allow(dead_code)]
    pub message_id: String,
    #[allow(dead_code)]
    pub request_id: String,
}

/// Aggregated model summary
#[derive(Debug, Clone)]
pub struct SessionSummary {
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
    pub claude_quota: Option<QuotaInfo>,

    // Codex
    pub codex_records: Vec<UsageRecord>,
    pub codex_sessions: Vec<SessionSummary>,
    pub codex_total_calls: usize,
    pub codex_total_cost: f64,
    pub codex_quota: Option<QuotaInfo>,

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
            claude_quota: None,
            codex_records: Vec::with_capacity(MAX_RECORDS),
            codex_sessions: Vec::new(),
            codex_total_calls: 0,
            codex_total_cost: 0.0,
            codex_quota: None,
            active_tab: Tab::ClaudeCode,
            last_error: None,
        }
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
        let mut map: BTreeMap<String, SessionSummary> = BTreeMap::new();
        for r in &self.claude_records {
            let entry = map.entry(r.model.clone()).or_insert_with(|| SessionSummary {
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
        let mut map: BTreeMap<String, SessionSummary> = BTreeMap::new();
        for r in &self.codex_records {
            let entry = map.entry(r.model.clone()).or_insert_with(|| SessionSummary {
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
