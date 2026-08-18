use agent_usage_monitor::cli;
use agent_usage_monitor::config::{self, Config};
use agent_usage_monitor::event::{AppEvent, EventLoop};
use agent_usage_monitor::mcp;
use agent_usage_monitor::platforms;
use agent_usage_monitor::readers::{self, PlatformReaders};
use agent_usage_monitor::state::{AppState, Platform};
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

        Some(cli::Commands::Mcp) => {
            return handle_mcp(&config).await;
        }
        None => {
            // Continue with normal monitor mode
        }
    }

    // CLI `Option` paths override config; see `platforms::resolve_paths`.
    let agent_paths = platforms::resolve_paths(&args, &config);
    agent_usage_monitor::quota::grok::set_data_dir(agent_paths.path_for(Platform::Grok));
    let fallback_seconds = args.refresh.unwrap_or(config.refresh);
    if fallback_seconds == 0 {
        warn!("refresh must be at least 1 second; using 1 second");
    }
    let fallback_interval = Duration::from_secs(fallback_seconds.max(1));

    for entry in platforms::entries() {
        info!(
            "Monitoring {} at {:?}",
            entry.log_name,
            agent_paths.path_for(entry.platform)
        );
    }
    let app_state = Arc::new(RwLock::new(AppState::with_capacity(config.max_records)));

    // Reader task: FS-driven via the watcher module, with a configured
    // fallback poll as a safety net for edge cases the watcher misses (atomic
    // rename, attribute-only writes, FS-event loss across reload).
    //
    // Readers are built ONCE and reused for every refresh. Rebuilding per
    // event would drop each reader's byte-offset/cursor/dedup state and
    // re-scan from zero, double-counting every record (see `readers` module).
    let reader_handle = task::spawn({
        let agent_paths = agent_paths.clone();
        let app_state = app_state.clone();
        async move {
            let build_paths = agent_paths.clone();
            let (mut readers, displayable) = task::spawn_blocking(move || {
                let readers = PlatformReaders::build(&build_paths);
                // `displayable_platforms` stats paths (Path::exists) — keep
                // the disk I/O off the async runtime with the reader build.
                let displayable =
                    platforms::displayable_platforms(readers.platforms(), &build_paths);
                (readers, displayable)
            })
            .await
            .expect("reader discovery task panicked");
            if let Ok(mut state) = app_state.write() {
                state.set_available_platforms(displayable);
            }
            let (mut platform_watchers, mut watcher_rx) = watcher::start_watchers(&agent_paths);
            let mut fallback = tokio::time::interval(fallback_interval);
            fallback.tick().await; // discard immediate first tick

            // Initial full scan, AWAITED before entering the loop: otherwise a
            // buffered watcher event or a fallback tick could `poll_delta`
            // (empty cursor -> reads the full history) before `scan_all` has
            // advanced the offset, and both merges would land -> double count.
            {
                let initial: Vec<(Platform, readers::SharedReader)> = readers
                    .platforms()
                    .into_iter()
                    .filter_map(|p| readers.get(p).map(|r| (p, r)))
                    .collect();
                let app_state = app_state.clone();
                let _ = task::spawn_blocking(move || {
                    for (platform, reader) in initial {
                        readers::scan_reader_into(&reader, &app_state, platform);
                    }
                })
                .await;
            }

            let mut refreshers = readers::ReaderRefreshers::new(app_state.clone());
            for platform in readers.platforms() {
                if let Some(reader) = readers.get(platform) {
                    refreshers.add(platform, reader);
                }
            }

            loop {
                tokio::select! {
                    Some(WatcherMessage::Event { platform, path }) = watcher_rx.recv() => {
                        refreshers.request_changed(platform, path);
                    }
                    _ = fallback.tick() => {
                        // Pick up paths that appeared after launch, then give
                        // every live reader a coalesced incremental refresh.
                        let discovery_paths = agent_paths.clone();
                        let (updated_readers, newly, displayable) = task::spawn_blocking(move || {
                            let mut readers = readers;
                            let newly = readers.discover_new(&discovery_paths);
                            let displayable = platforms::displayable_platforms(
                                readers.platforms(),
                                &discovery_paths,
                            );
                            (readers, newly, displayable)
                        })
                        .await
                        .expect("reader discovery task panicked");
                        readers = updated_readers;
                        if let Ok(mut state) = app_state.write() {
                            state.set_available_platforms(displayable);
                        }
                        for platform in readers.platforms() {
                            platform_watchers
                                .watch_platform(platform, agent_paths.path_for(platform));
                        }
                        for &platform in &newly {
                            if let Some(reader) = readers.get(platform) {
                                let app_state = app_state.clone();
                                let reader_for_scan = reader.clone();
                                let _ = task::spawn_blocking(move || {
                                    readers::scan_reader_into(
                                        &reader_for_scan,
                                        &app_state,
                                        platform,
                                    );
                                })
                                .await;
                                refreshers.add(platform, reader);
                            }
                        }
                        for platform in readers.platforms() {
                            if !newly.contains(&platform) {
                                refreshers.request(platform);
                            }
                        }
                    }
                }
            }
        }
    });

    // Quota fetcher: each fetch runs in spawn_blocking so the API call (HTTP
    // via ureq) does not block the tokio runtime. Quota and account capabilities
    // are registered alongside each platform in `platforms::REGISTRY`.
    let quota_state = app_state.clone();
    let quota_handle = task::spawn(async move {
        // Initial fetch — batch in one spawn_blocking to avoid concurrent
        // token-spawns at startup.
        {
            let qs = quota_state.clone();
            let (results, accounts) = task::spawn_blocking(|| {
                let quotas = platforms::entries()
                    .iter()
                    .filter_map(|entry| entry.quota_fetcher.map(|fetch| (entry.platform, fetch())))
                    .collect::<Vec<_>>();
                let accounts = platforms::entries()
                    .iter()
                    .filter_map(|entry| {
                        entry.account_fetcher.map(|fetch| (entry.platform, fetch()))
                    })
                    .collect::<Vec<_>>();
                (quotas, accounts)
            })
            .await
            .unwrap_or_default();

            for (platform, q) in results {
                if let Some(quota) = q {
                    info!("{:?} quota fetched successfully", platform);
                    if let Ok(mut state) = qs.write() {
                        state.platform_mut(platform).quota = Some(quota);
                    }
                } else {
                    warn!("Failed to fetch {:?} quota", platform);
                }
            }
            for (platform, email) in accounts {
                if let Ok(mut state) = qs.write() {
                    state.platform_mut(platform).account_email = email;
                }
            }
        }

        // Refresh every 2 minutes — each platform independently.
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;

            for entry in platforms::entries() {
                let p = entry.platform;
                let Some(fetch) = entry.quota_fetcher else {
                    continue;
                };
                let stale = quota_state
                    .try_read()
                    .map(|state| {
                        state
                            .platform(p)
                            .quota
                            .as_ref()
                            .is_none_or(|q| q.is_stale())
                    })
                    .unwrap_or(true);

                if stale {
                    let qs = quota_state.clone();
                    if let Some(q) = task::spawn_blocking(fetch).await.unwrap_or_default()
                        && let Ok(mut s) = qs.write()
                    {
                        s.platform_mut(p).quota = Some(q);
                    }
                }
            }

            for entry in platforms::entries() {
                let Some(fetch) = entry.account_fetcher else {
                    continue;
                };
                let platform = entry.platform;
                let email = task::spawn_blocking(fetch).await.unwrap_or_default();
                if let Ok(mut state) = quota_state.write() {
                    state.platform_mut(platform).account_email = email;
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
    agent_usage_monitor::quota::grok::set_data_dir(paths.path_for(Platform::Grok));

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

async fn handle_mcp(config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = cli::Cli::parse();
    let paths = platforms::resolve_paths(&cli, config);
    agent_usage_monitor::quota::grok::set_data_dir(paths.path_for(Platform::Grok));
    mcp::server::run_mcp_server(paths).await?;
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
                    _ => {}
                },
            }
        }
    }

    Ok(())
}
