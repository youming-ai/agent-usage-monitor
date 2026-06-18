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
use std::collections::{BTreeMap, BTreeSet};
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
            let tab_name = format!("{:?}", entry.tab);
            let mut canonical = String::with_capacity(tab_name.len() + 2);
            let mut chars = tab_name.chars();
            if let Some(first) = chars.next() {
                canonical.push(first.to_ascii_lowercase());
                for c in chars {
                    if c.is_ascii_uppercase() {
                        canonical.push('_');
                    }
                    canonical.push(c.to_ascii_lowercase());
                }
            }
            if canonical == normalized || tab_name == normalized || entry.log_name == normalized
            {
                set.insert(canonical);
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

pub struct CollectOptions {
    pub include_quota: bool,
    pub filters: Filters,
}

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
}
