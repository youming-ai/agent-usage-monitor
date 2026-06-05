use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "aum")]
#[command(version)]
#[command(about = "Real-time Claude Code, Codex, opencode & Kimi Code usage monitor")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to Claude Code data directory
    #[arg(long)]
    pub claude_path: Option<PathBuf>,

    /// Path to Codex data directory
    #[arg(long)]
    pub codex_path: Option<PathBuf>,

    /// Path to opencode data directory
    #[arg(long)]
    pub opencode_path: Option<PathBuf>,

    /// Path to Kimi Code data directory
    #[arg(long)]
    pub kimi_code_path: Option<PathBuf>,

    /// Path to pi data directory
    #[arg(long)]
    pub pi_path: Option<PathBuf>,

    /// Path to openclaw data directory
    #[arg(long)]
    pub openclaw_path: Option<PathBuf>,

    /// Path to hermes-agent data directory
    #[arg(long)]
    pub hermes_path: Option<PathBuf>,

    /// Path to Factory AI data directory
    #[arg(long)]
    pub factory_path: Option<PathBuf>,

    /// Polling interval in seconds
    #[arg(short, long)]
    pub refresh: Option<u64>,
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
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key (e.g., claude_path, codex_path, opencode_path, refresh, max_records)
        key: String,

        /// Configuration value
        value: String,
    },

    /// Reset configuration to defaults
    Reset,
}
