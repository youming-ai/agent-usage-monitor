use crate::state::Tab;
use ratatui::{
    style::{Color, Modifier, Style},
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
        Span::styled(
            format!(" │ ERROR: {}", err),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    // Format cost with color based on amount
    let cost_color = if total_cost >= 10.0 {
        Color::Red
    } else if total_cost >= 1.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    let line = Line::from(vec![
        Span::styled(
            format!("  {}  ", tab_label),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(100, 100, 100))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} calls", total_calls),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled(
            format!("${:.2}", total_cost),
            Style::default().fg(cost_color).add_modifier(Modifier::BOLD),
        ),
        error_span,
        Span::raw("  "),
        Span::styled(
            "Tab:switch │ r:clear │ q:quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Status "),
    )
}
