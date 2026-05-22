use crate::state::AppState;
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};
use std::sync::{Arc, RwLock};

pub fn usage_table_widget(app_state: &Arc<RwLock<AppState>>) -> Table<'static> {
    let state = app_state.read().unwrap_or_else(|e| e.into_inner());
    let rows: Vec<Row> = state
        .recent_calls
        .iter()
        .rev()
        .map(|c| {
            Row::new(vec![
                Cell::from(c.timestamp.format("%H:%M:%S").to_string()),
                Cell::from(c.model.clone()),
                Cell::from(c.prompt_tokens.to_string()),
                Cell::from(c.completion_tokens.to_string()),
                Cell::from(format!("{}ms", c.total_duration_ms)),
                Cell::from(format!("{:.1}", c.tokens_per_sec)),
            ])
        })
        .collect();

    let header = Row::new(vec!["Time", "Model", "In", "Out", "Total", "T/s"])
        .style(Style::default().fg(Color::Yellow));

    Table::new(
        rows,
        [
            Constraint::Percentage(15),
            Constraint::Percentage(30),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(16),
            Constraint::Percentage(15),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!("Recent API Calls ({}/{}) ", state.recent_calls.len(), crate::state::MAX_RECENT_CALLS))
            .borders(Borders::ALL),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}
