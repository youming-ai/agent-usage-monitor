use crate::state::Tab;
use ratatui::{
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Gauge},
};

pub fn progress_bar(active_tab: Tab, remaining_percent: Option<f64>) -> Gauge<'static> {
    let label = active_tab.label();
    let percent = remaining_percent.unwrap_or(0.0);
    let percent_display = (percent * 100.0).round() as u16;

    // Color based on remaining percentage
    let (gauge_color, label_color) = if percent >= 0.5 {
        (Color::Green, Color::Green)
    } else if percent >= 0.2 {
        (Color::Yellow, Color::Yellow)
    } else {
        (Color::Red, Color::Red)
    };

    let status_text = if remaining_percent.is_some() {
        format!("{}: {}% remaining", label, percent_display)
    } else {
        format!("{}: No quota data", label)
    };

    Gauge::default()
        .block(
            Block::default()
                .title(" Usage Progress ")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(gauge_color).bg(Color::DarkGray))
        .percent(percent_display)
        .label(Span::styled(status_text, Style::default().fg(label_color)))
}
