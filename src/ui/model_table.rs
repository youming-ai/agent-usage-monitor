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
    models: &HashMap<crate::state::InternedString, SessionSummary>,
    total_calls: usize,
) -> Table<'static> {
    let label = active_tab.label();

    let mut models: Vec<&SessionSummary> = models.values().collect();
    models.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| crate::state::resolve(a.model).cmp(crate::state::resolve(b.model)))
    });

    let rows: Vec<Row> = models
        .into_iter()
        .map(|s| {
            let total_tokens = s.total_input + s.total_output + s.total_cache_read;
            Row::new(vec![
                Cell::from(crate::state::resolve(s.model)),
                Cell::from(format_tokens(total_tokens)),
                Cell::from(format!("${:.2}", s.total_cost)),
                Cell::from(s.request_count.to_string()),
            ])
        })
        .collect();

    let header = Row::new(vec!["MODEL", "TOKENS", "COST", "#"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    Table::new(
        rows,
        [
            Constraint::Percentage(42),
            Constraint::Percentage(26),
            Constraint::Percentage(20),
            Constraint::Percentage(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!(" {label} models ({total_calls}) "))
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray))
}
