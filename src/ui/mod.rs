mod quota_bar;
mod session_table;
mod status_bar;
mod tabs;
mod usage_table;

use crate::state::{AppState, MAX_RECORDS};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use std::sync::{Arc, RwLock};

pub fn render(frame: &mut Frame, app_state: &Arc<RwLock<AppState>>) {
    let state = app_state.read().unwrap();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tabs
            Constraint::Length(3),  // Quota bar
            Constraint::Percentage(35),  // Session table
            Constraint::Percentage(50),  // Usage table
            Constraint::Min(1),    // Status bar
        ])
        .split(frame.area());

    frame.render_widget(tabs::tab_bar(state.active_tab), chunks[0]);

    // Render quota bar based on active tab
    let quota = match state.active_tab {
        crate::state::Tab::ClaudeCode => state.claude_quota.as_ref(),
        crate::state::Tab::Codex => state.codex_quota.as_ref(),
    };
    frame.render_widget(quota_bar::quota_bar(state.active_tab, quota), chunks[1]);

    let (sessions, records, total_calls, total_cost) = match state.active_tab {
        crate::state::Tab::ClaudeCode => (
            &state.claude_sessions,
            &state.claude_records,
            state.claude_total_calls,
            state.claude_total_cost,
        ),
        crate::state::Tab::Codex => (
            &state.codex_sessions,
            &state.codex_records,
            state.codex_total_calls,
            state.codex_total_cost,
        ),
    };

    frame.render_widget(session_table::session_table(sessions, total_calls), chunks[2]);
    frame.render_widget(usage_table::usage_table(records, MAX_RECORDS), chunks[3]);
    frame.render_widget(
        status_bar::status_bar(state.active_tab, total_calls, total_cost, &state.last_error),
        chunks[4],
    );
}
