use crate::quota::QuotaInfo;
use crate::state::Tab;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const SEPARATOR: &str = " │ ";

pub fn quota_bar(active_tab: Tab, quota: Option<&QuotaInfo>) -> Paragraph<'static> {
    let label = active_tab.label();
    let icon = active_tab.icon();
    let primary_color = active_tab.primary_color();

    let mut spans = vec![
        Span::styled(
            format!(" {icon} {label} "),
            Style::default().fg(Color::Black).bg(primary_color).add_modifier(Modifier::BOLD),
        ),
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

                let reset = window
                    .reset_in
                    .as_deref()
                    .map(|r| format!("reset {r}"))
                    .unwrap_or_default();

                spans.push(Span::styled(
                    format!("{}", window.label),
                    Style::default().fg(Color::White),
                ));
                if !reset.is_empty() {
                    spans.push(Span::raw(" "));
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
            .border_style(Style::default().fg(primary_color))
            .title(" Quota Info "),
    )
}
