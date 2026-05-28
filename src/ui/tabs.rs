use crate::state::Tab;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

pub fn tab_bar(active: Tab) -> Tabs<'static> {
    let titles = vec![
        Line::from(Span::styled(
            " Claude Code ",
            Style::default().fg(Color::Rgb(255, 165, 0)),
        )),
        Line::from(Span::styled(
            " Codex ",
            Style::default().fg(Color::Rgb(138, 43, 226)),
        )),
    ];

    let selected = match active {
        Tab::ClaudeCode => 0,
        Tab::Codex => 1,
    };

    Tabs::new(titles)
        .block(Block::default().title(" Usage Monitor ").borders(Borders::ALL))
        .select(selected)
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
        )
        .divider("|")
}
