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
    let icon = active_tab.icon();
    let primary_color = active_tab.primary_color();

    let error_span = if let Some(err) = last_error.as_ref() {
        Span::styled(
            format!(" │ ✗ {}", err),
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
            format!(" {icon} {} ", tab_label),
            Style::default()
                .fg(Color::Black)
                .bg(primary_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} CALLS", total_calls),
            Style::default().fg(Color::White),
        ),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("${:.2}", total_cost),
            Style::default().fg(cost_color).add_modifier(Modifier::BOLD),
        ),
        error_span,
        Span::raw("  "),
        Span::styled(
            "TAB:SWITCH │ R:CLEAR │ Q:QUIT",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" STATUS "),
    )
}
