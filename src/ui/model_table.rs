use super::util::format_tokens;
use crate::state::{SessionSummary, Tab};
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};
use std::collections::HashMap;

/// Per-model totals (input/output/cache tokens, cost, request count).
/// Takes a map keyed by model name; rows are sorted by cost descending (then
/// model name) so the order is stable across frames and process runs — a bare
/// `HashMap` iteration would reshuffle on rehash / key removal.
pub fn model_table(
    active_tab: Tab,
    models: &HashMap<String, SessionSummary>,
    total_calls: usize,
) -> Table<'static> {
    let label = active_tab.label();

    let mut models: Vec<&SessionSummary> = models.values().collect();
    models.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.model.cmp(&b.model))
    });

    let rows: Vec<Row> = models
        .into_iter()
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
            .title(format!(" {label} models ({total_calls}) "))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}
