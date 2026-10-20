use agent_usage_monitor::cli;
use agent_usage_monitor::config::{self, Config};
use agent_usage_monitor::event::{AppEvent, EventLoop};
use agent_usage_monitor::mcp;
use agent_usage_monitor::platforms;
use agent_usage_monitor::readers::{self, PlatformReaders};
use agent_usage_monitor::state::{AgentPaths, AppState, Platform};
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
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let args = cli::Cli::parse();
    args.validate().unwrap_or_else(|error| error.exit());
    info!(
        "Starting agent-usage-monitor v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Maintenance commands must work even when the config needs repair.
    match &args.command {
        Some(cli::Commands::Update { force, dry_run }) => {
            return handle_update(*force, *dry_run);
        }
        Some(cli::Commands::Config { action }) => {
            return handle_config(action.as_ref());
        }
        _ => {}
    }

    let config = config::load_config()?;
    info!(
        "Configuration resolved from {:?}",
        config::config_file_path()
    );

    // CLI `Option` paths override config; see `platforms::resolve_paths`.
    let agent_paths = platforms::resolve_paths(&args, &config);

    // Handle subcommands
    let monitor_refresh = match args.command {
        Some(cli::Commands::Stats(args)) => {
            return handle_stats(args, &agent_paths).await;
        }

        Some(cli::Commands::Mcp) => {
            return handle_mcp(agent_paths).await;
        }
        Some(cli::Commands::Monitor(options)) => options.refresh.or(args.refresh),
        None => args.refresh,
        Some(cli::Commands::Update { .. } | cli::Commands::Config { .. }) => unreachable!(),
    };
    let fallback_seconds = monitor_refresh.unwrap_or(config.refresh);
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
                let displayable = readers.platforms();
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
                            let displayable = readers.platforms();
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
    // via ureq) does not block the tokio runtime. Each supported platform has
    // a quota fetcher in `platforms::REGISTRY`.
    let quota_state = app_state.clone();
    let quota_handle = task::spawn(async move {
        // Refresh every 2 minutes — each platform independently.
        let mut interval = tokio::time::interval(Duration::from_secs(120));
        loop {
            interval.tick().await;

            for entry in platforms::entries() {
                refresh_quota(&quota_state, entry.platform, entry.quota_fetcher).await;
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

async fn refresh_quota(
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
    fetch: agent_usage_monitor::quota::Fetcher,
) {
    // The periodic tick determines freshness. Gating again on response age
    // would skip a tick whenever the previous request took nonzero time.
    if let Some(quota) = task::spawn_blocking(fetch).await.unwrap_or_default()
        && let Ok(mut state) = state.write()
    {
        state.platform_mut(platform).quota = Some(quota);
    }
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
        // Returned via `main`'s `Result`, whose Termination impl prints the
        // message and exits 1 — same output as the old eprintln + exit(1).
        Err(e) => Err(anyhow::anyhow!(e).into()),
    }
}

fn handle_config(
    action: Option<&cli::ConfigAction>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match action {
        Some(cli::ConfigAction::Show) | None => {
            let config = config::load_config()?;
            let content = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
            println!("Configuration file: {:?}", config::config_file_path());
            println!();
            println!("{}", content);
        }
        Some(cli::ConfigAction::Set { key, value }) => {
            let mut config = config::load_config()?;

            if let Err(msg) = platforms::apply_config_key(&mut config, key, value) {
                return Err(anyhow::anyhow!(msg).into());
            }

            config::save_config(&config).map_err(|e| e.to_string())?;
            println!("Configuration updated.");
        }
        Some(cli::ConfigAction::Reset) => {
            let config = Config::default();
            config::save_config(&config).map_err(|e| e.to_string())?;
            println!("Configuration reset to defaults.");
        }
    }

    Ok(())
}

async fn handle_stats(
    args: cli::StatsArgs,
    agent_paths: &AgentPaths,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let platform_filter = if args.platform.is_empty() {
        None
    } else {
        Some(stats::resolve_platform_filter(&args.platform)?)
    };

    let opts = stats::CollectOptions {
        include_quota: args.include_quota,
        filters: stats::Filters {
            platforms: platform_filter,
            since: args.since,
            until: args.until,
        },
    };
    let report = stats::collect(agent_paths, opts).await?;
    let pretty = args.pretty || (!args.compact && std::io::stdout().is_terminal());
    let stdout = std::io::stdout().lock();
    stats::write_json(&report, pretty, stdout)?;
    Ok(())
}

async fn handle_mcp(
    agent_paths: AgentPaths,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    mcp::server::run_mcp_server(agent_paths).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use agent_usage_monitor::quota::QuotaInfo;
    use std::time::Instant;

    fn fetched_quota() -> Option<QuotaInfo> {
        Some(QuotaInfo {
            tool_name: "refreshed".into(),
            email: None,
            account_id: None,
            plan: None,
            org: None,
            windows: vec![],
            live_summary: None,
            fetched_at: Instant::now(),
            error: None,
        })
    }

    #[tokio::test]
    async fn scheduled_refresh_does_not_skip_a_recent_successful_response() {
        let state = Arc::new(RwLock::new(AppState::new()));
        for platform in Platform::all() {
            // A request started at t=0 and completed at t=1 is only 119s old
            // at the t=120 tick. It must still be refreshed on that tick.
            for age_seconds in [119, 1] {
                let mut previous = fetched_quota().unwrap();
                previous.tool_name = "previous".into();
                previous.fetched_at = Instant::now() - Duration::from_secs(age_seconds);
                state.write().unwrap().platform_mut(*platform).quota = Some(previous);

                refresh_quota(&state, *platform, fetched_quota).await;

                assert_eq!(
                    state
                        .read()
                        .unwrap()
                        .platform(*platform)
                        .quota
                        .as_ref()
                        .unwrap()
                        .tool_name,
                    "refreshed",
                    "the scheduled refresh must run even when the previous response is {age_seconds}s old",
                );
            }
        }
    }
}
