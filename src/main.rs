mod cli;
mod event;
mod ollama_client;
mod proxy;
mod state;
mod ui;

use crate::event::{AppEvent, EventLoop};
use crate::ollama_client::OllamaClient;
use crate::proxy::start_proxy;
use crate::state::AppState;
use clap::Parser;
use crossterm::event::KeyCode;
use ratatui::DefaultTerminal;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = cli::Cli::parse();
    let app_state = Arc::new(RwLock::new(AppState::new()));

    // Start proxy
    let proxy_state = app_state.clone();
    let proxy_target = format!("http://{}", args.ollama_host);
    let proxy_port = args.proxy_port;
    let proxy_handle = task::spawn(async move {
        if let Err(e) = start_proxy(proxy_port, proxy_target, proxy_state).await {
            eprintln!("Proxy error: {}", e);
        }
    });

    // Start Ollama polling
    let poll_state = app_state.clone();
    let ollama_client = OllamaClient::new(format!("http://{}", args.ollama_host));
    let refresh_secs = args.refresh;
    let poll_handle = task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(refresh_secs));
        loop {
            interval.tick().await;
            match ollama_client.poll_ps().await {
                Ok(ps) => {
                    if let Ok(mut state) = poll_state.write() {
                        OllamaClient::update_state(ps, &mut state);
                        state.last_error = None;
                    }
                }
                Err(e) => {
                    if let Ok(mut state) = poll_state.write() {
                        state.last_error = Some(format!("Poll error: {}", e));
                        state.running_models.clear();
                    }
                }
            }
        }
    });

    // Run TUI
    let tui_state = app_state.clone();
    let tui_handle = task::spawn_blocking(move || {
        let mut terminal = ratatui::init();
        let result = run_tui(&mut terminal, tui_state, proxy_port, &args.ollama_host);
        ratatui::restore();
        result
    });

    // Wait for TUI to finish (user pressed 'q')
    tui_handle.await??;

    // Abort background tasks
    proxy_handle.abort();
    poll_handle.abort();

    Ok(())
}

fn run_tui(
    terminal: &mut DefaultTerminal,
    app_state: Arc<RwLock<AppState>>,
    proxy_port: u16,
    ollama_host: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tick_rate = Duration::from_millis(250);
    let (mut event_loop, _tx) = EventLoop::new(tick_rate);

    loop {
        terminal.draw(|frame| {
            ui::render(frame, &app_state, proxy_port, ollama_host);
        })?;

        if let Some(event) = event_loop.rx.blocking_recv() {
            match event {
                AppEvent::Tick => {}
                AppEvent::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('p') => {
                        if let Ok(state) = app_state.write() {
                            state.toggle_proxy_paused();
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Ok(mut state) = app_state.write() {
                            state.clear_calls();
                        }
                    }
                    _ => {}
                },
                AppEvent::Quit => break,
            }
        }
    }

    Ok(())
}
