mod model_table;
mod quota_bar;
mod session_table;
mod status_bar;
mod tabs;
mod util;

use crate::state::AppState;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    widgets::{Block, Borders},
};
use std::sync::{Arc, RwLock};

pub fn render(frame: &mut Frame, app_state: &Arc<RwLock<AppState>>) {
    let state = match app_state.try_read() {
        Ok(state) => state,
        Err(_) => return,
    };
    let active = state.active_tab;
    let accent = active.primary_color();

    let quota_h = if active.has_quota_api() { 1 } else { 0 };
    let chunks = Layout::vertical([
        Constraint::Length(2),       // header: tabs + account, with bottom rule
        Constraint::Length(quota_h), // quota summary (single line)
        Constraint::Min(6),          // session table
        Constraint::Length(1),       // status line
    ])
    .split(frame.area());

    // Active-tab data — single array lookup instead of a 13-way match.
    let p = state.platform(active);
    let (quota, sessions, _records, total_calls, total_cost) = p.refs();
    let entries = p.session_entries();

    // Header: a bottom rule in the accent color, with tabs left and the
    // account right-aligned on the row above it.
    let header_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(accent));
    let header_inner = header_block.inner(chunks[0]);
    frame.render_widget(header_block, chunks[0]);

    let email = quota.and_then(|q| q.email.as_deref());
    let acct_w = email.map(|e| e.chars().count() as u16 + 4).unwrap_or(15);
    let header_cols =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(acct_w)]).split(header_inner);
    frame.render_widget(
        tabs::tab_line(active, &state.available_tabs),
        header_cols[0],
    );
    frame.render_widget(tabs::account(email), header_cols[1]);

    // Only Claude and Codex have quota API backends; hide the quota row
    // entirely for platforms that never report quota so the session table
    // gets the extra line.
    if active.has_quota_api() {
        frame.render_widget(quota_bar::quota_panel(active, quota), chunks[1]);
    }

    // Split the main area: the per-model table sized to its rows on top, with
    // the per-session usage table filling the remaining space below. Compute
    // the upper bound with `.min().max(3)` rather than `clamp(3, upper)`: on a
    // very short terminal `upper` drops below 3, and `clamp` panics when
    // min > max (the layout tolerates an over-tall length, a panic doesn't).
    let models_h = (sessions.len() as u16 + 3)
        .min(chunks[2].height.saturating_sub(3))
        .max(3);
    let main =
        Layout::vertical([Constraint::Length(models_h), Constraint::Min(3)]).split(chunks[2]);
    frame.render_widget(
        model_table::model_table(active, sessions, total_calls),
        main[0],
    );
    // Derive the selected row index each render (selection is tracked by the
    // stable per-row key, so a live re-sort can't strand the highlight on the
    // wrong row). TableState scrolls it into view even when the list overflows.
    let selected_idx = state
        .selected_key
        .and_then(|key| entries.iter().position(|e| e.key == key));
    let mut table_state = ratatui::widgets::TableState::default();
    table_state.select(selected_idx);
    frame.render_stateful_widget(
        session_table::session_table(&entries, state.sessions_focused, accent),
        main[1],
        &mut table_state,
    );

    frame.render_widget(status_bar::status_bar(total_calls, total_cost), chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{QuotaInfo, QuotaWindow};
    use crate::state::{Platform, SessionSummary, Tab, UsageRecord};
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
        let claude = s.platform_mut(Tab::ClaudeCode);
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
        claude.sessions = vec![SessionSummary {
            model: crate::state::intern("claude-opus-4"),
            total_input: 1_200_000,
            total_output: 340_000,
            total_cache_read: 8_100_000,
            total_cache_creation: 0,
            total_cost: 12.34,
            request_count: 42,
        }]
        .into_iter()
        .map(|m| (m.model, m))
        .collect();
        let mk = |session: &str, model: &str, input: u64, output: u64| UsageRecord {
            session: crate::state::intern(session),
            session_id: crate::state::intern(session),
            id: crate::state::intern(&format!("{session}:{model}:{input}:{output}")),
            input_tokens: input,
            output_tokens: output,
            ..crate::state::test_record(Platform::ClaudeCode, model)
        };
        claude.records = vec![
            mk("ollama-monitor a3f2c1d8", "claude-opus-4", 1200, 340),
            mk("ollama-monitor a3f2c1d8", "claude-opus-4", 8400, 512),
            mk("ollama-monitor 9b4e7f02", "claude-sonnet-4", 2100, 180),
            mk("my-web-app 1c2d3e4f", "claude-opus-4", 5000, 600),
        ]
        .into();
        claude.total_calls = 42;
        claude.total_cost = 12.34;
        s
    }

    #[test]
    fn renders_without_panicking() {
        let out = dump(80, 18, sample_state());
        println!("\n{out}");
        assert!(out.contains("CLAUDE"));
        assert!(out.contains("82%"));
        assert!(out.contains("claude-opus-4"));
        assert!(out.contains("ollama-monitor"));
        assert!(out.contains("sessions"));
        assert!(out.contains("42 calls"));
    }

    #[test]
    fn renders_on_a_very_short_terminal_without_panicking() {
        // Height 5 makes the main area's height < 6, which used to drive the
        // models-height clamp's upper bound below its lower bound and panic.
        for h in 3..=7 {
            let _ = dump(80, h, sample_state());
        }
    }

    #[test]
    fn selected_session_below_the_fold_is_scrolled_into_view() {
        let mut s = AppState::with_capacity(100);
        let mk = |session: &str| UsageRecord {
            session: crate::state::intern(session),
            session_id: crate::state::intern(session),
            id: crate::state::intern(session),
            input_tokens: 100,
            ..crate::state::test_record(Platform::ClaudeCode, "claude-opus-4")
        };
        // 30 sessions — far more than a short panel can show at once.
        let records: Vec<_> = (0..30).map(|i| mk(&format!("sess-{i:02}"))).collect();
        s.add_records(Platform::ClaudeCode, records);
        // Select and focus the last row (all equal usage → sorted by label).
        s.selected_key = Some(crate::state::intern("sess-29"));
        s.sessions_focused = true;

        let out = dump(80, 16, s);
        assert!(
            out.contains("sess-29"),
            "selection past the fold must scroll into view:\n{out}"
        );
    }
}
