use crate::quota::QuotaInfo;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

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
    OpenCode,
    KimiCode,
    Pi,
    OpenClaw,
    Hermes,
    Factory,
    Grok,
    Cursor,
    Copilot,
    Antigravity,
    MimoCode,
}

impl Platform {
    /// Number of platform variants — used to size the fixed array in `AppState`.
    pub const COUNT: usize = 13;

    /// Zero-based index for array access. Must stay in sync with variant order.
    pub const fn index(self) -> usize {
        match self {
            Platform::ClaudeCode => 0,
            Platform::Codex => 1,
            Platform::OpenClaw => 2,
            Platform::Hermes => 3,
            Platform::OpenCode => 4,
            Platform::KimiCode => 5,
            Platform::Pi => 6,
            Platform::Factory => 7,
            Platform::Grok => 8,
            Platform::Cursor => 9,
            Platform::Copilot => 10,
            Platform::Antigravity => 11,
            Platform::MimoCode => 12,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    ClaudeCode,
    Codex,
    OpenCode,
    KimiCode,
    Pi,
    OpenClaw,
    Hermes,
    Factory,
    Grok,
    Cursor,
    Copilot,
    Antigravity,
    MimoCode,
}

impl Tab {
    /// Zero-based index for array access. Mirrors `Platform::index()` by
    /// construction — the two enums share variant order.
    pub const fn index(self) -> usize {
        match self {
            Tab::ClaudeCode => 0,
            Tab::Codex => 1,
            Tab::OpenClaw => 2,
            Tab::Hermes => 3,
            Tab::OpenCode => 4,
            Tab::KimiCode => 5,
            Tab::Pi => 6,
            Tab::Factory => 7,
            Tab::Grok => 8,
            Tab::Cursor => 9,
            Tab::Copilot => 10,
            Tab::Antigravity => 11,
            Tab::MimoCode => 12,
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
            Tab::OpenClaw => "OPENCLAW",
            Tab::Hermes => "HERMES",
            Tab::OpenCode => "OPENCODE",
            Tab::KimiCode => "KIMI-CODE",
            Tab::Pi => "PI",
            Tab::Factory => "FACTORY",
            Tab::Grok => "GROK",
            Tab::Cursor => "CURSOR",
            Tab::Copilot => "COPILOT",
            Tab::Antigravity => "ANTIGRAVITY",
            Tab::MimoCode => "MIMO-CODE",
        }
    }

    /// Primary color for the tab (used for borders and accents).
    /// Sourced from each agent's official CLI theme / brand palette.
    pub fn primary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(217, 119, 87),
            Tab::Codex => ratatui::style::Color::Rgb(215, 95, 215),
            Tab::OpenClaw => ratatui::style::Color::Rgb(255, 90, 45),
            Tab::Hermes => ratatui::style::Color::Rgb(255, 215, 0),
            Tab::OpenCode => ratatui::style::Color::Rgb(250, 178, 131),
            Tab::KimiCode => ratatui::style::Color::Rgb(103, 232, 249),
            Tab::Pi => ratatui::style::Color::Rgb(138, 190, 183),
            Tab::Factory => ratatui::style::Color::Rgb(242, 123, 47),
            Tab::Grok => ratatui::style::Color::Rgb(187, 154, 247),
            Tab::Cursor => ratatui::style::Color::Rgb(136, 192, 208),
            Tab::Copilot => ratatui::style::Color::Rgb(35, 134, 54),
            Tab::Antigravity => ratatui::style::Color::Rgb(66, 133, 244),
            Tab::MimoCode => ratatui::style::Color::Rgb(255, 103, 0),
        }
    }

    #[allow(dead_code)]
    pub fn secondary_color(self) -> ratatui::style::Color {
        match self {
            Tab::ClaudeCode => ratatui::style::Color::Rgb(254, 205, 170),
            Tab::Codex => ratatui::style::Color::Rgb(240, 176, 240),
            Tab::OpenClaw => ratatui::style::Color::Rgb(255, 184, 158),
            Tab::Hermes => ratatui::style::Color::Rgb(255, 235, 128),
            Tab::OpenCode => ratatui::style::Color::Rgb(255, 212, 184),
            Tab::KimiCode => ratatui::style::Color::Rgb(165, 243, 252),
            Tab::Pi => ratatui::style::Color::Rgb(197, 228, 224),
            Tab::Factory => ratatui::style::Color::Rgb(255, 201, 160),
            Tab::Grok => ratatui::style::Color::Rgb(221, 208, 252),
            Tab::Cursor => ratatui::style::Color::Rgb(184, 224, 235),
            Tab::Copilot => ratatui::style::Color::Rgb(155, 215, 165),
            Tab::Antigravity => ratatui::style::Color::Rgb(154, 194, 249),
            Tab::MimoCode => ratatui::style::Color::Rgb(255, 178, 102),
        }
    }

