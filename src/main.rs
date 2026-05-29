mod cli;
mod event;
mod quota;
mod reader;
mod state;
mod ui;
mod updater;

use crate::event::{AppEvent, EventLoop};
use crate::reader::claude::ClaudeReader;
use crate::reader::codex::CodexReader;
use crate::state::AppState;
use clap::Parser;
use crossterm::event::KeyCode;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = cli::Cli::parse();

    // Handle subcommands
    match args.command {
        Some(cli::Commands::Update { force, dry_run }) => {
            return handle_update(force, dry_run);
        }
        None => {
            // Continue with normal monitor mode
        }
    }

    let app_state = Arc::new(RwLock::new(AppState::new()));

    // Claude reader task (usage records)
    let claude_state = app_state.clone();
    let mut claude_reader = ClaudeReader::new(args.claude_path.clone());
    let refresh = args.refresh;
    let claude_handle = task::spawn(async move {
        let initial = claude_reader.scan_all();
        if !initial.is_empty()
            && let Ok(mut state) = claude_state.write() {
                state.add_claude_records(initial);
            }

        let mut interval = tokio::time::interval(Duration::from_secs(refresh));
        loop {
            interval.tick().await;
            let new_records = claude_reader.poll_delta();
            if !new_records.is_empty()
                && let Ok(mut state) = claude_state.write() {
                    state.add_claude_records(new_records);
                }
        }
    });

    // Codex reader task (usage records)
    let codex_state = app_state.clone();
    let mut codex_reader = CodexReader::new(args.codex_path.clone());
    let codex_handle = task::spawn(async move {
        let initial = codex_reader.scan_all();
        if !initial.is_empty()
            && let Ok(mut state) = codex_state.write() {
                state.add_codex_records(initial);
            }

        let mut interval = tokio::time::interval(Duration::from_secs(refresh));
        loop {
            interval.tick().await;
            let new_records = codex_reader.poll_delta();
            if !new_records.is_empty()
                && let Ok(mut state) = codex_state.write() {
                    state.add_codex_records(new_records);
                }
        }
    });

    // Quota reader task (Claude Code & Codex limits)
    let quota_state = app_state.clone();
    let quota_handle = task::spawn(async move {
        // Initial fetch
        {
            if let Some(quota) = quota::claude::fetch_quota()
                && let Ok(mut state) = quota_state.write() {
                    state.claude_quota = Some(quota);
                }
            if let Some(quota) = quota::codex::fetch_quota()
                && let Ok(mut state) = quota_state.write() {
                    state.codex_quota = Some(quota);
                }
        }

        // Refresh every 2 minutes
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;

            // Only refresh if stale
            let needs_refresh = {
                let state = quota_state.read().unwrap();
                state
                    .claude_quota
                    .as_ref()
                    .is_none_or(|q| q.is_stale())
                    || state
                        .codex_quota
                        .as_ref()
                        .is_none_or(|q| q.is_stale())
            };

            if needs_refresh {
                if let Some(quota) = quota::claude::fetch_quota()
                    && let Ok(mut state) = quota_state.write() {
                        state.claude_quota = Some(quota);
                    }
                if let Some(quota) = quota::codex::fetch_quota()
                    && let Ok(mut state) = quota_state.write() {
                        state.codex_quota = Some(quota);
                    }
            }
        }
    });

    // TUI task
    let tui_state = app_state.clone();
    let tui_handle = task::spawn_blocking(move || {
        let mut terminal = ratatui::init();
        let result = run_tui(&mut terminal, tui_state);
        ratatui::restore();
        result
    });

    tui_handle.await??;
    claude_handle.abort();
    codex_handle.abort();
    quota_handle.abort();

    Ok(())
}

fn handle_update(force: bool, dry_run: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Usage Monitor Updater");
    println!("====================");
    println!("Current version: v{}", updater::current_version());
    println!();

    match updater::check_and_update(force, dry_run) {
        Ok(result) => {
            if result.updated {
                println!("✓ {}", result.message);
                println!();
                println!("Please restart the application to use the new version.");
            } else if dry_run {
                println!("Dry run result:");
                println!("  Current: v{}", result.old_version);
                println!("  Latest:  v{}", result.new_version);
                println!();
                println!("{}", result.message);
            } else {
                println!("✓ {}", result.message);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_tui(
    terminal: &mut ratatui::DefaultTerminal,
    app_state: Arc<RwLock<AppState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tick_rate = Duration::from_millis(250);
    let (mut event_loop, _tx) = EventLoop::new(tick_rate);

    loop {
        terminal.draw(|frame| {
            ui::render(frame, &app_state);
        })?;

        if let Some(event) = event_loop.rx.blocking_recv() {
            match event {
                AppEvent::Tick => {}
                AppEvent::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Tab | KeyCode::Right => {
                        if let Ok(mut state) = app_state.write() {
                            state.active_tab = state.active_tab.next();
                        }
                    }
                    KeyCode::Left => {
                        if let Ok(mut state) = app_state.write() {
                            state.active_tab = state.active_tab.prev();
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Ok(mut state) = app_state.write() {
                            match state.active_tab {
                                state::Tab::ClaudeCode => state.clear_claude(),
                                state::Tab::Codex => state.clear_codex(),
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    Ok(())
}
