mod heatmap;
mod overview;
mod quota_bar;
mod util;

use crate::state::{AppState, Platform};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::sync::{Arc, RwLock};

/// Horizontal inset so the dashboard never touches the terminal edges.
const CONTENT_PADDING: u16 = 2;

pub fn render(frame: &mut Frame, app_state: &Arc<RwLock<AppState>>) {
    let state = match app_state.try_read() {
        Ok(state) => state,
        Err(_) => return,
    };

    let available = &state.available_platforms;
    if available.is_empty() {
        frame.render_widget(
            Paragraph::new(" no agent data found — check `aum config show` for data paths ")
                .style(Style::default().fg(Color::DarkGray)),
            frame.area(),
        );
        return;
    }

    // One blank row between platform panels so adjacent headers don't touch.
    let mut constraints: Vec<Constraint> = Vec::with_capacity(available.len() * 2);
    for (i, _) in available.iter().enumerate() {
        if i > 0 {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Ratio(1, available.len() as u32));
    }
    let area = frame.area().inner(Margin {
        vertical: 0,
        horizontal: CONTENT_PADDING,
    });
    let chunks = Layout::vertical(constraints).split(area);
    // Constraints interleave platform blocks with one-row spacers, so the
    // platform for index `i` lives at chunk `i * 2`.
    for (i, &platform) in available.iter().enumerate() {
        render_platform(frame, chunks[i * 2], platform, &state);
    }
}

