use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "usage-monitor")]
#[command(version)]
#[command(about = "Real-time Claude Code & Codex usage monitor")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to Claude Code data directory
    #[arg(long, default_value_os_t = default_claude_path())]
    pub claude_path: PathBuf,

    /// Path to Codex data directory
    #[arg(long, default_value_os_t = default_codex_path())]
    pub codex_path: PathBuf,

    /// Polling interval in seconds
    #[arg(short, long, default_value_t = 5)]
    pub refresh: u64,
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
}

fn default_claude_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude/projects")
}

fn default_codex_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
}
