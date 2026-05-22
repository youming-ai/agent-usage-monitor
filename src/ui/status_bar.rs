use crate::state::AppState;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::sync::{Arc, RwLock};

pub fn status_bar_widget(
    app_state: &Arc<RwLock<AppState>>,
    proxy_port: u16,
    ollama_host: &str,
) -> Paragraph<'static> {
    let state = app_state.read().unwrap_or_else(|e| e.into_inner());
    let proxy_status = if state.is_proxy_paused() {
        Span::styled("Proxy: PAUSED", Style::default().fg(Color::Red))
    } else {
        Span::styled("Proxy: ON", Style::default().fg(Color::Green))
    };

    let error_span = if let Some(ref err) = state.last_error {
        Span::styled(format!(" | ERROR: {}", err), Style::default().fg(Color::Red))
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![
        proxy_status,
        Span::raw(format!(
            " | {} → {} | Calls: {} | q:quit p:pause r:clr",
            proxy_port, ollama_host, state.total_calls
        )),
        error_span,
    ]);

    Paragraph::new(line).block(Block::default().borders(Borders::TOP))
}
