use crate::cli::Cli;
use crate::config::Config;
use crate::quota;
use crate::reader::UsageSource;
use crate::reader::claude::ClaudeReader;
use crate::reader::codex::CodexReader;
use crate::state::{AgentPaths, Platform};
use ratatui::style::Color;
use std::path::{Path, PathBuf};

/// Metadata and wiring for one supported agent platform. New platforms add a
/// single `RegistryEntry` here instead of touching `main.rs` match arms.
pub struct RegistryEntry {
    pub platform: Platform,
    pub config_key: &'static str,
    pub log_name: &'static str,
    pub label: &'static str,
    pub primary_color: Color,
    default_path_suffix: &'static str,
    is_available: fn(&Path) -> bool,
    config_path: fn(&Config) -> PathBuf,
    cli_path: fn(&Cli) -> Option<PathBuf>,
    set_config_path: fn(&mut Config, PathBuf),
    /// Log reader for this platform's local session files. `None` for
    /// platforms that expose quota/usage only via an API (e.g. Grok).
    create_reader: Option<fn(PathBuf) -> Box<dyn UsageSource>>,
    pub quota_fetcher: Option<quota::Fetcher>,
    pub account_fetcher: Option<quota::AccountFetcher>,
}

static REGISTRY: &[RegistryEntry] = &[
    RegistryEntry {
        platform: Platform::ClaudeCode,
        config_key: "claude_path",
        log_name: "Claude Code",
        label: "CLAUDE",
        primary_color: Color::Rgb(217, 119, 87),
        default_path_suffix: ".claude/projects",
        is_available: Path::exists,
        config_path: |c| c.claude_path.clone(),
        cli_path: |cli| cli.claude_path.clone(),
        set_config_path: |c, p| c.claude_path = p,
        create_reader: Some(|p| Box::new(ClaudeReader::new(p))),
        quota_fetcher: Some(quota::claude::fetch_quota),
        account_fetcher: None,
    },
    RegistryEntry {
        platform: Platform::Codex,
        config_key: "codex_path",
        log_name: "Codex",
        label: "CODEX",
        primary_color: Color::Rgb(59, 130, 246),
        default_path_suffix: ".codex",
        is_available: Path::exists,
        config_path: |c| c.codex_path.clone(),
        cli_path: |cli| cli.codex_path.clone(),
        set_config_path: |c, p| c.codex_path = p,
        create_reader: Some(|p| Box::new(CodexReader::new(p))),
        quota_fetcher: Some(quota::codex::fetch_quota),
        account_fetcher: None,
    },
    RegistryEntry {
        platform: Platform::Grok,
        config_key: "grok_path",
        log_name: "Grok",
        label: "GROK",
        primary_color: Color::Rgb(148, 163, 184),
        default_path_suffix: ".grok",
        is_available: Path::exists,
        config_path: |c| c.grok_path.clone(),
        cli_path: |cli| cli.grok_path.clone(),
        set_config_path: |c, p| c.grok_path = p,
        // No local session-log reader yet: Grok usage comes from the billing
        // API below. `~/.grok/sessions/**/updates.jsonl` exists but carries no
        // per-request cost, so a reader would fabricate $0 records.
        create_reader: None,
        quota_fetcher: Some(quota::grok::fetch_quota),
        account_fetcher: None,
    },
];

/// All registered platforms in UI order.
pub fn entries() -> &'static [RegistryEntry] {
    REGISTRY
}

/// Platforms to show in the TUI: platforms with a live reader, plus
/// quota-only platforms (no local log reader, e.g. Grok) whose data path
/// exists. `readers::PlatformReaders` only tracks platforms with readers, so
/// quota-only platforms must be added here or their panel never renders.
pub fn displayable_platforms(
    reader_platforms: impl IntoIterator<Item = Platform>,
    paths: &AgentPaths,
) -> Vec<Platform> {
    let mut set: std::collections::HashSet<_> = reader_platforms.into_iter().collect();
    for entry in REGISTRY {
        if entry.create_reader.is_none() && entry.is_available_at(&paths.path_for(entry.platform)) {
            set.insert(entry.platform);
        }
    }
    Platform::all()
        .iter()
        .copied()
        .filter(|p| set.contains(p))
        .collect()
}

/// Look up the registry row for a platform.
pub fn entry_for_platform(platform: Platform) -> &'static RegistryEntry {
    REGISTRY
        .iter()
        .find(|e| e.platform == platform)
        .expect("every Platform must have a RegistryEntry")
}

