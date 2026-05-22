pub mod model_table;
pub mod status_bar;
pub mod usage_table;

use crate::state::AppState;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use std::sync::{Arc, RwLock};

pub fn render(
    frame: &mut Frame,
    app_state: &Arc<RwLock<AppState>>,
    proxy_port: u16,
    ollama_host: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(55),
            Constraint::Min(1),
        ])
        .split(frame.area());

    frame.render_widget(model_table::model_table_widget(app_state), chunks[0]);
    frame.render_widget(usage_table::usage_table_widget(app_state), chunks[1]);
    frame.render_widget(status_bar::status_bar_widget(app_state, proxy_port, ollama_host), chunks[2]);
}
