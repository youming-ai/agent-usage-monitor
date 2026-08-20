//! Claude Code–style overview stats under the contribution heatmap.

use super::util::{format_activity_tokens, format_duration_secs, format_month_day, format_tokens};
use crate::state::{CompactDate, DayTotals, PlatformState, resolve};
use chrono::Utc;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::collections::BTreeMap;

/// Lines needed for the full overview stats block.
pub const OVERVIEW_LINES: u16 = 5;

#[derive(Debug, Clone)]
pub struct OverviewStats {
    pub favorite_model: String,
    pub total_tokens: u64,
    pub lifetime_tokens: u64,
    pub peak_tokens: u64,
    pub sessions: u64,
    pub longest_session_secs: i64,
    pub active_days: u64,
    pub span_days: u64,
    pub longest_streak: u64,
    pub current_streak: u64,
    pub most_active_day: Option<CompactDate>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl OverviewStats {
    pub fn from_platform(p: &PlatformState) -> Self {
        let mut favorite = String::from("—");
        let mut best_calls = 0u64;
        let mut input = 0u64;
        let mut output = 0u64;
        let mut cache_read = 0u64;
        let mut cache_write = 0u64;
        for m in p.models.values() {
            input += m.total_input;
            // Codex reasoning is a subset of output, not an additional token
            // category. Keep the split available without inflating totals.
            output += m.total_output;
            cache_read += m.total_cache_read;
            cache_write += m.total_cache_creation;
            if m.request_count > best_calls {
                best_calls = m.request_count;
                favorite = short_model_name(resolve(m.model));
            }
        }

        let mut longest_session_secs = 0i64;
        for s in p.sessions.values() {
            let dur = (s.last_ts - s.first_ts).num_seconds().max(0);
            longest_session_secs = longest_session_secs.max(dur);
        }

        let (active_days, span_days, longest_streak, current_streak, most_active) =
            day_stats(&p.daily);
        let peak_tokens = p.daily.values().map(|d| d.tokens).max().unwrap_or(0);

        Self {
            favorite_model: favorite,
            total_tokens: input + output + cache_read + cache_write,
            lifetime_tokens: p.lifetime_tokens,
            peak_tokens,
            sessions: p.sessions.len() as u64,
            longest_session_secs,
            active_days,
            span_days,
            longest_streak,
            current_streak,
            most_active_day: most_active,
            input_tokens: input,
            output_tokens: output,
            cache_read,
            cache_write,
        }
    }
}

/// Header used by the activity heatmap, matching the compact stats style of
/// the reference dashboard while keeping the values sourced from local logs.
pub fn activity_header(
    stats: &OverviewStats,
    accent: Color,
    visible_weeks: usize,
) -> Paragraph<'static> {
    let title = Style::default().add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let value = Style::default().fg(accent);
    let longest = if stats.longest_session_secs > 0 {
        format_duration_secs(stats.longest_session_secs)
    } else {
        "—".into()
    };

    let range = match visible_weeks {
        0 => "recent activity".into(),
        1 => "last week".into(),
        weeks => {
            let months = ((weeks as f64) * 7.0 / 30.0).round() as u64;
            match months {
                0 | 1 => "last month".into(),
                m => format!("last {m} months"),
            }
        }
    };

    Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Token activity", title),
            Span::styled(format!("    {range}"), dim),
        ]),
        Line::from(vec![
            Span::styled("Lifetime ", dim),
            Span::styled(format_activity_tokens(stats.lifetime_tokens), value),
            Span::styled("  ·  Peak ", dim),
            Span::styled(format_activity_tokens(stats.peak_tokens), value),
            Span::styled("  ·  Streak ", dim),
            Span::styled(
                format!("{}d (best {}d)", stats.current_streak, stats.longest_streak),
                value,
            ),
            Span::styled("  ·  Longest task ", dim),
            Span::styled(longest, value),
        ]),
    ])
}

/// Two-column overview as a multi-line Paragraph.
pub fn overview_paragraph(stats: &OverviewStats, accent: Color) -> Paragraph<'static> {
    let accent_style = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let label = Style::default();

    let most = stats
        .most_active_day
        .map(|d| format_month_day(d.month(), d.day()))
        .unwrap_or_else(|| "—".into());

    let longest = if stats.longest_session_secs > 0 {
        format_duration_secs(stats.longest_session_secs)
    } else {
        "—".into()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Favorite model: ", label),
            Span::styled(stats.favorite_model.clone(), accent_style),
            Span::raw("    "),
            Span::styled("Total tokens: ", label),
            Span::styled(format_tokens(stats.total_tokens), accent_style),
        ]),
        Line::from(vec![
            Span::styled("Sessions: ", label),
            Span::styled(stats.sessions.to_string(), accent_style),
            Span::raw("    "),
            Span::styled("Longest session: ", label),
            Span::styled(longest, accent_style),
        ]),
        Line::from(vec![
            Span::styled("Active days: ", label),
            Span::styled(
                format!("{}/{}", stats.active_days, stats.span_days.max(1)),
                accent_style,
            ),
            Span::raw("    "),
            Span::styled("Longest streak: ", label),
            Span::styled(format!("{} days", stats.longest_streak), accent_style),
        ]),
        Line::from(vec![
            Span::styled("Most active day: ", label),
            Span::styled(most, accent_style),
            Span::raw("    "),
            Span::styled("Current streak: ", label),
            Span::styled(format!("{} days", stats.current_streak), accent_style),
        ]),
        Line::from(vec![Span::styled(
            format!(
                "Input {} · Output {} · Cache read {} · Cache write {}",
                format_tokens(stats.input_tokens),
                format_tokens(stats.output_tokens),
                format_tokens(stats.cache_read),
                format_tokens(stats.cache_write),
            ),
            dim,
        )]),
    ];

    Paragraph::new(lines)
}

