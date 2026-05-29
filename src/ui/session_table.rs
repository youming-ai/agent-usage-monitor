use crate::state::{SessionSummary, Tab};
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

pub fn session_table(active_tab: Tab, sessions: &[SessionSummary], total_calls: usize) -> Table<'static> {
    let icon = active_tab.icon();
    let label = active_tab.label();

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

            // Color cost indicator based on level
            let indicator_color = if s.total_cost >= 10.0 {
                Color::Red
            } else if s.total_cost >= 1.0 {
                Color::Yellow
            } else if s.total_cost >= 0.1 {
                Color::Green
            } else {
                Color::DarkGray
            };

            Row::new(vec![
                Cell::from(s.model.clone()),
                Cell::from(format_tokens(s.total_input)),
                Cell::from(format_tokens(s.total_output)),
                Cell::from(format_tokens(s.total_cache_read)),
                Cell::from(format!("${:.2}", s.total_cost)),
                Cell::from(cost_indicator).style(Style::default().fg(indicator_color)),
                Cell::from(s.request_count.to_string()),
            ])
        })
        .collect();

    let header = Row::new(vec!["MODEL", "INPUT", "OUTPUT", "CACHE", "COST", "LEVEL", "#"])
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
            .title(format!(" {icon} {label} SESSIONS ({}) ", total_calls))
            .borders(Borders::ALL),
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
