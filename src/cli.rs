use chrono::NaiveDate;
use clap::{Args, CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aum")]
#[command(version)]
#[command(about = "Real-time usage monitor for Claude Code and Codex")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to Claude Code data directory
    #[arg(long, global = true)]
    pub claude_path: Option<PathBuf>,

    /// Path to Codex data directory
    #[arg(long, global = true)]
    pub codex_path: Option<PathBuf>,

    /// Monitor fallback polling interval in seconds (minimum: 1; monitor mode only)
    #[arg(short, long, value_parser = clap::value_parser!(u64).range(1..))]
    pub refresh: Option<u64>,
}

impl Cli {
    pub fn validate(&self) -> Result<(), clap::Error> {
        if self.refresh.is_some()
            && self
                .command
                .as_ref()
                .is_some_and(|command| !matches!(command, Commands::Monitor(_)))
        {
            return Err(Self::command().error(
                clap::error::ErrorKind::ArgumentConflict,
                "--refresh is only available with `aum` or `aum monitor`",
            ));
        }
        if let Some(Commands::Monitor(args)) = &self.command
            && self.refresh.is_some()
            && args.refresh.is_some()
        {
            return Err(Self::command().error(
                clap::error::ErrorKind::ArgumentConflict,
                "--refresh may only be specified once",
            ));
        }
        if let Some(Commands::Stats(args)) = &self.command
            && let (Some(since), Some(until)) = (args.since, args.until)
            && since > until
        {
            return Err(Self::command().error(
                clap::error::ErrorKind::ValueValidation,
                "--since must be on or before --until (UTC dates)",
            ));
        }
        Ok(())
    }
}

#[derive(Args, Debug)]
pub struct MonitorArgs {
    /// Fallback polling interval in seconds (minimum: 1)
    #[arg(short, long, value_parser = clap::value_parser!(u64).range(1..))]
    pub refresh: Option<u64>,
}

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Platforms to include: claude, codex (comma-separated; legacy names accepted)
    #[arg(long, value_delimiter = ',', value_parser = parse_platform)]
    pub platform: Vec<String>,

    /// Start date, inclusive, in UTC (YYYY-MM-DD)
    #[arg(long, value_parser = parse_date)]
    pub since: Option<NaiveDate>,

    /// End date, inclusive, in UTC (YYYY-MM-DD)
    #[arg(long, value_parser = parse_date)]
    pub until: Option<NaiveDate>,

    /// Include current live quota, independent of dates (requires local credentials)
    #[arg(long)]
    pub include_quota: bool,

    /// Pretty-print JSON (default when stdout is a terminal)
    #[arg(long)]
    pub pretty: bool,

    /// Compact JSON (default when stdout is redirected)
    #[arg(long, conflicts_with = "pretty")]
    pub compact: bool,
}

fn parse_date(value: &str) -> Result<NaiveDate, String> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "expected a valid UTC date in YYYY-MM-DD format".to_string())?;
    if date.format("%Y-%m-%d").to_string() != value {
        return Err("expected YYYY-MM-DD format".to_string());
    }
    Ok(date)
}

fn parse_platform(value: &str) -> Result<String, String> {
    crate::stats::resolve_platform(value).map_err(|error| error.to_string())
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the terminal dashboard (also the default with no subcommand)
    Monitor(MonitorArgs),
    /// Check for updates and install the latest version
    Update {
        /// Force update even if already on latest version
        #[arg(short, long)]
        force: bool,

        /// Show what would be updated without installing
        #[arg(short, long)]
        dry_run: bool,
    },

    /// Show or edit configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Print a JSON usage report without starting the dashboard
    Stats(StatsArgs),
    /// Run as an MCP (Model Context Protocol) server over stdio
    Mcp,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key (e.g., claude_path, codex_path, refresh, max_records)
        key: String,

        /// Configuration value
        value: String,
    },

    /// Reset configuration to defaults
    Reset,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_schema_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn data_paths_work_before_and_after_subcommands() {
        for args in [
            vec!["aum", "--codex-path", "/tmp/codex", "stats"],
            vec!["aum", "stats", "--codex-path", "/tmp/codex"],
            vec!["aum", "mcp", "--codex-path", "/tmp/codex"],
            vec!["aum", "monitor", "--codex-path", "/tmp/codex"],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            cli.validate().unwrap();
            assert_eq!(cli.codex_path, Some(PathBuf::from("/tmp/codex")));
        }
    }

    #[test]
    fn refresh_only_applies_to_monitoring() {
        for args in [
            vec!["aum", "-r", "2"],
            vec!["aum", "monitor", "-r", "2"],
            vec!["aum", "-r", "2", "monitor"],
        ] {
            Cli::try_parse_from(args).unwrap().validate().unwrap();
        }
        for command in ["stats", "mcp", "config", "update"] {
            assert!(
                Cli::try_parse_from(["aum", "-r", "2", command])
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        assert!(
            Cli::try_parse_from(["aum", "-r", "2", "monitor", "-r", "3"])
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(Cli::try_parse_from(["aum", "monitor", "-r", "0"]).is_err());
    }

    #[test]
    fn invalid_dates_and_platforms_are_rejected_during_parsing() {
        for args in [
            vec!["aum", "stats", "--since", "2026-02-30"],
            vec!["aum", "stats", "--until", "2026-5-1"],
            vec!["aum", "stats", "--platform", ""],
            vec!["aum", "stats", "--platform", "claude,,codex"],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn refresh_requires_at_least_one_second() {
        assert!(Cli::try_parse_from(["aum", "--refresh", "0"]).is_err());
        assert_eq!(
            Cli::try_parse_from(["aum", "--refresh", "1"])
                .unwrap()
                .refresh,
            Some(1)
        );
    }
}
