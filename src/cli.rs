use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aum")]
#[command(version)]
#[command(about = "Real-time usage monitor for Claude Code, Codex, and Grok quota")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to Claude Code data directory
    #[arg(long)]
    pub claude_path: Option<PathBuf>,

    /// Path to Codex data directory
    #[arg(long)]
    pub codex_path: Option<PathBuf>,

    /// Path to Grok data directory (quota reads its auth.json)
    #[arg(long)]
    pub grok_path: Option<PathBuf>,

    /// Fallback polling interval in seconds (minimum: 1)
    #[arg(short, long, value_parser = clap::value_parser!(u64).range(1..))]
    pub refresh: Option<u64>,
}

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// 仅输出指定平台 (逗号分隔, 支持 config_key / platform variant / log_name)
    #[arg(long, value_delimiter = ',')]
    pub platform: Vec<String>,

    /// 起始日期（含） YYYY-MM-DD
    #[arg(long)]
    pub since: Option<String>,

    /// 结束日期（含） YYYY-MM-DD
    #[arg(long)]
    pub until: Option<String>,

    /// 拉取 quota (Claude/Codex/Grok 需本地凭据; Grok 读取 ~/.grok/auth.json)
    #[arg(long)]
    pub include_quota: bool,

    /// Pretty-print JSON (默认: stdout 是 TTY 时 pretty)
    #[arg(long)]
    pub pretty: bool,

    /// 显式 compact 输出 (反 --pretty)
    #[arg(long, conflicts_with = "pretty")]
    pub compact: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
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
    /// 输出 JSON 用量报告（不启动 TUI）
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