fn short_model_name(model: &str) -> String {
    // claude-opus-4-8 → Opus 4.8-ish; keep readable.
    let s = model
        .trim_start_matches("claude-")
        .trim_start_matches("gpt-");
    let mut out = String::new();
    for (i, part) in s.split('-').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if part.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            out.push_str(part);
        } else if let Some(first) = part.chars().next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(&part[first.len_utf8()..]);
        }
    }
    if out.is_empty() {
        model.to_string()
    } else {
        out
    }
}

fn day_stats(
    daily: &BTreeMap<CompactDate, DayTotals>,
) -> (u64, u64, u64, u64, Option<CompactDate>) {
    if daily.is_empty() {
        return (0, 0, 0, 0, None);
    }

    let today = CompactDate::from_datetime(Utc::now());
    let first = *daily.keys().next().unwrap();
    let last = *daily.keys().next_back().unwrap();
    let span = first.days_between(last).saturating_add(1);

    let active: Vec<CompactDate> = daily
        .iter()
        .filter(|(_, d)| d.calls > 0 || d.tokens > 0 || d.cost_usd > 0.0)
        .map(|(k, _)| *k)
        .collect();
    let active_count = active.len() as u64;

    let mut most_active = None;
    let mut best_metric = 0u64;
    for (k, d) in daily {
        let m = super::heatmap::day_metric(d);
        if m > best_metric {
            best_metric = m;
            most_active = Some(*k);
        }
    }

    // Longest consecutive streak among active days.
    let mut longest = 0u64;
    let mut run = 0u64;
    let mut prev: Option<CompactDate> = None;
    for day in &active {
        if let Some(p) = prev {
            if p.days_between(*day) == 1 {
                run += 1;
            } else {
                run = 1;
            }
        } else {
            run = 1;
        }
        longest = longest.max(run);
        prev = Some(*day);
    }

    // Current streak: consecutive days ending today (or yesterday if no activity today).
    let mut current = 0u64;
    let mut cursor = today;
    // Allow streak to end yesterday if today is empty so far.
    if !is_active(daily, cursor)
        && let Some(y) = cursor.checked_sub_days(1)
    {
        cursor = y;
    }
    loop {
        if !is_active(daily, cursor) {
            break;
        }
        current += 1;
        match cursor.checked_sub_days(1) {
            Some(prev) => cursor = prev,
            None => break,
        }
    }

    (active_count, span, longest, current, most_active)
}

fn is_active(daily: &BTreeMap<CompactDate, DayTotals>, day: CompactDate) -> bool {
    daily
        .get(&day)
        .is_some_and(|d| d.calls > 0 || d.tokens > 0 || d.cost_usd > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, Platform, UsageRecord, intern, record_id};
    use chrono::TimeZone;

    #[test]
    fn overview_counts_from_records() {
        let mut s = AppState::with_capacity(100);
        let ts = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap();
        let rec = |id: &str, model: &str, input: u64| UsageRecord {
            timestamp: ts,
            model: intern(model),
            session: intern("s1"),
            id: record_id(id),
            input_tokens: input,
            output_tokens: 10,
            cache_read_tokens: 100,
            cache_creation_tokens: 0,
            reasoning_tokens: 0,
            session_title: intern(""),
            project: intern("p"),
            cost_usd: 0.5,
        };
        s.add_records(
            Platform::ClaudeCode,
            vec![
                rec("a", "claude-opus-4", 1000),
                rec("b", "claude-opus-4", 2000),
                rec("c", "claude-sonnet-4", 100),
            ],
        );
        let ov = OverviewStats::from_platform(s.platform(Platform::ClaudeCode));
        assert_eq!(ov.sessions, 1);
        assert!(ov.favorite_model.to_lowercase().contains("opus"));
        assert_eq!(ov.active_days, 1);
        assert_eq!(ov.lifetime_tokens, 3_430);
        assert_eq!(ov.peak_tokens, 3_430);
        assert!(ov.total_tokens > 0);
    }
}
