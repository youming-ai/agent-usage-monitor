use crate::state::Tab;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

pub fn tab_bar(active: Tab) -> Tabs<'static> {
    let titles = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "☁ Claude Code ",
                Style::default()
                    .fg(Color::Rgb(255, 165, 0))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "⚡ Codex ",
                Style::default()
                    .fg(Color::Rgb(138, 43, 226))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let selected = match active {
        Tab::ClaudeCode => 0,
        Tab::Codex => 1,
    };

    Tabs::new(titles)
        .block(
            Block::default()
                .title(" Ollama Monitor ")
                .title_style(
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .select(selected)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray)
                .fg(Color::White),
        )
        .divider(Span::styled(
            " │ ",
            Style::default().fg(Color::DarkGray),
        ))
}
