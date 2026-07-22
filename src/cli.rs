use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aum")]
#[command(version)]
#[command(about = "Real-time usage monitor for Claude Code, Codex & Cursor CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to Claude Code data directory
    #[arg(long)]
    pub claude_path: Option<PathBuf>,

    /// Path to Codex data directory
    #[arg(long)]
    pub codex_path: Option<PathBuf>,

    /// Path to Cursor CLI data directory
    #[arg(long)]
    pub cursor_path: Option<PathBuf>,

    /// Polling interval in seconds
    #[arg(short, long)]
    pub refresh: Option<u64>,
}

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// 仅输出指定平台 (逗号分隔, 支持 config_key / Tab variant / log_name)
    #[arg(long, value_delimiter = ',')]
    pub platform: Vec<String>,

    /// 起始日期（含） YYYY-MM-DD
    #[arg(long)]
    pub since: Option<String>,

    /// 结束日期（含） YYYY-MM-DD
    #[arg(long)]
    pub until: Option<String>,

    /// 拉取 quota (Claude/Codex 需本地凭据)
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
        /// Configuration key (e.g., claude_path, copilot_path, antigravity_path, refresh, max_records)
        key: String,

        /// Configuration value
        value: String,
    },

    /// Reset configuration to defaults
    Reset,
}
