use agent_usage_monitor::cli;
use agent_usage_monitor::config::{self, Config};
use agent_usage_monitor::event::{AppEvent, EventLoop};
use agent_usage_monitor::platforms;
use agent_usage_monitor::quota;
use agent_usage_monitor::state::{AppState, Platform, Tab};
use agent_usage_monitor::stats;
use agent_usage_monitor::ui;
use agent_usage_monitor::updater;
use agent_usage_monitor::watcher::{self, WatcherMessage};
use clap::Parser;
use crossterm::event::KeyCode;
use std::io::IsTerminal;
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
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let args = cli::Cli::parse();
    info!(
        "Starting agent-usage-monitor v{}",
        env!("CARGO_PKG_VERSION")
    );

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

    // Handle subcommands
    match args.command {
        Some(cli::Commands::Update { force, dry_run }) => {
            return handle_update(force, dry_run);
        }
        Some(cli::Commands::Config { action }) => {
            return handle_config(action);
        }
        Some(cli::Commands::Stats(args)) => {
            return handle_stats(args, &config).await;
        }
        None => {
            // Continue with normal monitor mode
        }
    }

    // CLI `Option` paths override config; see `platforms::resolve_paths`.
    let agent_paths = platforms::resolve_paths(&args, &config);
    // Reader refresh is now FS-driven (see watcher module); the 30s
    // fallback below replaces the previous `tokio::time::interval` loop.

    for entry in platforms::entries() {
        info!(
            "Monitoring {} at {:?}",
            entry.log_name,
            agent_paths.path_for(entry.tab)
        );
    }
    let app_state = Arc::new(RwLock::new(AppState::with_capacity(config.max_records)));
    app_state
        .write()
        .unwrap()
        .detect_available_tabs(&agent_paths);

    // Reader task: FS-driven via the watcher module, with a 30s fallback
    // poll as a safety net for edge cases the watcher misses (atomic
    // rename, attribute-only writes, FS-event loss across reload).
    let reader_handle = task::spawn({
        let agent_paths = agent_paths.clone();
        let app_state = app_state.clone();
        async move {
            let (_platform_watchers, mut watcher_rx) =
                watcher::start_watchers(&agent_paths);
            let mut fallback = tokio::time::interval(Duration::from_secs(30));
            fallback.tick().await; // discard immediate first tick

            loop {
                tokio::select! {
                    Some(msg) = watcher_rx.recv() => {
                        let platform_filter: Option<Platform> = match &msg {
                            WatcherMessage::Event { platform, .. } => Some(*platform),
                            WatcherMessage::FallbackTick => None,
                        };
                        for entry in platforms::entries() {
                            if let Some(p) = platform_filter {
                                if entry.platform != p { continue; }
                            }
                            let path = agent_paths.path_for(entry.tab);
                            if !path.exists() { continue; }
                            let mut reader = entry.build_reader(path);
                            let platform = entry.platform;
                            let app_state = app_state.clone();
                            task::spawn_blocking(move || {
                                let records = reader.poll_delta();
                                if !records.is_empty() {
                                    info!("{:?}: Found {} new records", platform, records.len());
                                    if let Ok(mut state) = app_state.write() {
                                        state.add_records(platform, records);
                                    }
                                }
                            });
                        }
                    }
                    _ = fallback.tick() => {
                        for entry in platforms::entries() {
                            let path = agent_paths.path_for(entry.tab);
                            if !path.exists() { continue; }
                            let mut reader = entry.build_reader(path);
                            let platform = entry.platform;
                            let app_state = app_state.clone();
                            task::spawn_blocking(move || {
                                let records = reader.poll_delta();
                                if !records.is_empty() {
                                    info!("{:?}: Found {} new records", platform, records.len());
                                    if let Ok(mut state) = app_state.write() {
                                        state.add_records(platform, records);
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
    });

    // Quota fetcher: each fetch runs in spawn_blocking so the API call (HTTP
    // via ureq) does not block the tokio runtime. Fetchers are registered in
    // `quota::fetchers()` — adding a new quota source no longer requires
    // changing main.rs.
    let quota_state = app_state.clone();
    let quota_handle = task::spawn(async move {
        // Initial fetch — batch in one spawn_blocking to avoid concurrent
        // token-spawns at startup.
        {
            let qs = quota_state.clone();
            let results: Vec<(Platform, Option<quota::QuotaInfo>)> =
                task::spawn_blocking(move || {
                    quota::fetchers()
                        .iter()
                        .map(|f| (f.platform(), f.fetch()))
                        .collect()
                })
                .await
                .unwrap_or_default();

            for (platform, q) in results {
                let tab = Tab::from_platform(platform);
                if let Some(quota) = q {
                    info!("{:?} quota fetched successfully", platform);
                    if let Ok(mut state) = qs.write() {
                        state.platform_mut(tab).quota = Some(quota);
                    }
                } else {
                    warn!("Failed to fetch {:?} quota", platform);
                }
            }
        }

        // Refresh every 2 minutes — each platform independently.
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;

            for fetcher in quota::fetchers() {
                let p = fetcher.platform();
                let tab = match p {
                    Platform::ClaudeCode => Tab::ClaudeCode,
                    Platform::Codex => Tab::Codex,
                    _ => continue,
                };
                let stale = quota_state
                    .try_read()
                    .map(|state| {
                        state
                            .platform(tab)
                            .quota
                            .as_ref()
                            .is_none_or(|q| q.is_stale())
                    })
                    .unwrap_or(true);

                if stale {
                    let qs = quota_state.clone();
                    let fetcher_platform = p;
                    if let Some(q) = task::spawn_blocking(move || fetcher.fetch())
                        .await
                        .unwrap_or_default()
                    {
                        let tab = match fetcher_platform {
                            Platform::ClaudeCode => Tab::ClaudeCode,
                            Platform::Codex => Tab::Codex,
                            _ => continue,
                        };
                        if let Ok(mut s) = qs.write() {
                            s.platform_mut(tab).quota = Some(q);
                        }
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
    reader_handle.abort();
    quota_handle.abort();

    Ok(())
}

fn handle_update(
    force: bool,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

fn handle_config(
    action: Option<cli::ConfigAction>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

async fn handle_stats(
    args: cli::StatsArgs,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let since = args
        .since
        .as_deref()
        .map(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("--since must be YYYY-MM-DD: {e}"))?;
    let until = args
        .until
        .as_deref()
        .map(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose()
        .map_err(|e| format!("--until must be YYYY-MM-DD: {e}"))?;

    let platform_filter = if args.platform.is_empty() {
        None
    } else {
        Some(stats::resolve_platform_filter(&args.platform)?)
    };

    // Re-parse CLI to give resolve_paths a &Cli. Stats subcommand has not yet
    // consumed CLI args, so the global parse is cheap and safe.
    let cli = cli::Cli::parse();
    let paths = platforms::resolve_paths(&cli, config);

    let opts = stats::CollectOptions {
        include_quota: args.include_quota,
        filters: stats::Filters {
            platforms: platform_filter,
            since,
            until,
        },
    };
    let report = stats::collect(&paths, opts).await?;
    let pretty = args.pretty || (!args.compact && std::io::stdout().is_terminal());
    let stdout = std::io::stdout().lock();
    stats::write_json(&report, pretty, stdout)?;
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
