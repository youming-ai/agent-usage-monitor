use agent_usage_monitor::cli;
use agent_usage_monitor::config::{self, Config};
use agent_usage_monitor::event::{AppEvent, EventLoop};
use agent_usage_monitor::mcp;
use agent_usage_monitor::platforms;
use agent_usage_monitor::quota;
use agent_usage_monitor::readers::{self, PlatformReaders};
use agent_usage_monitor::state::{AppState, Platform, Tab, resolve};
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
    //
    // Readers are built ONCE and reused for every refresh. Rebuilding per
    // event would drop each reader's byte-offset/cursor/dedup state and
    // re-scan from zero, double-counting every record (see `readers` module).
    let reader_handle = task::spawn({
        let agent_paths = agent_paths.clone();
        let app_state = app_state.clone();
        async move {
            let mut readers = PlatformReaders::build(&agent_paths);
            let (_platform_watchers, mut watcher_rx) = watcher::start_watchers(&agent_paths);
            let mut fallback = tokio::time::interval(Duration::from_secs(30));
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

            loop {
                tokio::select! {
                    Some(msg) = watcher_rx.recv() => {
                        // Poll only the platform whose files changed.
                        let targets: Vec<Platform> = match &msg {
                            WatcherMessage::Event { platform, .. } => vec![*platform],
                        };
                        for platform in targets {
                            if let Some(reader) = readers.get(platform) {
                                let app_state = app_state.clone();
                                task::spawn_blocking(move || {
                                    readers::poll_reader_into(&reader, &app_state, platform);
                                });
                            }
                        }
                    }
                    _ = fallback.tick() => {
                        // Pick up platforms whose data dir appeared after
                        // launch (full scan once), poll the rest for deltas.
                        let newly: Vec<Platform> = readers.discover_new(&agent_paths);
                        for platform in readers.platforms() {
                            if let Some(reader) = readers.get(platform) {
                                let app_state = app_state.clone();
                                let is_new = newly.contains(&platform);
                                task::spawn_blocking(move || {
                                    if is_new {
                                        readers::scan_reader_into(&reader, &app_state, platform);
                                    } else {
                                        readers::poll_reader_into(&reader, &app_state, platform);
                                    }
                                });
                            }
                        }
                    }
                }
            }
        }
    });

    // Quota fetcher: each fetch runs in spawn_blocking so the API call (HTTP
    // via ureq) does not block the tokio runtime. Fetchers are registered in
    // `quota::FETCHERS` — adding a new quota source no longer requires
    // changing main.rs.
    let quota_state = app_state.clone();
    let quota_handle = task::spawn(async move {
        // Initial fetch — batch in one spawn_blocking to avoid concurrent
        // token-spawns at startup.
        {
            let qs = quota_state.clone();
            let results: Vec<(Platform, Option<quota::QuotaInfo>)> = task::spawn_blocking(|| {
                quota::FETCHERS
                    .iter()
                    .map(|&(platform, fetch)| (platform, fetch()))
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

            for &(p, fetch) in quota::FETCHERS {
                let tab = Tab::from_platform(p);
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
                    if let Some(q) = task::spawn_blocking(fetch).await.unwrap_or_default() {
                        let tab = Tab::from_platform(p);
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

async fn handle_mcp(config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = cli::Cli::parse();
    let paths = platforms::resolve_paths(&cli, config);
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
                    KeyCode::Char('q') => break,
                    // Esc backs out of session-selection mode; quits otherwise.
                    KeyCode::Esc => {
                        if let Ok(mut state) = app_state.write() {
                            if state.sessions_focused {
                                state.unfocus_sessions();
                            } else {
                                break;
                            }
                        }
                    }
                    // Tab/←/→ switch tabs only when not selecting a session, so
                    // the arrow keys can drive the list once it has focus.
                    KeyCode::Tab | KeyCode::Right => {
                        if let Ok(mut state) = app_state.write()
                            && !state.sessions_focused
                        {
                            state.active_tab = state.active_tab.next_in(&state.available_tabs);
                            state.reset_session_focus();
                        }
                    }
                    KeyCode::Left => {
                        if let Ok(mut state) = app_state.write()
                            && !state.sessions_focused
                        {
                            state.active_tab = state.active_tab.prev_in(&state.available_tabs);
                            state.reset_session_focus();
                        }
                    }
                    // Down/j: enter the sessions list (highlighting the first
                    // row) or move down within it. Up/k moves up once focused.
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let Ok(mut state) = app_state.write() {
                            if state.sessions_focused {
                                state.move_selection(1);
                            } else {
                                state.focus_sessions();
                            }
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if let Ok(mut state) = app_state.write() {
                            if state.sessions_focused {
                                state.move_selection(-1);
                            } else {
                                state.focus_sessions();
                            }
                        }
                    }
                    // Enter: focus the list, then resume the selected session.
                    KeyCode::Enter => {
                        let launch = app_state.write().ok().and_then(|mut state| {
                            if state.sessions_focused {
                                state
                                    .selected_launch()
                                    .map(|(sid, cwd)| (state.active_tab, sid, cwd))
                            } else {
                                state.focus_sessions();
                                None
                            }
                        });
                        if let Some((tab, sid, cwd)) = launch {
                            let sid = resolve(sid).to_string();
                            let cwd = resolve(cwd).to_string();
                            if !sid.is_empty() {
                                let (program, args) =
                                    platforms::entry_for_tab(tab).resume_command(&sid);
                                // Open the session in a new terminal window and
                                // keep monitoring. If no new window could be
                                // opened (e.g. no terminal emulator on Linux),
                                // fall back to handing off the current terminal
                                // by exec-replacing this process.
                                if let Err(e) = spawn_new_window(program, &args, &cwd, &sid) {
                                    warn!(
                                        "could not open a new terminal window ({e}); \
                                         handing off the current terminal instead"
                                    );
                                    return exec_handoff(program, &args, &cwd);
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Ok(mut state) = app_state.write() {
                            let tab = state.active_tab;
                            state.clear_tab(tab);
                            state.reset_session_focus();
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    Ok(())
}

/// Single-quote a string for `/bin/sh`, escaping embedded quotes — so a cwd
/// or arg containing spaces or shell metacharacters is passed through literally.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The `/bin/sh` command that resumes a session: `cd <cwd> && exec <prog> …`
/// (the `cd` is omitted when the working directory is unknown). Shared by the
/// macOS `.command` script and the Linux `sh -c` invocation.
fn resume_shell_line(program: &str, args: &[String], cwd: &str) -> String {
    let mut line = String::new();
    if !cwd.is_empty() {
        line.push_str(&format!("cd {} && ", shell_quote(cwd)));
    }
    line.push_str("exec ");
    line.push_str(&shell_quote(program));
    for a in args {
        line.push(' ');
        line.push_str(&shell_quote(a));
    }
    line
}

/// Open the resume command in a new terminal window, leaving `aum` running.
/// `Ok` means a window was launched; `Err` means the caller should fall back
/// to `exec_handoff`. stdin/stdout/stderr are detached so the launcher can't
/// disturb the TUI still drawing on this terminal.
#[cfg(target_os = "macos")]
fn spawn_new_window(
    program: &str,
    args: &[String],
    cwd: &str,
    session_id: &str,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    // A `*.command` file opened via `open` launches the user's default
    // terminal (Terminal.app, or iTerm if they've set it) in a new window.
    let safe: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let path = std::env::temp_dir().join(format!("aum-resume-{safe}.command"));
    // ponytail: the script lingers in the temp dir until the OS clears it;
    // reopening the same session overwrites it, so at most one per session.
    std::fs::write(
        &path,
        format!("#!/bin/sh\n{}\n", resume_shell_line(program, args, cwd)),
    )?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;

    std::process::Command::new("open")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

/// Linux: no standard "new terminal window" mechanism, so this is best-effort.
/// `x-terminal-emulator` is the Debian/Ubuntu alternatives entry pointing at
/// the user's default emulator; if it's absent, `spawn` errors and the caller
/// falls back to the exec hand-off.
#[cfg(not(target_os = "macos"))]
fn spawn_new_window(
    program: &str,
    args: &[String],
    cwd: &str,
    _session_id: &str,
) -> std::io::Result<()> {
    use std::process::Stdio;

    std::process::Command::new("x-terminal-emulator")
        .arg("-e")
        .arg("sh")
        .arg("-c")
        .arg(resume_shell_line(program, args, cwd))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

/// Fallback path: restore the terminal and `exec`-replace this process with the
/// agent CLI, resuming `session_id` in `cwd`. Used only when no new window
/// could be opened. Returns (an `Err`) only if `exec` itself fails.
fn exec_handoff(
    program: &str,
    args: &[String],
    cwd: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::os::unix::process::CommandExt;

    ratatui::restore();

    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if !cwd.is_empty() {
        cmd.current_dir(cwd);
    }

    // `exec` only returns on failure. Surface it plainly on the now-restored
    // terminal so the user knows why nothing launched.
    let err = cmd.exec();
    eprintln!("Failed to launch `{program}` to resume session: {err}");
    Err(Box::new(err))
}

#[cfg(test)]
mod launch_tests {
    use super::*;

    #[test]
    fn shell_quote_wraps_and_escapes() {
        assert_eq!(shell_quote("/a/b c"), "'/a/b c'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn resume_shell_line_includes_cd_only_when_cwd_known() {
        let args = vec!["--resume".to_string(), "abc".to_string()];
        assert_eq!(
            resume_shell_line("claude", &args, "/work/proj"),
            "cd '/work/proj' && exec 'claude' '--resume' 'abc'"
        );
        assert_eq!(
            resume_shell_line("cursor-agent", &["--resume=abc".to_string()], ""),
            "exec 'cursor-agent' '--resume=abc'"
        );
    }
}
