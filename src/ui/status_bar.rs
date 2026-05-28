use crate::state::Tab;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn status_bar(
    active_tab: Tab,
    total_calls: usize,
    total_cost: f64,
    last_error: &Option<String>,
) -> Paragraph<'static> {
    let tab_label = active_tab.label();

    let error_span = if let Some(err) = last_error.as_ref() {
        Span::styled(format!(" | ERROR: {}", err), Style::default().fg(Color::Red))
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![
        Span::styled(
            format!("[{}]", tab_label),
            Style::default().fg(Color::Green),
        ),
        Span::raw(format!(
            " {} calls | ${:.2} | Tab:switch r:clear q:quit",
            total_calls, total_cost
        )),
        error_span,
    ]);

    Paragraph::new(line).block(Block::default().borders(Borders::TOP))
}
