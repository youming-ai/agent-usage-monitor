use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Bottom status line (no border): ` N calls · $X.XX`.
pub fn status_bar(total_calls: usize, total_cost: f64) -> Paragraph<'static> {
    let line = Line::from(vec![
        Span::raw(format!(" {total_calls} calls")),
        Span::styled(" · ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("${total_cost:.2}")),
    ]);
    Paragraph::new(line).alignment(Alignment::Left)
}
