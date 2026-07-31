use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Bottom status line (no border): ` N calls · $X.XX` plus reader health.
pub fn status_bar(
    total_calls: usize,
    total_cost: f64,
    reader_error: Option<&str>,
) -> Paragraph<'static> {
    let mut spans = vec![
        Span::raw(format!(" {total_calls} calls")),
        Span::styled(" · ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("${total_cost:.2}")),
    ];
    if let Some(error) = reader_error {
        spans.push(Span::styled(
            format!(" · reader error: {error}"),
            Style::default().fg(Color::Red),
        ));
    }
    Paragraph::new(Line::from(spans)).alignment(Alignment::Left)
}
