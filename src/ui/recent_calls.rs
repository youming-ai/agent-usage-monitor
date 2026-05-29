use super::util::{format_cost, format_tokens};
use crate::state::{Tab, UsageRecord};
use chrono::Local;
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

/// Real-time feed of individual API calls, newest first.
pub fn recent_calls(active_tab: Tab, records: &[UsageRecord]) -> Table<'static> {
    let icon = active_tab.icon();

    let rows: Vec<Row> = records
        .iter()
        .rev()
        .map(|r| {
            let time = r.timestamp.with_timezone(&Local).format("%H:%M:%S").to_string();
            Row::new(vec![
                Cell::from(time).style(Style::default().fg(Color::DarkGray)),
                Cell::from(r.model.clone()),
                Cell::from(format_tokens(r.input_tokens)),
                Cell::from(format_tokens(r.output_tokens)),
                Cell::from(format_cost(r.cost_usd)),
            ])
        })
        .collect();

    let header = Row::new(vec!["TIME", "MODEL", "IN", "OUT", "COST"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    Table::new(
        rows,
        [
            Constraint::Length(9),
            Constraint::Percentage(40),
            Constraint::Percentage(18),
            Constraint::Percentage(18),
            Constraint::Percentage(18),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!(" {icon} recent calls "))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
}