    /// Whether this platform has a quota API endpoint (Claude + Codex only).
    /// Map from the corresponding `Platform` variant. The two enums share
    /// variant order, so this is a constant-time match.
    pub const fn from_platform(p: Platform) -> Self {
        match p {
            Platform::ClaudeCode => Tab::ClaudeCode,
            Platform::Codex => Tab::Codex,
            Platform::OpenClaw => Tab::OpenClaw,
            Platform::Hermes => Tab::Hermes,
            Platform::OpenCode => Tab::OpenCode,
            Platform::KimiCode => Tab::KimiCode,
            Platform::Pi => Tab::Pi,
            Platform::Factory => Tab::Factory,
            Platform::Grok => Tab::Grok,
            Platform::Cursor => Tab::Cursor,
            Platform::Copilot => Tab::Copilot,
            Platform::Antigravity => Tab::Antigravity,
            Platform::MimoCode => Tab::MimoCode,
        }
    }

    pub const fn has_quota_api(self) -> bool {
        matches!(self, Tab::ClaudeCode | Tab::Codex)
    }

    pub fn all() -> &'static [Tab] {
        &[
            Tab::ClaudeCode,
            Tab::Codex,
            Tab::OpenClaw,
            Tab::Hermes,
            Tab::OpenCode,
            Tab::KimiCode,
            Tab::Pi,
            Tab::Factory,
            Tab::Grok,
            Tab::Cursor,
            Tab::Copilot,
            Tab::Antigravity,
            Tab::MimoCode,
        ]
    }

    pub fn default_path(self) -> std::path::PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        match self {
            Tab::ClaudeCode => home.join(".claude/projects"),
            Tab::Codex => home.join(".codex"),
            Tab::OpenClaw => home.join(".openclaw/agents"),
            Tab::Hermes => home.join(".hermes"),
            Tab::OpenCode => crate::config::xdg_data_dir().join("opencode"),
            Tab::KimiCode => home.join(".kimi-code"),
            Tab::Pi => home.join(".pi/agent/sessions"),
            Tab::Factory => home.join(".factory/projects"),
            Tab::Grok => home.join(".grok"),
            Tab::Cursor => home.join(".cursor"),
            Tab::Copilot => home.join(".copilot"),
            Tab::Antigravity => home.join(".gemini/antigravity-cli"),
            Tab::MimoCode => crate::config::xdg_data_dir().join("mimocode"),
        }
    }

    /// Shared detection logic: does the agent's data exist at this path?
    fn is_available_at_path(path: &std::path::Path, tab: Tab) -> bool {
        match tab {
            Tab::OpenClaw => path.exists(),
            Tab::Hermes => path.join("state.db").exists(),
            Tab::OpenCode => path.join("opencode.db").exists(),
            Tab::KimiCode => path.exists(),
            Tab::Pi => path.exists(),
            Tab::Factory => path.exists(),
            Tab::Grok => path.join("sessions").exists(),
            Tab::Cursor => {
                path.join("projects").exists() || path.join("chats").exists()
            }
            Tab::Copilot => path.join("session-state").exists(),
            Tab::Antigravity => path.join("brain").exists(),
            Tab::MimoCode => path.join("mimocode.db").exists(),
            _ => path.exists(),
        }
    }

    pub fn is_available_at(self, paths: &AgentPaths) -> bool {
        let path = paths.path_for(self);
        Self::is_available_at_path(&path, self)
    }

    #[allow(dead_code)]
    pub fn is_available(self) -> bool {
        let path = self.default_path();
        Self::is_available_at_path(&path, self)
    }
}

/// Single API call record
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    #[allow(dead_code)]
    pub platform: Platform,
    pub model: String,
    pub session: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cost_usd: f64,
}

/// Aggregated per-model totals.
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

/// Per-platform state held in a `[PlatformState; Platform::COUNT]` array.
#[derive(Debug)]
pub struct PlatformState {
    pub records: VecDeque<UsageRecord>,
    pub sessions: HashMap<String, SessionSummary>,
    pub total_calls: usize,
    pub total_cost: f64,
    pub quota: Option<QuotaInfo>,
    pub max_records: usize,
}

impl PlatformState {
    /// Borrow references for the UI render pass.
    pub fn refs(&self) -> (Option<&QuotaInfo>, &HashMap<String, SessionSummary>, &VecDeque<UsageRecord>, usize, f64) {
        (self.quota.as_ref(), &self.sessions, &self.records, self.total_calls, self.total_cost)
    }
}

/// Global application state
pub struct AppState {
    pub platforms: [PlatformState; Platform::COUNT],
    pub active_tab: Tab,
    pub available_tabs: Vec<Tab>,
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
        };
        Self {
            platforms: [
                make(), make(), make(), make(), make(), make(),
                make(), make(), make(), make(), make(), make(),
                make(),
            ],
            active_tab: Tab::ClaudeCode,
            available_tabs: Vec::new(),
        }
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
            if p.records.len() >= p.max_records
                && let Some(old) = p.records.pop_front() {
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
    }

    pub fn detect_available_tabs(&mut self, paths: &AgentPaths) {
        self.available_tabs = Tab::all()
            .iter()
            .filter(|tab| tab.is_available_at(paths))
            .copied()
            .collect();

        if !self.available_tabs.contains(&self.active_tab) {
            self.active_tab = self.available_tabs.first().copied().unwrap_or(Tab::ClaudeCode);
        }
    }
}

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

