use super::util::format_tokens;
use crate::state::SessionEntry;
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

/// Per-session usage table. `entries` is already sorted (highest usage first).
/// The selected row is highlighted via the Table's `row_highlight_style` and
/// scrolled into view by the caller's `TableState` — brighter when the panel
/// has keyboard focus. The border and title also change when focused so it's
/// clear the arrow keys now drive the list rather than the tabs.
pub fn session_table(entries: &[SessionEntry], focused: bool, accent: Color) -> Table<'static> {
    let rows: Vec<Row> = entries
        .iter()
        .map(|e| {
            Row::new(vec![
                Cell::from(e.label),
                Cell::from(format_tokens(e.tokens)),
                Cell::from(e.requests.to_string()),
            ])
        })
        .collect();

    let header = Row::new(vec!["SESSION", "TOKENS", "REQUESTS"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let title = if focused {
        " sessions · ↑↓ select · enter resume · esc back "
    } else {
        " sessions "
    };
    let border = if focused { accent } else { Color::DarkGray };
    let highlight = if focused {
        Style::default().fg(Color::Black).bg(accent)
    } else {
        // Remembered-but-unfocused selection: visible, but clearly not active.
        Style::default().add_modifier(Modifier::REVERSED | Modifier::DIM)
    };

    Table::new(
        rows,
        [
            Constraint::Percentage(56),
            Constraint::Percentage(22),
            Constraint::Percentage(22),
        ],
    )
    .header(header)
    .row_highlight_style(highlight)
    .block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border)),
    )
}
