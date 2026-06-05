use crate::state::Tab;
use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// One tab entry, styled by whether it is active.
fn tab_span(tab: Tab, active: bool) -> Span<'static> {
    let text = format!(" {} ", tab.label());
    if active {
        Span::styled(
            text,
            Style::default()
                .fg(tab.primary_color())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        // Inactive tabs are dimmed and lower-case to recede.
        Span::styled(text.to_lowercase(), Style::default().fg(Color::DarkGray))
    }
}

/// The header tab row: ` CLAUDE   codex   opencode` with the active tab accented.
pub fn tab_line(active: Tab, available: &[Tab]) -> Paragraph<'static> {
    let mut spans = Vec::new();
    for (i, &tab) in available.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(tab_span(tab, active == tab));
    }
    Paragraph::new(Line::from(spans))
}

/// Right-aligned account indicator shown in the header.
pub fn account(email: Option<&str>) -> Paragraph<'static> {
    let line = match email {
        Some(e) => Line::from(Span::raw(format!("✓ {e} "))),
        None => Line::from(Span::styled(
            "not signed in ",
            Style::default().fg(Color::DarkGray),
        )),
    };
    Paragraph::new(line).alignment(Alignment::Right)
}