fn reverse_model_aggregate(map: &mut HashMap<String, SessionSummary>, r: &UsageRecord) {
    if let Some(entry) = map.get_mut(&r.model) {
        entry.total_input = entry.total_input.saturating_sub(r.input_tokens);
        entry.total_output = entry.total_output.saturating_sub(r.output_tokens);
        entry.total_cache_read = entry.total_cache_read.saturating_sub(r.cache_read_tokens);
        entry.total_cache_creation = entry.total_cache_creation.saturating_sub(r.cache_creation_tokens);
        entry.total_cost -= r.cost_usd;
        // Clamp f64 drift near zero so the next upsert doesn't start from a
        // tiny negative value.
        if entry.total_cost < 0.0 && entry.total_cost > -1e-9 {
            entry.total_cost = 0.0;
        }
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

    fn rec(platform: Platform, model: &str, input: u64, output: u64, cost: f64) -> UsageRecord {
        UsageRecord {
            timestamp: Utc::now(),
            platform,
            model: model.into(),
            session: "test".into(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: cost,
        }
    }

    fn claude_idx() -> usize { Tab::ClaudeCode.index() }
    fn codex_idx() -> usize { Tab::Codex.index() }

    #[test]
    fn platform_and_tab_indices_are_in_sync() {
        // If these ever drift, array access will panic at runtime.
        for tab in Tab::all() {
            let p = match tab {
                Tab::ClaudeCode => Platform::ClaudeCode,
                Tab::Codex => Platform::Codex,
                Tab::OpenClaw => Platform::OpenClaw,
                Tab::Hermes => Platform::Hermes,
                Tab::OpenCode => Platform::OpenCode,
                Tab::KimiCode => Platform::KimiCode,
                Tab::Pi => Platform::Pi,
                Tab::Factory => Platform::Factory,
                Tab::Grok => Platform::Grok,
                Tab::Cursor => Platform::Cursor,
                Tab::Copilot => Platform::Copilot,
                Tab::Antigravity => Platform::Antigravity,
                Tab::MimoCode => Platform::MimoCode,
            };
            assert_eq!(tab.index(), p.index(), "mismatch for {tab:?} / {p:?}");
        }
    }

    #[test]
    fn add_records_dispatches_by_platform() {
        let mut s = AppState::with_capacity(10);
        s.add_records(Platform::ClaudeCode, vec![rec(Platform::ClaudeCode, "opus-4", 100, 50, 1.0)]);
        s.add_records(Platform::Codex, vec![rec(Platform::Codex, "gpt-5", 200, 80, 2.0)]);
        assert_eq!(s.platforms[claude_idx()].total_calls, 1);
        assert_eq!(s.platforms[codex_idx()].total_calls, 1);
        assert!(s.platforms[claude_idx()].sessions.contains_key("opus-4"));
        assert!(s.platforms[codex_idx()].sessions.contains_key("gpt-5"));
    }

    #[test]
    fn eviction_reverses_aggregate() {
        let mut s = AppState::with_capacity(2);
        s.add_records(Platform::ClaudeCode, vec![
            rec(Platform::ClaudeCode, "opus-4", 100, 50, 1.0),
            rec(Platform::ClaudeCode, "opus-4", 200, 80, 2.0),
            rec(Platform::ClaudeCode, "opus-4", 300, 90, 3.0),
        ]);
        let m = s.platforms[claude_idx()].sessions.get("opus-4").expect("model present");
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
        s.add_records(Platform::ClaudeCode, vec![rec(Platform::ClaudeCode, "opus-4", 100, 50, 1.0)]);
        s.add_records(Platform::ClaudeCode, vec![rec(Platform::ClaudeCode, "sonnet-4", 200, 80, 2.0)]);
        let c = &s.platforms[claude_idx()];
        assert!(!c.sessions.contains_key("opus-4"));
        assert!(c.sessions.contains_key("sonnet-4"));
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
            (Tab::OpenCode, missing.join("opencode")),
            (Tab::KimiCode, missing.join("kimi")),
            (Tab::Pi, missing.join("pi")),
            (Tab::OpenClaw, missing.join("openclaw")),
            (Tab::Hermes, missing.join("hermes")),
            (Tab::Factory, missing.join("factory")),
            (Tab::Grok, missing.join("grok")),
            (Tab::Cursor, missing.join("cursor")),
            (Tab::Copilot, missing.join("copilot")),
            (Tab::Antigravity, missing.join("antigravity")),
            (Tab::MimoCode, missing.join("mimocode")),
        ]));

        let mut state = AppState::new();
        state.detect_available_tabs(&paths);
        assert_eq!(state.available_tabs, vec![Tab::ClaudeCode]);
    }

    #[test]
    fn opencode_default_path_uses_xdg_not_macos_app_support() {
        let p = Tab::OpenCode.default_path();
        assert!(
            !p.to_string_lossy().contains("Application Support"),
            "opencode should use XDG, got {p:?}"
        );
        assert!(
            p.ends_with("opencode"),
            "opencode default path should end with 'opencode', got {p:?}"
        );
    }
}
