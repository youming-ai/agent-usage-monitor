use crate::state::Tab;
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Gauge},
};

pub fn progress_bar(active_tab: Tab, remaining_percent: Option<f64>) -> Gauge<'static> {
    let label = active_tab.label();
    let icon = active_tab.icon();
    let primary_color = active_tab.primary_color();
    let percent = remaining_percent.unwrap_or(0.0);
    let percent_display = (percent * 100.0).round() as u16;

    // Use platform-specific color for the gauge
    let gauge_color = primary_color;

    // Status indicator based on remaining percentage
    let status_icon = if percent >= 0.5 {
        "✓"
    } else if percent >= 0.2 {
        "⚠"
    } else {
        "✗"
    };

    let status_text = if remaining_percent.is_some() {
        format!("{icon} {} {} {}% REMAINING", label, status_icon, percent_display)
    } else {
        format!("{icon} {}: NO QUOTA DATA", label)
    };

    Gauge::default()
        .block(
            Block::default()
                .title(" USAGE PROGRESS ")
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
        .percent(percent_display)
        .label(Span::styled(status_text, Style::default().fg(Color::White)))
}