impl RegistryEntry {
    /// Construct a usage reader for this platform at `path`, or `None` for
    /// platforms with no local log format (quota-API-only).
    pub fn build_reader(&self, path: PathBuf) -> Option<Box<dyn UsageSource>> {
        self.create_reader.map(|f| f(path))
    }

    pub fn has_quota(&self) -> bool {
        self.quota_fetcher.is_some()
    }

    /// True if this platform has a local session-log reader (and therefore
    /// participates in the watcher/reader pipeline).
    pub fn has_reader(&self) -> bool {
        self.create_reader.is_some()
    }

    pub fn default_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(self.default_path_suffix)
    }

    pub fn is_available_at(&self, path: &Path) -> bool {
        (self.is_available)(path)
    }
}

/// Merge CLI overrides (when set) with config file values into resolved paths.
pub fn resolve_paths(cli: &Cli, config: &Config) -> AgentPaths {
    let paths = REGISTRY
        .iter()
        .map(|entry| {
            let path = (entry.cli_path)(cli).unwrap_or_else(|| (entry.config_path)(config));
            (entry.platform, path)
        })
        .collect();
    AgentPaths::new(paths)
}

/// Apply a `aum config set <key> <value>` update. Path keys are driven by the
/// registry; fallback `refresh` and `max_records` are handled separately.
pub fn apply_config_key(config: &mut Config, key: &str, value: &str) -> Result<(), String> {
    for entry in REGISTRY {
        if entry.config_key == key {
            (entry.set_config_path)(config, PathBuf::from(value));
            return Ok(());
        }
    }

    match key {
        "refresh" => {
            let refresh = value
                .parse()
                .map_err(|e: std::num::ParseIntError| e.to_string())?;
            if refresh == 0 {
                return Err("refresh must be at least 1 second".into());
            }
            config.refresh = refresh;
        }
        "max_records" => {
            config.max_records = value
                .parse()
                .map_err(|e: std::num::ParseIntError| e.to_string())?;
        }
        _ => {
            return Err(format_unknown_key(key));
        }
    }
    Ok(())
}

/// Comma-separated path keys plus global settings, for error messages.
pub fn config_key_list() -> String {
    let path_keys: Vec<_> = REGISTRY.iter().map(|e| e.config_key).collect();
    format!("{}, refresh, max_records", path_keys.join(", "))
}

fn format_unknown_key(key: &str) -> String {
    format!(
        "Unknown configuration key: {key}\nAvailable keys: {}",
        config_key_list()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_has_registry_entry() {
        for platform in Platform::all() {
            let entry = entry_for_platform(*platform);
            assert_eq!(entry.platform, *platform);
        }
    }

    #[test]
    fn config_keys_are_unique() {
        let keys: Vec<_> = REGISTRY.iter().map(|e| e.config_key).collect();
        let mut deduped = keys.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(keys.len(), deduped.len(), "duplicate config keys");
    }

    #[test]
    fn every_registered_platform_has_quota() {
        for platform in Platform::all() {
            let entry = entry_for_platform(*platform);
            assert!(entry.has_quota());
            assert!(entry.account_fetcher.is_none());
        }
    }

    #[test]
    fn apply_config_key_sets_path() {
        let mut config = Config::default();
        apply_config_key(&mut config, "codex_path", "/tmp/codex").unwrap();
        assert_eq!(config.codex_path, PathBuf::from("/tmp/codex"));
    }

    #[test]
    fn apply_config_key_rejects_unknown_or_zero_refresh() {
        let mut config = Config::default();
        assert!(apply_config_key(&mut config, "nope", "x").is_err());
        assert!(apply_config_key(&mut config, "refresh", "0").is_err());
        assert!(apply_config_key(&mut config, "cursor_path", "/tmp/x").is_err());
        assert!(apply_config_key(&mut config, "grok_path", "/tmp/grok").is_ok());
        assert_eq!(config.grok_path, PathBuf::from("/tmp/grok"));
    }

    #[test]
    fn resolve_paths_cli_overrides_config() {
        let config = Config::default();
        let cli = Cli {
            command: None,
            claude_path: Some(PathBuf::from("/cli/claude")),
            codex_path: None,
            grok_path: None,
            refresh: None,
        };
        let paths = resolve_paths(&cli, &config);
        assert_eq!(
            paths.path_for(Platform::ClaudeCode),
            PathBuf::from("/cli/claude")
        );
        assert_eq!(paths.path_for(Platform::Codex), config.codex_path);
    }
}