fn render_platform(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    platform: Platform,
    state: &AppState,
) {
    let p = state.platform(platform);
    let (quota, _, _, _) = p.refs();
    let accent = platform.primary_color();

    let has_quota = crate::platforms::entry_for_platform(platform).has_quota();
    let h = area.height;

    let header_h: u16 = if h >= 2 { 2 } else { 1 };
    let quota_h: u16 = if has_quota && h >= 4 {
        quota_bar::quota_panel_height(quota).min(h.saturating_sub(header_h + 3))
    } else {
        0
    };
    let content_remain = h.saturating_sub(header_h + quota_h);
    let full_activity_h = 2 + heatmap::HEATMAP_FULL_HEIGHT;

    // The reference layout gets a two-line activity summary above the year
    // grid when there is room. Shorter sections keep the old source label and
    // degrade from the year grid to the compact strip without clipping the
    // header.
    let (local_label_h, activity_header_h, heatmap_h, overview_h) =
        if content_remain >= full_activity_h {
            (0, 2, heatmap::HEATMAP_FULL_HEIGHT, 0)
        } else if content_remain > heatmap::HEATMAP_FULL_HEIGHT {
            (1, 0, heatmap::HEATMAP_FULL_HEIGHT, 0)
        } else if content_remain > heatmap::HEATMAP_MIN_HEIGHT {
            (1, 0, heatmap::HEATMAP_MIN_HEIGHT, 0)
        } else if content_remain >= overview::OVERVIEW_LINES + 2 {
            // Not enough rows for the year grid: one-line strip + stats.
            (1, 0, 1, overview::OVERVIEW_LINES)
        } else if content_remain >= 2 {
            (1, 0, 1, 0)
        } else if content_remain >= 1 {
            (1, 0, 0, 0)
        } else {
            (0, 0, 0, 0)
        };

    let chunks = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(quota_h),
        Constraint::Length(local_label_h),
        Constraint::Length(activity_header_h),
        Constraint::Length(heatmap_h),
        Constraint::Min(overview_h),
    ])
    .split(area);

    // Header: platform label left, account right.
    let header_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(accent));
    let header_inner = header_block.inner(chunks[0]);
    frame.render_widget(header_block, chunks[0]);

    let email = quota
        .and_then(|q| q.email.as_deref())
        .or(p.account_email.as_deref());
    let acct_w = email.map(|e| e.chars().count() as u16 + 4).unwrap_or(15);
    let header_cols =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(acct_w)]).split(header_inner);

    let mut spans = vec![Span::styled(
        format!(" {} ", platform.label()),
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )];
    if let Some(error) = p.reader_error.as_deref() {
        spans.push(Span::styled(
            format!(" · reader error: {error}"),
            Style::default().fg(Color::Red),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), header_cols[0]);

    let account_line = match email {
        Some(e) => Line::from(Span::raw(format!("✓ {e} "))),
        None => Line::from(Span::styled(
            "not signed in ",
            Style::default().fg(Color::DarkGray),
        )),
    };
    frame.render_widget(
        Paragraph::new(account_line).alignment(Alignment::Right),
        header_cols[1],
    );

    if quota_h > 0 {
        frame.render_widget(quota_bar::quota_panel(platform, quota), chunks[1]);
    }

    if local_label_h > 0 {
        frame.render_widget(
            Paragraph::new(" local activity (from logs)")
                .style(Style::default().fg(Color::DarkGray)),
            chunks[2],
        );
    }

    let heat_area = chunks[4];
    if activity_header_h > 0 {
        let stats = overview::OverviewStats::from_platform(p);
        frame.render_widget(
            overview::activity_header(&stats, accent, heatmap::visible_weeks(heat_area.width)),
            chunks[3],
        );
    }
    if heat_area.height >= heatmap::HEATMAP_MIN_HEIGHT && heat_area.width > heatmap::GUTTER {
        frame.render_widget(heatmap::contribution_heatmap(&p.daily, accent), heat_area);
    } else if heat_area.height >= 1 && heat_area.width > 0 {
        frame.render_widget(heatmap::contribution_strip(&p.daily, accent), heat_area);
    }

    if overview_h > 0 && chunks[5].height >= 3 {
        let stats = overview::OverviewStats::from_platform(p);
        frame.render_widget(overview::overview_paragraph(&stats, accent), chunks[5]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{QuotaInfo, QuotaWindow};
    use crate::state::{CompactDate, DayTotals, UsageRecord};
    use ratatui::{Terminal, backend::TestBackend};
    use std::time::Instant;

    fn dump(width: u16, height: u16, state: AppState) -> String {
        let state = Arc::new(RwLock::new(state));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| render(f, &state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..height {
            for x in 0..width {
                out.push_str(buf.cell((x, y)).unwrap().symbol());
            }
            out.push('\n');
        }
        out
    }

    fn sample_state() -> AppState {
        let mut s = AppState::new();
        s.set_available_platforms(Platform::all().iter().copied());
        let claude = s.platform_mut(Platform::ClaudeCode);
        claude.quota = Some(QuotaInfo {
            tool_name: "Claude Code".into(),
            email: Some("you@mail.com".into()),
            account_id: None,
            plan: Some("claude max 5x".into()),
            org: Some("elestyle".into()),
            windows: vec![
                QuotaWindow {
                    label: "5h".into(),
                    remaining_percent: Some(0.82),
                    resets_at: None,
                    reset_in: Some("2h30m".into()),
                },
                QuotaWindow {
                    label: "7d".into(),
                    remaining_percent: Some(0.54),
                    resets_at: None,
                    reset_in: Some("4d6h".into()),
                },
            ],
            live_summary: Some("extra usage on".into()),
            fetched_at: Instant::now(),
            error: None,
        });

        let today = CompactDate::from_datetime(chrono::Utc::now());
        for days_ago in 0..14 {
            if let Some(d) = today.checked_sub_days(days_ago) {
                s.platform_mut(Platform::ClaudeCode).daily.insert(
                    d,
                    DayTotals {
                        cost_usd: 1.0 + days_ago as f64 * 0.1,
                        tokens: 100_000,
                        calls: 5,
                    },
                );
            }
        }

        let mk = |session: &str, model: &str, input: u64, output: u64| UsageRecord {
            session: crate::state::intern(session),
            id: crate::state::record_id(&format!("{session}:{model}:{input}:{output}")),
            input_tokens: input,
            output_tokens: output,
            ..crate::state::test_record(model)
        };
        s.add_records(
            Platform::ClaudeCode,
            vec![
                mk("demo a3f2c1d8", "claude-opus-4", 1200, 340),
                mk("demo a3f2c1d8", "claude-opus-4", 800, 200),
            ],
        );
        s
    }

    #[test]
    fn renders_heatmap_and_overview() {
        // Tall enough for heatmap + overview on two platforms.
        let out = dump(100, 40, sample_state());
        println!("\n{out}");
        assert!(out.contains("CLAUDE"));
        assert!(out.contains("CODEX"));
        assert!(out.contains("82%"), "claude quota window");
        assert!(out.contains("you@mail.com"));
        assert!(out.contains("live"), "live quota label");
        assert!(
            out.contains("claude max") || out.contains("elestyle") || out.contains("extra"),
            "live plan/org/summary"
        );
        assert!(out.contains('■') || out.contains('·'), "heatmap cells");
        assert!(
            out.contains("Token activity"),
            "local data must stay labeled"
        );
        assert!(
            ["Mon", "Wed", "Fri"]
                .into_iter()
                .all(|label| out.contains(label)),
            "year heatmap should include weekday rows:\n{out}"
        );
        assert!(out.contains("Token activity"), "activity summary");
        assert!(!out.contains("MODEL"));
        assert!(!out.contains("SESSION"));
    }

    #[test]
    fn renders_on_a_very_short_terminal_without_panicking() {
        for h in 3..=7 {
            let _ = dump(80, h, sample_state());
        }
    }

    #[test]
    fn standard_height_keeps_local_activity_label() {
        let out = dump(80, 24, sample_state());
        assert!(
            out.contains("local activity"),
            "local heatmaps must be labeled:\n{out}"
        );
    }

    #[test]
    fn heatmap_fills_the_padded_platform_width() {
        let out = dump(100, 40, sample_state());
        let row = out
            .lines()
            .find(|line| line.trim_start().starts_with("Mon"))
            .expect("heatmap weekday row");
        let last_cell = row
            .chars()
            .enumerate()
            .filter(|(_, ch)| matches!(ch, '■' | '·'))
            .map(|(index, _)| index)
            .max()
            .expect("heatmap cells");
        // The grid stretches across the full (padded) width, not the old
        // half-width mark.
        assert!(last_cell > 80, "heatmap did not fill the width: {row}");
        assert!(out.contains("months"), "header range is stale:\n{out}");
    }

    #[test]
    fn renders_hint_when_no_platform_is_available() {
        let out = dump(80, 8, AppState::new());
        assert!(
            out.contains("no agent data found"),
            "missing all platforms must show a hint:\n{out}"
        );
    }
}
