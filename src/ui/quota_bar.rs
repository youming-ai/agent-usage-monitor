use crate::quota::QuotaInfo;
use crate::state::Tab;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn quota_bar(active_tab: Tab, quota: Option<&QuotaInfo>) -> Paragraph<'static> {
    let label = active_tab.label();

    let (status_text, _status_color) = match quota {
        Some(quota) => {
            let mut parts = Vec::new();

            // Add email/account info
            if let Some(email) = &quota.email {
                parts.push(Span::styled(
                    format!("✓ {email}"),
                    Style::default().fg(Color::Green),
                ));
            }

            // Add quota windows
            for window in &quota.windows {
                let percent = window
                    .remaining_percent
                    .map(|p| format!("{}%", (p * 100.0).round() as u64))
                    .unwrap_or_else(|| "?%".to_string());

                let reset = window
                    .reset_in
                    .as_deref()
                    .map(|r| format!(" · reset {r}"))
                    .unwrap_or_default();

                parts.push(Span::raw(format!(
                    "{} remain {percent}{reset}",
                    window.label
                )));
            }

            if let Some(err) = &quota.error {
                parts.push(Span::styled(
                    format!("✗ {err}"),
                    Style::default().fg(Color::Red),
                ));
            }

            if parts.is_empty() {
                (
                    vec![Span::raw("No quota data")],
                    Color::DarkGray,
                )
            } else {
                (parts, Color::Cyan)
            }
        }
        None => (
            vec![Span::styled(
                "Loading quota...",
                Style::default().fg(Color::DarkGray),
            )],
            Color::DarkGray,
        ),
    };

    let mut spans = vec![
        Span::styled(format!("  {label}  "), Style::default().fg(Color::Yellow)),
        Span::raw("│ "),
    ];
    spans.extend(status_text);

    let line = Line::from(spans);
    Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Quota "),
    )
}
