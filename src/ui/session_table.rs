use super::util::format_age;
use crate::state::{SessionTotals, resolve};
use ratatui::{
    layout::Constraint,
    style::{Modifier, Style},
    widgets::{Cell, Row, Table},
};

/// Top sessions by cost. Columns: SESSION · TITLE · COST · AGE.
pub fn session_table(entries: &[&SessionTotals]) -> Table<'static> {
    let rows: Vec<Row> = entries
        .iter()
        .map(|e| {
            let title = resolve(e.title);
            let title = if title.is_empty() { "—" } else { title };
            let title = truncate(title, 28);
            Row::new(vec![
                Cell::from(truncate(resolve(e.session), 22)),
                Cell::from(title),
                Cell::from(format!("${:.2}", e.cost_usd)),
                Cell::from(format_age(e.last_ts)),
            ])
        })
        .collect();

    let header = Row::new(vec!["SESSION", "TITLE", "COST", "AGE"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    Table::new(
        rows,
        [
            Constraint::Percentage(32),
            Constraint::Percentage(40),
            Constraint::Percentage(14),
            Constraint::Percentage(14),
        ],
    )
    .header(header)
}

fn truncate(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let take = max_chars.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}
