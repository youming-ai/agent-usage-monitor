mod model_table;
mod quota_bar;
mod session_table;
mod status_bar;
mod tabs;
mod util;

use crate::state::AppState;
use ratatui::{
    layout::{Constraint, Layout},
    style::Style,
    widgets::{Block, Borders},
    Frame,
};
use std::sync::{Arc, RwLock};

pub fn render(frame: &mut Frame, app_state: &Arc<RwLock<AppState>>) {
    let state = match app_state.try_read() {
        Ok(state) => state,
        Err(_) => return,
    };
    let active = state.active_tab;
    let accent = active.primary_color();

    let chunks = Layout::vertical([
        Constraint::Length(2), // header: tabs + account, with bottom rule
        Constraint::Length(2), // quota windows (one bar per line)
        Constraint::Min(6),    // session table
        Constraint::Length(1), // status line
    ])
    .split(frame.area());

    // Active-tab data.
    let (quota, sessions, records, total_calls, total_cost) = match active {
        crate::state::Tab::ClaudeCode => (
            state.claude_quota.as_ref(),
            &state.claude_sessions,
            &state.claude_records,
            state.claude_total_calls,
            state.claude_total_cost,
        ),
        crate::state::Tab::Codex => (
            state.codex_quota.as_ref(),
            &state.codex_sessions,
            &state.codex_records,
            state.codex_total_calls,
            state.codex_total_cost,
        ),
        crate::state::Tab::OpenCode => (
            state.opencode_quota.as_ref(),
            &state.opencode_sessions,
            &state.opencode_records,
            state.opencode_total_calls,
            state.opencode_total_cost,
        ),
        crate::state::Tab::KimiCode => (
            state.kimi_code_quota.as_ref(),
            &state.kimi_code_sessions,
            &state.kimi_code_records,
            state.kimi_code_total_calls,
            state.kimi_code_total_cost,
        ),
        crate::state::Tab::Pi => (
            state.pi_quota.as_ref(),
            &state.pi_sessions,
            &state.pi_records,
            state.pi_total_calls,
            state.pi_total_cost,
        ),
        crate::state::Tab::OpenClaw => (
            state.openclaw_quota.as_ref(),
            &state.openclaw_sessions,
            &state.openclaw_records,
            state.openclaw_total_calls,
            state.openclaw_total_cost,
        ),
        crate::state::Tab::Hermes => (
            state.hermes_quota.as_ref(),
            &state.hermes_sessions,
            &state.hermes_records,
            state.hermes_total_calls,
            state.hermes_total_cost,
        ),
        crate::state::Tab::Factory => (
            state.factory_quota.as_ref(),
            &state.factory_sessions,
            &state.factory_records,
            state.factory_total_calls,
            state.factory_total_cost,
        ),
        crate::state::Tab::Grok => (
            state.grok_quota.as_ref(),
            &state.grok_sessions,
            &state.grok_records,
            state.grok_total_calls,
            state.grok_total_cost,
        ),
        crate::state::Tab::Cursor => (
            state.cursor_quota.as_ref(),
            &state.cursor_sessions,
            &state.cursor_records,
            state.cursor_total_calls,
            state.cursor_total_cost,
        ),
        crate::state::Tab::Copilot => (
            state.copilot_quota.as_ref(),
            &state.copilot_sessions,
            &state.copilot_records,
            state.copilot_total_calls,
            state.copilot_total_cost,
        ),
        crate::state::Tab::Antigravity => (
            state.antigravity_quota.as_ref(),
            &state.antigravity_sessions,
            &state.antigravity_records,
            state.antigravity_total_calls,
            state.antigravity_total_cost,
        ),
    };

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
    frame.render_widget(tabs::tab_line(active, &state.available_tabs), header_cols[0]);
    frame.render_widget(tabs::account(email), header_cols[1]);

    let quota_widget = match active {
        crate::state::Tab::OpenCode
        | crate::state::Tab::KimiCode
        | crate::state::Tab::Pi
        | crate::state::Tab::OpenClaw
        | crate::state::Tab::Hermes
        | crate::state::Tab::Factory
        | crate::state::Tab::Grok
        | crate::state::Tab::Cursor
        | crate::state::Tab::Copilot
        | crate::state::Tab::Antigravity => quota_bar::no_quota_source(),
        _ => quota_bar::quota_panel(active, quota),
    };
    frame.render_widget(quota_widget, chunks[1]);

    // Split the main area: the per-model table sized to its rows on top, with
    // the per-session usage table filling the remaining space below.
    let models_h = (sessions.len() as u16 + 3).clamp(3, chunks[2].height.saturating_sub(3));
    let main = Layout::vertical([Constraint::Length(models_h), Constraint::Min(3)]).split(chunks[2]);
    frame.render_widget(
        model_table::model_table(active, sessions, total_calls),
        main[0],
    );
    frame.render_widget(session_table::session_table(records), main[1]);

    frame.render_widget(status_bar::status_bar(total_calls, total_cost), chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{QuotaInfo, QuotaWindow};
    use crate::state::{Platform, SessionSummary, UsageRecord};
    use chrono::Utc;
    use ratatui::{backend::TestBackend, Terminal};
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
        s.claude_quota = Some(QuotaInfo {
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
        s.claude_sessions = vec![SessionSummary {
            model: "claude-opus-4".into(),
            total_input: 1_200_000,
            total_output: 340_000,
            total_cache_read: 8_100_000,
            total_cache_creation: 0,
            total_cost: 12.34,
            request_count: 42,
        }].into_iter().map(|m| (m.model.clone(), m)).collect();
        let mk = |session: &str, model: &str, input: u64, output: u64| UsageRecord {
            timestamp: Utc::now(),
            platform: Platform::ClaudeCode,
            model: model.into(),
            session: session.into(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: 0.0,
        };
        s.claude_records = vec![
            // Same project, two distinct conversations + a third project.
            mk("ollama-monitor a3f2c1d8", "claude-opus-4", 1200, 340),
            mk("ollama-monitor a3f2c1d8", "claude-opus-4", 8400, 512),
            mk("ollama-monitor 9b4e7f02", "claude-sonnet-4", 2100, 180),
            mk("my-web-app 1c2d3e4f", "claude-opus-4", 5000, 600),
        ].into();
        s.claude_total_calls = 42;
        s.claude_total_cost = 12.34;
        s
    }

    #[test]
    fn renders_without_panicking() {
        let out = dump(80, 18, sample_state());
        // Print for manual inspection with `--nocapture`.
        println!("\n{out}");
        // Header tab, a quota bar, both tables, and the status line are present.
        assert!(out.contains("CLAUDE"));
        assert!(out.contains("82%"));
        assert!(out.contains("claude-opus-4")); // model table
        assert!(out.contains("ollama-monitor")); // session table
        assert!(out.contains("sessions"));
        assert!(out.contains("42 calls"));
    }

    #[test]
    fn renders_opencode_tab_empty_with_no_quota() {
        let mut s = sample_state();
        s.active_tab = crate::state::Tab::OpenCode;
        let out = dump(80, 18, s);
        assert!(out.contains("OPENCODE")); // third tab is present
        assert!(out.contains("no quota data")); // opencode has no quota source
    }
}
