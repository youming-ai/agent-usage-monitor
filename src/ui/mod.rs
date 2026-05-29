mod progress_bar;
mod quota_bar;
mod session_table;
mod status_bar;
mod tabs;

use crate::state::AppState;
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
            Constraint::Length(3),  // Progress bar
            Constraint::Min(10),   // Session table (flexible)
            Constraint::Length(3),  // Status bar
        ])
        .split(frame.area());

    frame.render_widget(tabs::tab_bar(state.active_tab), chunks[0]);

    // Get active tab data
    let (quota, sessions, total_calls, total_cost) = match state.active_tab {
        crate::state::Tab::ClaudeCode => (
            state.claude_quota.as_ref(),
            &state.claude_sessions,
            state.claude_total_calls,
            state.claude_total_cost,
        ),
        crate::state::Tab::Codex => (
            state.codex_quota.as_ref(),
            &state.codex_sessions,
            state.codex_total_calls,
            state.codex_total_cost,
        ),
    };

    // Render quota bar
    frame.render_widget(quota_bar::quota_bar(state.active_tab, quota), chunks[1]);

    // Render progress bar based on quota
    let remaining_percent = quota
        .and_then(|q| q.windows.first())
        .and_then(|w| w.remaining_percent);
    frame.render_widget(
        progress_bar::progress_bar(state.active_tab, remaining_percent),
        chunks[2],
    );

    // Render session table
    frame.render_widget(session_table::session_table(sessions, total_calls), chunks[3]);

    // Render status bar
    frame.render_widget(
        status_bar::status_bar(state.active_tab, total_calls, total_cost, &state.last_error),
        chunks[4],
    );
}
