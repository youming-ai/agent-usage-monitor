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
pub fn tab_line(active: Tab) -> Paragraph<'static> {
    let line = Line::from(vec![
        tab_span(Tab::ClaudeCode, active == Tab::ClaudeCode),
        Span::raw("  "),
        tab_span(Tab::Codex, active == Tab::Codex),
        Span::raw("  "),
        tab_span(Tab::OpenCode, active == Tab::OpenCode),
        Span::raw("  "),
        tab_span(Tab::KimiCode, active == Tab::KimiCode),
    ]);
    Paragraph::new(line)
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
