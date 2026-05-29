use crate::state::SessionSummary;
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

pub fn session_table(sessions: &[SessionSummary], total_calls: usize) -> Table<'static> {
    let rows: Vec<Row> = sessions
        .iter()
        .map(|s| {
            // Add visual indicator for cost level
            let cost_indicator = if s.total_cost >= 10.0 {
                "●●●"
            } else if s.total_cost >= 1.0 {
                "●●○"
            } else if s.total_cost >= 0.1 {
                "●○○"
            } else {
                "○○○"
            };

            Row::new(vec![
                Cell::from(s.model.clone()),
                Cell::from(format_tokens(s.total_input)),
                Cell::from(format_tokens(s.total_output)),
                Cell::from(format_tokens(s.total_cache_read)),
                Cell::from(format!("${:.2}", s.total_cost)),
                Cell::from(cost_indicator),
                Cell::from(s.request_count.to_string()),
            ])
        })
        .collect();

    let header = Row::new(vec!["Model", "Input", "Output", "Cache", "Cost", "Level", "#"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(14),
            Constraint::Percentage(14),
            Constraint::Percentage(14),
            Constraint::Percentage(12),
            Constraint::Percentage(10),
            Constraint::Percentage(11),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!(" Sessions ({}) ", total_calls))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
