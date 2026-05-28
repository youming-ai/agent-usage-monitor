use crate::state::UsageRecord;
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

pub fn usage_table(records: &[UsageRecord], max: usize) -> Table<'static> {
    let rows: Vec<Row> = records
        .iter()
        .rev()
        .map(|r| {
            Row::new(vec![
                Cell::from(r.timestamp.format("%H:%M:%S").to_string()),
                Cell::from(r.model.clone()),
                Cell::from(format_tokens(r.input_tokens)),
                Cell::from(format_tokens(r.output_tokens)),
                Cell::from(format!("${:.4}", r.cost_usd)),
            ])
        })
        .collect();

    let header = Row::new(vec!["Time", "Model", "In", "Out", "Cost"])
        .style(Style::default().fg(Color::Yellow));

    Table::new(
        rows,
        [
            Constraint::Percentage(15),
            Constraint::Percentage(35),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
            Constraint::Percentage(16),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!("Recent API Calls ({}/{})", records.len(), max))
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
