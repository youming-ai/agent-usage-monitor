mod cli;
mod config;
mod event;
mod quota;
mod reader;
mod state;
mod ui;
mod updater;

use crate::config::Config;
use crate::event::{AppEvent, EventLoop};
use crate::reader::claude::ClaudeReader;
use crate::reader::codex::CodexReader;
use crate::reader::UsageSource;
use crate::state::AppState;
use clap::Parser;
use crossterm::event::KeyCode;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::task;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let args = cli::Cli::parse();
    info!("Starting agent-usage-monitor v{}", env!("CARGO_PKG_VERSION"));

    // Handle subcommands
    match args.command {
        Some(cli::Commands::Update { force, dry_run }) => {
            return handle_update(force, dry_run);
        }
        Some(cli::Commands::Config { action }) => {
            return handle_config(action);
        }
        None => {
            // Continue with normal monitor mode
        }
    }

    // Load configuration
    let config = match config::load_config() {
        Ok(config) => {
            info!("Configuration loaded from {:?}", config::config_file_path());
            config
        }
        Err(e) => {
            warn!("Failed to load config: {}, using defaults", e);
            Config::default()
        }
    };

    // Merge CLI args with config: CLI takes precedence, but we can no longer
    // rely on "arg != default" to detect user input (the user may have
    // explicitly set a value equal to the default in config). Instead, the
    // CLI fields are `Option`, so `None` is the unambiguous "user didn't
    // provide it" signal — and we fall back to the config value.
    let claude_path = args.claude_path.unwrap_or(config.claude_path);
    let codex_path = args.codex_path.unwrap_or(config.codex_path);
    let opencode_path = args.opencode_path.unwrap_or(config.opencode_path);
    let kimi_code_path = args.kimi_code_path.unwrap_or(config.kimi_code_path);
    // Clamp to at least 1s: tokio::time::interval panics on a zero period, so
    // `--refresh 0` (or refresh = 0 in config) would crash the reader tasks.
    let refresh = args.refresh.unwrap_or(config.refresh).max(1);

    info!("Monitoring Claude Code at {:?}", claude_path);
    info!("Monitoring Codex at {:?}", codex_path);
    info!("Monitoring opencode at {:?}", opencode_path);
    info!("Monitoring Kimi Code at {:?}", kimi_code_path);
    info!("Refresh interval: {} seconds", refresh);

    let app_state = Arc::new(RwLock::new(AppState::with_capacity(config.max_records)));

    // Reader tasks: one per usage source, all driven uniformly via UsageSource.
    let sources: Vec<Arc<std::sync::Mutex<Box<dyn UsageSource>>>> = vec![
        Arc::new(std::sync::Mutex::new(
            Box::new(ClaudeReader::new(claude_path.clone())) as Box<dyn UsageSource>,
        )),
        Arc::new(std::sync::Mutex::new(
            Box::new(CodexReader::new(codex_path.clone())) as Box<dyn UsageSource>,
        )),
        Arc::new(std::sync::Mutex::new(
            Box::new(reader::opencode::OpencodeReader::new(opencode_path.clone()))
                as Box<dyn UsageSource>,
        )),
        Arc::new(std::sync::Mutex::new(
            Box::new(reader::kimi_code::KimiCodeReader::new(kimi_code_path.clone()))
                as Box<dyn UsageSource>,
        )),
    ];
    let mut reader_handles = Vec::new();
    for source in &sources {
        let source = source.clone();
        let reader_state = app_state.clone();
        let refresh_interval = refresh;
        let platform = source.lock().unwrap_or_else(|e| e.into_inner()).platform();
        reader_handles.push(task::spawn(async move {
            // Initial scan
            let s = source.clone();
            let initial = task::spawn_blocking(move || {
                s.lock().unwrap_or_else(|e| e.into_inner()).scan_all()
            })
            .await
            .unwrap_or_default();
            info!("{:?}: Found {} initial records", platform, initial.len());
            if !initial.is_empty()
                && let Ok(mut state) = reader_state.write() {
                    state.add_records(platform, initial);
                }

            let mut interval = tokio::time::interval(Duration::from_secs(refresh_interval));
            loop {
                interval.tick().await;
                let s = source.clone();
                let new_records = task::spawn_blocking(move || {
                    s.lock().unwrap_or_else(|e| e.into_inner()).poll_delta()
                })
                .await
                .unwrap_or_default();
                if !new_records.is_empty() {
                    info!("{:?}: Found {} new records", platform, new_records.len());
                    if let Ok(mut state) = reader_state.write() {
                        state.add_records(platform, new_records);
                    }
                }
            }
        }));
    }

    // Quota reader task (Claude Code & Codex limits)
    let quota_state = app_state.clone();
    let quota_handle = task::spawn(async move {
        // Initial fetch
        {
            match quota::claude::fetch_quota() {
                Some(quota) => {
                    info!("Claude quota fetched successfully");
                    if let Ok(mut state) = quota_state.write() {
                        state.claude_quota = Some(quota);
                    }
                }
                None => warn!("Failed to fetch Claude quota"),
            }
            match quota::codex::fetch_quota() {
                Some(quota) => {
                    info!("Codex quota fetched successfully");
                    if let Ok(mut state) = quota_state.write() {
                        state.codex_quota = Some(quota);
                    }
                }
                None => warn!("Failed to fetch Codex quota"),
            }
        }

        // Refresh every 2 minutes
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;

            // Only refresh if stale
            let needs_refresh = {
                match quota_state.try_read() {
                    Ok(state) => {
                        state
                            .claude_quota
                            .as_ref()
                            .is_none_or(|q| q.is_stale())
                            || state
                                .codex_quota
                                .as_ref()
                                .is_none_or(|q| q.is_stale())
                    }
                    Err(_) => true,  // If we can't read, assume we need refresh
                }
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
    for handle in &reader_handles {
        handle.abort();
    }
    quota_handle.abort();

    Ok(())
}

fn handle_update(force: bool, dry_run: bool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("agent-usage-monitor (aum) updater");
    println!("=================================");
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

fn handle_config(action: Option<cli::ConfigAction>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match action {
        Some(cli::ConfigAction::Show) => {
            let config = config::load_config().unwrap_or_default();
            let content = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
            println!("Configuration file: {:?}", config::config_file_path());
            println!();
            println!("{}", content);
        }
        Some(cli::ConfigAction::Set { key, value }) => {
            let mut config = config::load_config().unwrap_or_default();
            
            match key.as_str() {
                "claude_path" => config.claude_path = std::path::PathBuf::from(value),
                "codex_path" => config.codex_path = std::path::PathBuf::from(value),
                "opencode_path" => config.opencode_path = std::path::PathBuf::from(value),
                "refresh" => config.refresh = value.parse().map_err(|e: std::num::ParseIntError| e.to_string())?,
                "max_records" => config.max_records = value.parse().map_err(|e: std::num::ParseIntError| e.to_string())?,
                _ => {
                    eprintln!("Unknown configuration key: {}", key);
                    eprintln!("Available keys: claude_path, codex_path, opencode_path, refresh, max_records");
                    std::process::exit(1);
                }
            }
            
            config::save_config(&config).map_err(|e| e.to_string())?;
            println!("Configuration updated.");
        }
        Some(cli::ConfigAction::Reset) => {
            let config = Config::default();
            config::save_config(&config).map_err(|e| e.to_string())?;
            println!("Configuration reset to defaults.");
        }
        None => {
            // Show config by default
            let config = config::load_config().unwrap_or_default();
            let content = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
            println!("Configuration file: {:?}", config::config_file_path());
            println!();
            println!("{}", content);
        }
    }
    
    Ok(())
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
                                state::Tab::OpenCode => state.clear_opencode(),
                                state::Tab::KimiCode => state.clear_kimi_code(),
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
