use crate::state::Tab;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

pub fn tab_bar(active: Tab) -> Tabs<'static> {
    let claude_color = Tab::ClaudeCode.primary_color();
    let codex_color = Tab::Codex.primary_color();
    let claude_icon = Tab::ClaudeCode.icon();
    let codex_icon = Tab::Codex.icon();
    let claude_label = Tab::ClaudeCode.label();
    let codex_label = Tab::Codex.label();

    let titles = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{claude_icon} {claude_label} "),
                Style::default()
                    .fg(claude_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{codex_icon} {codex_label} "),
                Style::default()
                    .fg(codex_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let selected = match active {
        Tab::ClaudeCode => 0,
        Tab::Codex => 1,
    };

    let active_color = active.primary_color();

    Tabs::new(titles)
        .block(
            Block::default()
                .title(" Usage Monitor ")
                .title_style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(active_color)),
        )
        .select(selected)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(active_color)
                .fg(Color::Black),
        )
        .divider(Span::styled(
            " │ ",
            Style::default().fg(Color::DarkGray),
        ))
}
