use super::util::format_tokens;
use crate::state::{InternedString, ModelTotals, Platform};
use ratatui::{
    layout::Constraint,
    style::{Modifier, Style},
    widgets::{Cell, Row, Table},
};
use std::collections::HashMap;

/// Per-model totals (input/output/cache tokens, cost, request count).
/// Takes a map keyed by model name; rows are sorted by cost descending (then
/// model name) so the order is stable across frames and process runs — a bare
/// `HashMap` iteration would reshuffle on rehash / key removal.
pub fn model_table(
    models: &HashMap<InternedString, ModelTotals>,
    _platform: Platform,
) -> Table<'static> {
    let mut models: Vec<&ModelTotals> = models.values().collect();
    models.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| crate::state::resolve(a.model).cmp(crate::state::resolve(b.model)))
    });

    let rows: Vec<Row> = models
        .into_iter()
        .map(|s| {
            let out = s.total_output + s.total_reasoning;
            let cache = s.total_cache_read + s.total_cache_creation;
            Row::new(vec![
                Cell::from(crate::state::resolve(s.model)),
                Cell::from(format_tokens(s.total_input)),
                Cell::from(format_tokens(out)),
                Cell::from(format_tokens(cache)),
                Cell::from(format!("${:.2}", s.total_cost)),
                Cell::from(s.request_count.to_string()),
            ])
        })
        .collect();

    let header = Row::new(vec!["MODEL", "IN", "OUT", "CACHE", "COST", "#"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    Table::new(
        rows,
        [
            Constraint::Percentage(34),
            Constraint::Percentage(14),
            Constraint::Percentage(14),
            Constraint::Percentage(14),
            Constraint::Percentage(14),
            Constraint::Percentage(10),
        ],
    )
    .header(header)
}
