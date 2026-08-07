mod heatmap;
mod model_table;
mod quota_bar;
mod session_table;
mod util;

use crate::state::{AppState, Platform};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::sync::{Arc, RwLock};

const SESSION_TOP_N: usize = 5;

pub fn render(frame: &mut Frame, app_state: &Arc<RwLock<AppState>>) {
    let state = match app_state.try_read() {
        Ok(state) => state,
        Err(_) => return,
    };

    // Every platform whose data directory exists, stacked top to bottom in
    // registry order. Sections split the screen equally; a section whose
    // model rows exceed its share clips at the bottom (rows are sorted by
    // cost, so the most relevant stay visible).
    let available = &state.available_platforms;
    if available.is_empty() {
        frame.render_widget(
            Paragraph::new(" no agent data found — check `aum config show` for data paths ")
                .style(Style::default().fg(Color::DarkGray)),
            frame.area(),
        );
        return;
    }

    let constraints: Vec<Constraint> = available
        .iter()
        .map(|_| Constraint::Ratio(1, available.len() as u32))
        .collect();
    let chunks = Layout::vertical(constraints).split(frame.area());
    for (i, &platform) in available.iter().enumerate() {
        render_platform(frame, chunks[i], platform, &state);
    }
}

fn render_platform(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    platform: Platform,
    state: &AppState,
) {
    let p = state.platform(platform);
    let (quota, models, _, _) = p.refs();
    let accent = platform.primary_color();

    let has_quota = crate::platforms::entry_for_platform(platform).has_quota();
    let h = area.height;

    // Priority under tight height: header > quota > models > heatmap > sessions.
    // Drop sessions first, then shrink heatmap to a 1-row strip, then drop it.
    let quota_h: u16 = if has_quota && h >= 4 { 1 } else { 0 };
    let header_h: u16 = if h >= 2 { 2 } else { 1 };
    let remain_after_header = h.saturating_sub(header_h + quota_h);

    let (heatmap_h, sessions_h) = if remain_after_header >= 14 {
        (7u16, 6u16) // full 7-row graph + top-N sessions
    } else if remain_after_header >= 10 {
        (7, 0)
    } else if remain_after_header >= 5 {
        (1, 0) // strip
    } else {
        (0, 0)
    };

    let chunks = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(quota_h),
        Constraint::Length(heatmap_h),
        Constraint::Min(1),
        Constraint::Length(sessions_h),
    ])
    .split(area);

    // Header row: accent underline, platform label on the left, account
    // identity right-aligned. Per-model tokens/cost live in the table below.
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

    if heatmap_h >= 7 {
        let weeks = chunks[2].width.saturating_sub(2).min(26);
        frame.render_widget(heatmap::contribution_heatmap(&p.daily, weeks), chunks[2]);
    } else if heatmap_h == 1 {
        frame.render_widget(
            heatmap::contribution_strip(&p.daily, chunks[2].width),
            chunks[2],
        );
    }

    frame.render_widget(model_table::model_table(models, platform), chunks[3]);

    if sessions_h > 0 {
        let top = p.top_sessions(SESSION_TOP_N);
        if !top.is_empty() {
            frame.render_widget(session_table::session_table(&top), chunks[4]);
        }
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
            fetched_at: Instant::now(),
            error: None,
        });
        let add_model = |state: &mut AppState, p: Platform, model: &str, cost: f64, calls: u64| {
            let ps = state.platform_mut(p);
            let m = crate::state::intern(model);
            ps.models.insert(
                m,
                crate::state::ModelTotals {
                    model: m,
                    total_input: 1_200_000,
                    total_output: 340_000,
                    total_cache_read: 8_100_000,
                    total_cache_creation: 0,
                    total_reasoning: 0,
                    total_cost: cost,
                    request_count: calls,
                },
            );
        };
        add_model(&mut s, Platform::ClaudeCode, "claude-opus-4", 12.34, 42);
        add_model(&mut s, Platform::Codex, "gpt-5.4", 3.21, 17);

        // Seed daily so the heatmap has non-empty cells.
        let today = CompactDate::from_datetime(chrono::Utc::now());
        s.platform_mut(Platform::ClaudeCode).daily.insert(
            today,
            DayTotals {
                cost_usd: 12.34,
                tokens: 9_600_000,
                calls: 42,
            },
        );

        let mk = |session: &str, model: &str, input: u64, output: u64| UsageRecord {
            session: crate::state::intern(session),
            id: crate::state::record_id(&format!("{session}:{model}:{input}:{output}")),
            input_tokens: input,
            output_tokens: output,
            session_title: crate::state::intern("demo title"),
            project: crate::state::intern("demo"),
            ..crate::state::test_record(model)
        };
        // Use add_records so session aggregates populate.
        s.add_records(
            Platform::ClaudeCode,
            vec![
                mk("ollama-monitor a3f2c1d8", "claude-opus-4", 1200, 340),
                mk("my-web-app 1c2d3e4f", "claude-opus-4", 5000, 600),
            ],
        );
        s
    }

    #[test]
    fn renders_all_platform_sections_stacked() {
        let out = dump(80, 40, sample_state());
        println!("\n{out}");
        let claude = out.find("CLAUDE").unwrap();
        let codex = out.find("CODEX").unwrap();
        assert!(claude < codex);
        assert!(!out.contains("CURSOR"));
        assert!(!out.contains("GROK"));
        assert!(out.contains("82%"), "claude quota window");
        assert!(out.contains("claude-opus-4"));
        assert!(out.contains("gpt-5.4"));
        assert!(out.contains("you@mail.com"));
        assert!(!out.contains(" calls"), "header must not show call totals");
        assert!(
            out.contains("$12.34") || out.contains("12.34"),
            "cost visible"
        );
        // Split columns
        assert!(out.contains("IN"));
        assert!(out.contains("OUT"));
        assert!(out.contains("CACHE"));
    }

    #[test]
    fn renders_on_a_very_short_terminal_without_panicking() {
        for h in 3..=7 {
            let _ = dump(80, h, sample_state());
        }
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
