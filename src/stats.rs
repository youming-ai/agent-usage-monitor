//! `aum stats --json` subcommand implementation.
//!
//! Produces a JSON usage report by driving each registered reader's
//! `scan_all()` and aggregating into a `StatsReport`. Independent from
//! the TUI: no event loop, no ratatui, no background tasks.

use crate::quota::{QuotaInfo, QuotaWindow};
use crate::state::{AgentPaths, Platform, UsageRecord};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

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

fn serialize_interned_map<S>(
    map: &BTreeMap<crate::state::InternedString, u64>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map_ser = serializer.serialize_map(Some(map.len()))?;
    for (k, v) in map {
        map_ser.serialize_entry(crate::state::resolve(*k), v)?;
    }
    map_ser.end()
}

#[derive(Serialize, Default)]
pub struct DateBucket {
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    #[serde(serialize_with = "serialize_interned_map")]
    pub models: BTreeMap<crate::state::InternedString, u64>,
}

#[derive(Serialize, Default, Clone)]
pub struct ToolOpsView {
    pub files_read: u64,
    pub files_edited: u64,
    pub files_added: u64,
    pub files_deleted: u64,
    pub terminal_commands: u64,
    pub lines_read: u64,
    pub lines_edited: u64,
    /// Internal per-day breakdown for MCP date filtering. The public stats
    /// JSON keeps the existing flat aggregate schema.
    #[serde(skip_serializing)]
    pub by_date: BTreeMap<crate::state::CompactDate, ToolOpsView>,
}

impl From<crate::state::ToolOps> for ToolOpsView {
    fn from(o: crate::state::ToolOps) -> Self {
        Self {
            files_read: o.files_read,
            files_edited: o.files_edited,
            files_added: o.files_added,
            files_deleted: o.files_deleted,
            terminal_commands: o.terminal_commands,
            lines_read: o.lines_read,
            lines_edited: o.lines_edited,
            by_date: o
                .by_date
                .into_iter()
                .map(|(date, ops)| (date, Self::from(ops)))
                .collect(),
        }
    }
}

#[derive(Serialize)]
pub struct PlatformReport {
    pub platform_key: String,
    pub available: bool,
    pub data_path: PathBuf,
    pub totals: PlatformTotals,
    pub models: BTreeMap<String, ModelSummary>,
    pub sessions: Vec<SessionSummaryView>,
    pub dates: BTreeMap<crate::state::CompactDate, DateBucket>,
    pub tool_ops: ToolOpsView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<QuotaView>,
}

#[derive(Clone, Serialize)]
pub struct QuotaView {
    pub tool_name: String,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    pub windows: Vec<QuotaWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_summary: Option<String>,
    pub fetched_at: String,
    pub error: Option<String>,
}

impl QuotaView {
    pub fn from_info(q: QuotaInfo, fetched_at: DateTime<Utc>) -> Self {
        Self {
            tool_name: q.tool_name,
            email: q.email,
            plan: q.plan,
            org: q.org,
            windows: q.windows,
            live_summary: q.live_summary,
            fetched_at: fetched_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            error: q.error.map(|e| e.display()),
        }
    }
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
        if let Some(s) = self.since
            && d < s
        {
            return false;
        }
        if let Some(u) = self.until
            && d > u
        {
            return false;
        }
        true
    }
}

/// Derive the canonical snake_case JSON key for a platform variant.
pub fn platform_canonical_key(platform: Platform) -> String {
    let platform_name = format!("{:?}", platform);
    let mut canonical = String::with_capacity(platform_name.len() + 2);
    let mut chars = platform_name.chars();
    if let Some(first) = chars.next() {
        canonical.push(first.to_ascii_lowercase());
        for c in chars {
            if c.is_ascii_uppercase() {
                canonical.push('_');
            }
            canonical.push(c.to_ascii_lowercase());
        }
    }
    canonical
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
            let platform_name = format!("{:?}", entry.platform);
            let canonical = platform_canonical_key(entry.platform);
            if canonical == normalized
                || platform_name == normalized
                || entry.log_name == normalized
            {
                set.insert(canonical);
                matched = true;
                break;
            }
        }
        if !matched {
            anyhow::bail!(
                "unknown platform: `{normalized}`; run `aum config set` to list valid keys"
            );
        }
    }
    Ok(set)
}

