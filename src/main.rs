use agent_usage_monitor::cli;
use agent_usage_monitor::config::{self, Config};
use agent_usage_monitor::event::{AppEvent, EventLoop};
use agent_usage_monitor::platforms;
use agent_usage_monitor::quota;
use agent_usage_monitor::reader::UsageSource;
use agent_usage_monitor::state::AppState;
use agent_usage_monitor::ui;
use agent_usage_monitor::updater;
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

    // CLI `Option` paths override config; see `platforms::resolve_paths`.
    let agent_paths = platforms::resolve_paths(&args, &config);
    // Clamp to at least 1s: tokio::time::interval panics on a zero period, so
    // `--refresh 0` (or refresh = 0 in config) would crash the reader tasks.
    let refresh = args.refresh.unwrap_or(config.refresh).max(1);

    for entry in platforms::entries() {
        info!(
            "Monitoring {} at {:?}",
            entry.log_name,
            agent_paths.path_for(entry.tab)
        );
    }
    info!("Refresh interval: {} seconds", refresh);

    let app_state = Arc::new(RwLock::new(AppState::with_capacity(config.max_records)));
    app_state
        .write()
        .unwrap()
        .detect_available_tabs(&agent_paths);

    // Reader tasks: one per registered platform, driven uniformly via UsageSource.
    let sources: Vec<Arc<std::sync::Mutex<Box<dyn UsageSource>>>> = platforms::entries()
        .iter()
        .map(|entry| {
            let path = agent_paths.path_for(entry.tab);
            Arc::new(std::sync::Mutex::new(entry.build_reader(path)))
        })
        .collect();
    let mut reader_handles = Vec::new();
    for source in &sources {
        let source = source.clone();
        let reader_state = app_state.clone();
        let refresh_interval = refresh;
        let platform = source.lock().unwrap_or_else(|e| {
            warn!("{:?}: reader mutex poisoned, recovering", e.get_ref().platform());
            e.into_inner()
        }).platform();
        reader_handles.push(task::spawn(async move {
            // Initial scan
            let s = source.clone();
            let initial = task::spawn_blocking(move || {
                s.lock().unwrap_or_else(|e| {
                    warn!("{:?}: reader mutex poisoned during scan_all, recovering", e.get_ref().platform());
                    e.into_inner()
                }).scan_all()
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
                    s.lock().unwrap_or_else(|e| {
                        warn!("{:?}: reader mutex poisoned during poll_delta, recovering", e.get_ref().platform());
                        e.into_inner()
                    }).poll_delta()
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

    // Quota fetcher: each fetch runs in spawn_blocking so the API call (HTTP
    // via ureq / curl) does not block the tokio runtime. Per-platform stale
    // checks avoid re-fetching a healthy quota just because another platform
    // has a stale or missing result.
    let quota_state = app_state.clone();
    let quota_handle = task::spawn(async move {
        // Initial fetch — batch in one spawn_blocking to avoid three concurrent
        // token-spawns at startup.
        {
            let qs = quota_state.clone();
            let (claude_q, codex_q) = task::spawn_blocking(move || {
                (quota::claude::fetch_quota(),
                 quota::codex::fetch_quota())
            })
            .await
            .unwrap_or_default();

            if let Some(q) = claude_q {
                info!("Claude quota fetched successfully");
                if let Ok(mut state) = qs.write() {
                    state.claude_quota = Some(q);
                }
            } else {
                warn!("Failed to fetch Claude quota");
            }
            if let Some(q) = codex_q {
                info!("Codex quota fetched successfully");
                if let Ok(mut state) = qs.write() {
                    state.codex_quota = Some(q);
                }
            } else {
                warn!("Failed to fetch Codex quota");
            }
        }

        // Refresh every 2 minutes — each platform independently.
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;

            let stale = quota_state
                .try_read()
                .map(|state| (
                    state.claude_quota.as_ref().is_none_or(|q| q.is_stale()),
                    state.codex_quota.as_ref().is_none_or(|q| q.is_stale()),
                ))
                .unwrap_or((true, true));

            if stale.0 {
                let qs = quota_state.clone();
                if let Some(q) = task::spawn_blocking(quota::claude::fetch_quota)
                    .await.unwrap_or_default() {
                        if let Ok(mut s) = qs.write() {
                            s.claude_quota = Some(q);
                        }
                    }
            }
            if stale.1 {
                let qs = quota_state.clone();
                if let Some(q) = task::spawn_blocking(quota::codex::fetch_quota)
                    .await.unwrap_or_default() {
                        if let Ok(mut s) = qs.write() {
                            s.codex_quota = Some(q);
                        }
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

            if let Err(msg) = platforms::apply_config_key(&mut config, &key, &value) {
                eprintln!("{msg}");
                std::process::exit(1);
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
                            state.active_tab = state.active_tab.next_in(&state.available_tabs);
                        }
                    }
                    KeyCode::Left => {
                        if let Ok(mut state) = app_state.write() {
                            state.active_tab = state.active_tab.prev_in(&state.available_tabs);
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Ok(mut state) = app_state.write() {
                            let tab = state.active_tab;
                            state.clear_tab(tab);
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    Ok(())
}