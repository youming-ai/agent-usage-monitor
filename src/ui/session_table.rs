use super::util::format_tokens;
use crate::state::{SessionSummary, Tab};
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

pub fn session_table(
    active_tab: Tab,
    sessions: &[SessionSummary],
    total_calls: usize,
) -> Table<'static> {
    let icon = active_tab.icon();
    let label = active_tab.label();

    let rows: Vec<Row> = sessions
        .iter()
        .map(|s| {
            Row::new(vec![
                Cell::from(s.model.clone()),
                Cell::from(format_tokens(s.total_input)),
                Cell::from(format_tokens(s.total_output)),
                Cell::from(format_tokens(s.total_cache_read)),
                Cell::from(format!("${:.2}", s.total_cost)),
                Cell::from(s.request_count.to_string()),
            ])
        })
        .collect();

    let header = Row::new(vec!["MODEL", "INPUT", "OUTPUT", "CACHE", "COST", "#"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    Table::new(
        rows,
        [
            Constraint::Percentage(30),
            Constraint::Percentage(16),
            Constraint::Percentage(16),
            Constraint::Percentage(16),
            Constraint::Percentage(14),
            Constraint::Percentage(8),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!(" {icon} {label} sessions ({total_calls}) "))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}
