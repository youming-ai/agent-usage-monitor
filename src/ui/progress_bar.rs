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

    // Color based on remaining percentage
    let gauge_color = if percent >= 0.5 {
        Color::Green
    } else if percent >= 0.2 {
        Color::Yellow
    } else {
        Color::Red
    };

    let status_text = if remaining_percent.is_some() {
        format!("{icon} {}: {}% remaining", label, percent_display)
    } else {
        format!("{icon} {}: No quota data", label)
    };

    Gauge::default()
        .block(
            Block::default()
                .title(" Usage Progress ")
                .title_style(Style::default().fg(primary_color).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(primary_color)),
        )
        .gauge_style(Style::default().fg(gauge_color).bg(Color::DarkGray))
        .percent(percent_display)
        .label(Span::styled(status_text, Style::default().fg(Color::White)))
}