pub struct CollectOptions {
    pub include_quota: bool,
    pub filters: Filters,
}

pub fn build_platform_report(
    path: &Path,
    available: bool,
    records: Vec<UsageRecord>,
    quota: Option<QuotaView>,
    platform_key: String,
) -> PlatformReport {
    build_platform_report_with_ops(
        path,
        available,
        records,
        quota,
        platform_key,
        ToolOpsView::default(),
    )
}

pub fn build_platform_report_with_ops(
    path: &Path,
    available: bool,
    records: Vec<UsageRecord>,
    quota: Option<QuotaView>,
    platform_key: String,
    tool_ops: ToolOpsView,
) -> PlatformReport {
    let mut totals = PlatformTotals::default();
    let mut models: BTreeMap<crate::state::InternedString, ModelSummary> = BTreeMap::new();
    struct SessionAcc {
        session: crate::state::InternedString,
        calls: u64,
        cost_usd: f64,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
        models: Vec<crate::state::InternedString>,
    }
    let mut session_map: BTreeMap<crate::state::InternedString, SessionAcc> = BTreeMap::new();
    let mut dates: BTreeMap<crate::state::CompactDate, DateBucket> = BTreeMap::new();

    for r in records {
        totals.calls += 1;
        totals.cost_usd += r.cost_usd;
        totals.input_tokens += r.input_tokens;
        totals.output_tokens += r.output_tokens;
        totals.cache_read_tokens += r.cache_read_tokens;
        totals.cache_creation_tokens += r.cache_creation_tokens;

        let m = models.entry(r.model).or_default();
        m.calls += 1;
        m.cost_usd += r.cost_usd;
        m.input_tokens += r.input_tokens;
        m.output_tokens += r.output_tokens;
        m.cache_read_tokens += r.cache_read_tokens;
        m.cache_creation_tokens += r.cache_creation_tokens;

        let s = session_map.entry(r.session).or_insert_with(|| SessionAcc {
            session: r.session,
            calls: 0,
            cost_usd: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            models: Vec::new(),
        });
        s.calls += 1;
        s.cost_usd += r.cost_usd;
        s.input_tokens += r.input_tokens;
        s.output_tokens += r.output_tokens;
        s.cache_read_tokens += r.cache_read_tokens;
        s.cache_creation_tokens += r.cache_creation_tokens;
        if !s.models.contains(&r.model) {
            s.models.push(r.model);
        }

        let date_key = crate::state::CompactDate::from_datetime(r.timestamp);
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

    let mut serialized_models = BTreeMap::new();
    for (k, v) in models {
        serialized_models.insert(crate::state::resolve(k).to_string(), v);
    }

    let mut serialized_sessions = Vec::new();
    for s in session_map.into_values() {
        serialized_sessions.push(SessionSummaryView {
            session: crate::state::resolve(s.session).to_string(),
            calls: s.calls,
            cost_usd: s.cost_usd,
            input_tokens: s.input_tokens,
            output_tokens: s.output_tokens,
            cache_read_tokens: s.cache_read_tokens,
            cache_creation_tokens: s.cache_creation_tokens,
            models: s
                .models
                .into_iter()
                .map(|m| crate::state::resolve(m).to_string())
                .collect(),
        });
    }

    PlatformReport {
        platform_key,
        available,
        data_path: path.to_path_buf(),
        totals,
        models: serialized_models,
        sessions: serialized_sessions,
        dates,
        tool_ops,
        quota,
    }
}
pub async fn collect(paths: &AgentPaths, opts: CollectOptions) -> Result<StatsReport> {
    use crate::platforms;

    use std::collections::HashMap;

    // 第一遍：scan_all 收集记录（顺序，task::spawn_blocking 包 I/O）
    let mut entries: Vec<(String, Platform, PathBuf, Vec<UsageRecord>, ToolOpsView)> = Vec::new();
    for entry in platforms::entries() {
        let key = platform_canonical_key(entry.platform);
        if !opts.filters.matches_platform(&key) {
            continue;
        }
        let path = paths.path_for(entry.platform);
        // Platforms without a local session reader keep their row empty in the
        // report (currently none are registered, but the branch is defensive).
        let Some(mut reader) = entry.build_reader(path.clone()) else {
            entries.push((
                key,
                entry.platform,
                path,
                Vec::new(),
                ToolOpsView::from(crate::state::ToolOps::default()),
            ));
            continue;
        };
        let (records, tool_ops) = tokio::task::spawn_blocking(move || {
            let records = reader.scan_all();
            let ops = reader.take_tool_ops_delta();
            (records, ops)
        })
        .await
        .context("reader task failed")?;
        let records = records.with_context(|| format!("failed to read {}", path.display()))?;
        entries.push((
            key,
            entry.platform,
            path,
            records,
            ToolOpsView::from(tool_ops),
        ));
    }

    // Quota：仅在 --include-quota 时拉取。fetch() 是阻塞 HTTP，调完后取时间戳
    // 作为 fetched_at 写入 QuotaView（Instant 无法转 RFC3339，必须用 DateTime<Utc>）。
    let quota_views: Option<HashMap<Platform, QuotaView>> = if opts.include_quota {
        // Platforms without a usage API still report who is signed in; they'd
        // otherwise vanish from `--json` entirely.
        let (quotas, accounts) = tokio::task::spawn_blocking(|| {
            let entries = crate::platforms::entries();
            let quotas = entries
                .iter()
                .filter_map(|entry| entry.quota_fetcher.map(|fetch| (entry.platform, fetch())))
                .collect::<Vec<_>>();
            let accounts = entries
                .iter()
                .filter_map(|entry| {
                    entry
                        .account_fetcher
                        .map(|fetch| (entry.platform, entry.log_name, fetch()))
                })
                .collect::<Vec<_>>();
            (quotas, accounts)
        })
        .await
        .unwrap_or_default();
        let now = Utc::now();
        let fetched_at = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut views: HashMap<Platform, QuotaView> = quotas
            .into_iter()
            .filter_map(|(p, q)| q.map(|qi| (p, QuotaView::from_info(qi, now))))
            .collect();
        for (platform, tool_name, email) in accounts {
            views.entry(platform).or_insert_with(|| QuotaView {
                tool_name: tool_name.to_string(),
                error: email
                    .is_none()
                    .then(|| crate::quota::QuotaError::NoCredentials.display()),
                email,
                plan: None,
                org: None,
                windows: vec![],
                live_summary: None,
                fetched_at: fetched_at.clone(),
            });
        }
        Some(views)
    } else {
        None
    };

    // 第二遍：聚合
    let mut report = StatsReport {
        generated_at: Utc::now(),
        platforms: BTreeMap::new(),
        totals: Totals::default(),
    };
    for (key, platform, path, records, tool_ops) in entries {
        let filtered: Vec<UsageRecord> = records
            .into_iter()
            .filter(|r| opts.filters.matches_date(r.timestamp))
            .collect();
        let available = path.exists();
        let quota = quota_views.as_ref().and_then(|m| m.get(&platform).cloned());
        let pr = build_platform_report_with_ops(
            &path,
            available,
            filtered,
            quota,
            key.clone(),
            tool_ops,
        );
        if pr.available {
            report.totals.platforms_with_data += 1;
        }
        report.totals.total_calls += pr.totals.calls;
        report.totals.total_cost_usd += pr.totals.cost_usd;
        report.platforms.insert(key, pr);
    }
    Ok(report)
}
pub fn write_json<W: Write>(report: &StatsReport, pretty: bool, out: W) -> Result<()> {
    let writer = std::io::BufWriter::new(out);
    if pretty {
        serde_json::to_writer_pretty(writer, report).context("serialize pretty json")?;
    } else {
        serde_json::to_writer(writer, report).context("serialize compact json")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::QuotaError;
    use chrono::TimeZone;

    fn rec(
        model: &str,
        session: &str,
        day: u32,
        input: u64,
        output: u64,
        cost: f64,
    ) -> UsageRecord {
        UsageRecord {
            timestamp: Utc.with_ymd_and_hms(2026, 6, day, 12, 0, 0).unwrap(),
            session: crate::state::intern(session),
            id: crate::state::record_id(&format!("{session}:{day}:{input}:{output}")),
            input_tokens: input,
            output_tokens: output,
            cost_usd: cost,
            ..crate::state::test_record(model)
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
        let pr = build_platform_report(&path, true, records, None, "claude_code".to_string());
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
        let pr = build_platform_report(&path, true, records, None, "claude_code".to_string());
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
        let pr = build_platform_report(&path, true, records, None, "claude_code".to_string());
        assert_eq!(pr.dates.len(), 2);
        let day_15 = pr
            .dates
            .get(&crate::state::CompactDate::new(2026, 6, 15))
            .unwrap();
        assert_eq!(day_15.calls, 2);
        assert_eq!(
            day_15.models.get(&crate::state::intern("claude-sonnet-4")),
            Some(&2)
        );
    }

    #[test]
    fn build_platform_report_session_lists_models_distinct() {
        let path = PathBuf::from("/tmp/.claude/projects");
        let records = vec![
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
            rec("claude-opus-4", "s1", 15, 200, 100, 0.50),
            rec("claude-sonnet-4", "s1", 15, 100, 50, 0.10),
        ];
        let pr = build_platform_report(&path, true, records, None, "claude_code".to_string());
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
    fn resolve_platform_filter_accepts_platform_variant() {
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
        let result =
            resolve_platform_filter(&["claude_code".to_string(), "ClaudeCode".to_string()])
                .unwrap();
        assert_eq!(result.len(), 1);
    }
    #[test]
    fn quota_view_from_info_copies_fields() {
        use crate::quota::QuotaWindow;
        use std::time::Instant;
        let info = QuotaInfo {
            tool_name: "Claude Code".to_string(),
            email: Some("me@example.com".to_string()),
            account_id: None,
            plan: Some("claude max 5x".into()),
            org: Some("elestyle".into()),
            windows: vec![QuotaWindow {
                label: "5h".to_string(),
                remaining_percent: Some(0.85),
                resets_at: None,
                reset_in: Some("3h 25m".to_string()),
            }],
            live_summary: Some("extra usage on".into()),
            fetched_at: Instant::now(),
            error: None,
        };
        let fetched_at = Utc::now();
        let view = QuotaView::from_info(info, fetched_at);
        assert_eq!(view.tool_name, "Claude Code");
        assert_eq!(view.email, Some("me@example.com".to_string()));
        assert_eq!(view.windows.len(), 1);
        assert_eq!(view.windows[0].label, "5h");
        assert_eq!(view.plan.as_deref(), Some("claude max 5x"));
        assert!(view.fetched_at.contains("T"));
    }

    #[test]
    fn quota_view_from_info_captures_error_string() {
        use std::time::Instant;
        let info = QuotaInfo {
            tool_name: "Codex".to_string(),
            email: None,
            account_id: None,
            plan: None,
            org: None,
            windows: vec![],
            live_summary: None,
            fetched_at: Instant::now(),
            error: Some(QuotaError::Auth("no token".to_string())),
        };
        let view = QuotaView::from_info(info, Utc::now());
        assert!(view.error.is_some());
        let err_str = view.error.unwrap();
        assert!(
            err_str.contains("re-auth") || err_str.contains("no token"),
            "error string should contain context, got: {err_str}"
        );
    }
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
                platform_key: "claude_code".to_string(),
                available: true,
                data_path: PathBuf::from("/tmp/x"),
                totals: PlatformTotals::default(),
                models: BTreeMap::new(),
                sessions: vec![],
                dates: BTreeMap::new(),
                tool_ops: ToolOpsView::default(),
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
        assert!(
            !s.contains("\"quota\""),
            "quota field should be skipped when None"
        );
    }
}
