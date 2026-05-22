use crate::state::AppState;
use humansize::{format_size, BINARY};
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};
use std::sync::{Arc, RwLock};

pub fn model_table_widget(app_state: &Arc<RwLock<AppState>>) -> Table<'static> {
    let state = app_state.read().unwrap();
    let rows: Vec<Row> = state
        .running_models
        .iter()
        .map(|m| {
            Row::new(vec![
                Cell::from(m.name.clone()),
                Cell::from(m.running_for.clone()),
                Cell::from(format_size(m.size, BINARY)),
                Cell::from(
                    m.vram
                        .map(|v| format_size(v, BINARY))
                        .unwrap_or_else(|| "N/A".to_string()),
                ),
            ])
        })
        .collect();

    let header = Row::new(vec!["Model", "Running", "Memory", "VRAM"])
        .style(Style::default().fg(Color::Yellow));

    Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
        ],
    )
    .header(header)
    .block(Block::default().title("Running Models").borders(Borders::ALL))
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}