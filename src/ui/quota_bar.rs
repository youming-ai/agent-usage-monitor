use crate::quota::QuotaInfo;
use crate::state::Tab;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const SEPARATOR: &str = " │ ";

pub fn quota_bar(active_tab: Tab, quota: Option<&QuotaInfo>) -> Paragraph<'static> {
    let label = active_tab.label();

    let mut spans = vec![
        Span::styled(format!("  {label}  "), Style::default().fg(Color::Yellow)),
        Span::styled(SEPARATOR, Style::default().fg(Color::DarkGray)),
    ];

    match quota {
        Some(quota) => {
            let mut has_content = false;

            // Add email/account info
            if let Some(email) = &quota.email {
                spans.push(Span::styled(
                    format!("✓ {email}"),
                    Style::default().fg(Color::Green),
                ));
                has_content = true;
            }

            // Add quota windows
            for window in &quota.windows {
                if has_content {
                    spans.push(Span::styled(SEPARATOR, Style::default().fg(Color::DarkGray)));
                }

                let percent = window
                    .remaining_percent
                    .map(|p| format!("{}%", (p * 100.0).round() as u64))
                    .unwrap_or_else(|| "?%".to_string());

                // Color based on remaining percentage
                let percent_color = window
                    .remaining_percent
                    .map(|p| {
                        if p >= 0.5 {
                            Color::Green
                        } else if p >= 0.2 {
                            Color::Yellow
                        } else {
                            Color::Red
                        }
                    })
                    .unwrap_or(Color::White);

                let reset = window
                    .reset_in
                    .as_deref()
                    .map(|r| format!(" · reset {r}"))
                    .unwrap_or_default();

                spans.push(Span::styled(
                    format!("{} remain ", window.label),
                    Style::default().fg(Color::White),
                ));
                spans.push(Span::styled(percent, Style::default().fg(percent_color)));
                if !reset.is_empty() {
                    spans.push(Span::styled(reset, Style::default().fg(Color::DarkGray)));
                }

                has_content = true;
            }

            // Add error if present
            if let Some(err) = &quota.error {
                if has_content {
                    spans.push(Span::styled(SEPARATOR, Style::default().fg(Color::DarkGray)));
                }
                spans.push(Span::styled(
                    format!("✗ {err}"),
                    Style::default().fg(Color::Red),
                ));
            }

            if !has_content {
                spans.push(Span::styled(
                    "No quota data",
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
        None => {
            spans.push(Span::styled(
                "Loading quota...",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    let line = Line::from(spans);
    Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Quota "),
    )
}
