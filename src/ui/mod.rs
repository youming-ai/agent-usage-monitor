mod heatmap;
mod quota_bar;
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

pub fn render(frame: &mut Frame, app_state: &Arc<RwLock<AppState>>) {
    let state = match app_state.try_read() {
        Ok(state) => state,
        Err(_) => return,
    };

    // Every platform whose data directory exists, stacked top to bottom in
    // registry order. Each section is header + optional quota + heatmap.
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
    let (quota, _, _, _) = p.refs();
    let accent = platform.primary_color();

    let has_quota = crate::platforms::entry_for_platform(platform).has_quota();
    let h = area.height;

    let quota_h: u16 = if has_quota && h >= 4 { 1 } else { 0 };
    let header_h: u16 = if h >= 2 { 2 } else { 1 };
    let remain = h.saturating_sub(header_h + quota_h);

    // Full 7-row graph when room; otherwise a 1-row strip; drop if tiny.
    let heatmap_h: u16 = if remain >= 8 {
        8 // month/gutter room + 7 day rows (widget clips to height)
    } else if remain >= 1 {
        remain.clamp(1, 7)
    } else {
        0
    };

    let chunks = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(quota_h),
        Constraint::Min(heatmap_h.max(1)),
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

    let heat_area = chunks[2];
    if heat_area.height >= 7 {
        let weeks = heat_area.width.saturating_sub(2).min(52);
        frame.render_widget(heatmap::contribution_heatmap(&p.daily, weeks), heat_area);
    } else if heat_area.height >= 1 && heat_area.width > 0 {
        frame.render_widget(
            heatmap::contribution_strip(&p.daily, heat_area.width),
            heat_area,
        );
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

        let today = CompactDate::from_datetime(chrono::Utc::now());
        s.platform_mut(Platform::ClaudeCode).daily.insert(
            today,
            DayTotals {
                cost_usd: 12.34,
                tokens: 9_600_000,
                calls: 42,
            },
        );
        s.platform_mut(Platform::Codex).daily.insert(
            today,
            DayTotals {
                cost_usd: 3.21,
                tokens: 1_000_000,
                calls: 10,
            },
        );

        // Still ingest records so aggregates stay exercised for non-UI paths.
        let mk = |session: &str, model: &str, input: u64, output: u64| UsageRecord {
            session: crate::state::intern(session),
            id: crate::state::record_id(&format!("{session}:{model}:{input}:{output}")),
            input_tokens: input,
            output_tokens: output,
            ..crate::state::test_record(model)
        };
        s.add_records(
            Platform::ClaudeCode,
            vec![mk("demo a3f2c1d8", "claude-opus-4", 1200, 340)],
        );
        s
    }

    #[test]
    fn renders_platform_heatmaps_only() {
        let out = dump(80, 24, sample_state());
        println!("\n{out}");
        let claude = out.find("CLAUDE").unwrap();
        let codex = out.find("CODEX").unwrap();
        assert!(claude < codex);
        assert!(out.contains("82%"), "claude quota window");
        assert!(out.contains("you@mail.com"));
        // Model / session tables removed from the TUI.
        assert!(!out.contains("MODEL"));
        assert!(!out.contains("SESSION"));
        assert!(!out.contains("claude-opus-4"));
        // Heatmap cells use ■
        assert!(out.contains('■'), "heatmap should paint cells");
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
