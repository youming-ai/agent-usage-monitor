use crate::cli::Cli;
use crate::config::Config;
use crate::reader::UsageSource;
use crate::reader::antigravity::AntigravityReader;
use crate::reader::claude::ClaudeReader;
use crate::reader::codex::CodexReader;
use crate::reader::copilot::CopilotReader;
use crate::reader::cursor::CursorReader;
use crate::reader::factory::FactoryReader;
use crate::reader::grok::GrokReader;
use crate::reader::hermes::HermesReader;
use crate::reader::kimi_code::KimiCodeReader;
use crate::reader::openclaw::OpenClawReader;
use crate::reader::pi::PiReader;
use crate::reader::sqlite_message_reader::SqliteMessageReader;
use crate::state::{AgentPaths, Platform, Tab};
use std::path::PathBuf;

/// Metadata and wiring for one supported agent platform. New platforms add a
/// single `RegistryEntry` here instead of touching `main.rs` match arms.
pub struct RegistryEntry {
    pub tab: Tab,
    pub platform: Platform,
    pub config_key: &'static str,
    pub log_name: &'static str,
    config_path: fn(&Config) -> PathBuf,
    cli_path: fn(&Cli) -> Option<PathBuf>,
    set_config_path: fn(&mut Config, PathBuf),
    create_reader: fn(PathBuf) -> Box<dyn UsageSource>,
}

static REGISTRY: &[RegistryEntry] = &[
    RegistryEntry {
        tab: Tab::ClaudeCode,
        platform: Platform::ClaudeCode,
        config_key: "claude_path",
        log_name: "Claude Code",
        config_path: |c| c.claude_path.clone(),
        cli_path: |cli| cli.claude_path.clone(),
        set_config_path: |c, p| c.claude_path = p,
        create_reader: |p| Box::new(ClaudeReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::Codex,
        platform: Platform::Codex,
        config_key: "codex_path",
        log_name: "Codex",
        config_path: |c| c.codex_path.clone(),
        cli_path: |cli| cli.codex_path.clone(),
        set_config_path: |c, p| c.codex_path = p,
        create_reader: |p| Box::new(CodexReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::OpenClaw,
        platform: Platform::OpenClaw,
        config_key: "openclaw_path",
        log_name: "OpenClaw",
        config_path: |c| c.openclaw_path.clone(),
        cli_path: |cli| cli.openclaw_path.clone(),
        set_config_path: |c, p| c.openclaw_path = p,
        create_reader: |p| Box::new(OpenClawReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::Hermes,
        platform: Platform::Hermes,
        config_key: "hermes_path",
        log_name: "Hermes",
        config_path: |c| c.hermes_path.clone(),
        cli_path: |cli| cli.hermes_path.clone(),
        set_config_path: |c, p| c.hermes_path = p,
        create_reader: |p| Box::new(HermesReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::OpenCode,
        platform: Platform::OpenCode,
        config_key: "opencode_path",
        log_name: "opencode",
        config_path: |c| c.opencode_path.clone(),
        cli_path: |cli| cli.opencode_path.clone(),
        set_config_path: |c, p| c.opencode_path = p,
        create_reader: |p| {
            Box::new(SqliteMessageReader::new(
                p,
                "opencode.db",
                Platform::OpenCode,
                "opencode",
            ))
        },
    },
    RegistryEntry {
        tab: Tab::KimiCode,
        platform: Platform::KimiCode,
        config_key: "kimi_code_path",
        log_name: "Kimi Code",
        config_path: |c| c.kimi_code_path.clone(),
        cli_path: |cli| cli.kimi_code_path.clone(),
        set_config_path: |c, p| c.kimi_code_path = p,
        create_reader: |p| Box::new(KimiCodeReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::Pi,
        platform: Platform::Pi,
        config_key: "pi_path",
        log_name: "Pi",
        config_path: |c| c.pi_path.clone(),
        cli_path: |cli| cli.pi_path.clone(),
        set_config_path: |c, p| c.pi_path = p,
        create_reader: |p| Box::new(PiReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::Factory,
        platform: Platform::Factory,
        config_key: "factory_path",
        log_name: "Factory",
        config_path: |c| c.factory_path.clone(),
        cli_path: |cli| cli.factory_path.clone(),
        set_config_path: |c, p| c.factory_path = p,
        create_reader: |p| Box::new(FactoryReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::Grok,
        platform: Platform::Grok,
        config_key: "grok_path",
        log_name: "Grok",
        config_path: |c| c.grok_path.clone(),
        cli_path: |cli| cli.grok_path.clone(),
        set_config_path: |c, p| c.grok_path = p,
        create_reader: |p| Box::new(GrokReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::Cursor,
        platform: Platform::Cursor,
        config_key: "cursor_path",
        log_name: "Cursor CLI",
        config_path: |c| c.cursor_path.clone(),
        cli_path: |cli| cli.cursor_path.clone(),
        set_config_path: |c, p| c.cursor_path = p,
        create_reader: |p| Box::new(CursorReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::Copilot,
        platform: Platform::Copilot,
        config_key: "copilot_path",
        log_name: "Copilot CLI",
        config_path: |c| c.copilot_path.clone(),
        cli_path: |cli| cli.copilot_path.clone(),
        set_config_path: |c, p| c.copilot_path = p,
        create_reader: |p| Box::new(CopilotReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::Antigravity,
        platform: Platform::Antigravity,
        config_key: "antigravity_path",
        log_name: "Antigravity CLI",
        config_path: |c| c.antigravity_path.clone(),
        cli_path: |cli| cli.antigravity_path.clone(),
        set_config_path: |c, p| c.antigravity_path = p,
        create_reader: |p| Box::new(AntigravityReader::new(p)),
    },
    RegistryEntry {
        tab: Tab::MimoCode,
        platform: Platform::MimoCode,
        config_key: "mimo_code_path",
        log_name: "MiMo Code",
        config_path: |c| c.mimo_code_path.clone(),
        cli_path: |cli| cli.mimo_code_path.clone(),
        set_config_path: |c, p| c.mimo_code_path = p,
        create_reader: |p| {
            Box::new(SqliteMessageReader::new(
                p,
                "mimocode.db",
                Platform::MimoCode,
                "mimocode",
            ))
        },
    },
];

/// All registered platforms in tab order.
pub fn entries() -> &'static [RegistryEntry] {
    REGISTRY
}

/// Look up the registry row for a tab.
pub fn entry_for_tab(tab: Tab) -> &'static RegistryEntry {
    REGISTRY
        .iter()
        .find(|e| e.tab == tab)
        .expect("every Tab must have a RegistryEntry")
}

impl RegistryEntry {
    /// Construct a usage reader for this platform at `path`.
    pub fn build_reader(&self, path: PathBuf) -> Box<dyn UsageSource> {
        (self.create_reader)(path)
    }
}

/// Merge CLI overrides (when set) with config file values into resolved paths.
pub fn resolve_paths(cli: &Cli, config: &Config) -> AgentPaths {
    let paths = REGISTRY
        .iter()
        .map(|entry| {
            let path = (entry.cli_path)(cli).unwrap_or_else(|| (entry.config_path)(config));
            (entry.tab, path)
        })
        .collect();
    AgentPaths::new(paths)
}

/// Apply a `aum config set <key> <value>` update. Path keys are driven by the
/// registry; `refresh` and `max_records` are handled separately.
pub fn apply_config_key(config: &mut Config, key: &str, value: &str) -> Result<(), String> {
    for entry in REGISTRY {
        if entry.config_key == key {
            (entry.set_config_path)(config, PathBuf::from(value));
            return Ok(());
        }
    }

    match key {
        "refresh" => {
            config.refresh = value
                .parse()
                .map_err(|e: std::num::ParseIntError| e.to_string())?;
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
    fn every_tab_has_registry_entry() {
        for tab in Tab::all() {
            let entry = entry_for_tab(*tab);
            assert_eq!(entry.tab, *tab);
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
    fn apply_config_key_sets_path() {
        let mut config = Config::default();
        apply_config_key(&mut config, "grok_path", "/tmp/grok").unwrap();
        assert_eq!(config.grok_path, PathBuf::from("/tmp/grok"));
    }

    #[test]
    fn apply_config_key_rejects_unknown() {
        let mut config = Config::default();
        assert!(apply_config_key(&mut config, "nope", "x").is_err());
    }

    #[test]
    fn resolve_paths_cli_overrides_config() {
        let config = Config::default();
        let cli = Cli {
            command: None,
            claude_path: Some(PathBuf::from("/cli/claude")),
            codex_path: None,
            opencode_path: None,
            kimi_code_path: None,
            pi_path: None,
            openclaw_path: None,
            hermes_path: None,
            factory_path: None,
            grok_path: None,
            cursor_path: None,
            copilot_path: None,
            antigravity_path: None,
            mimo_code_path: None,
            refresh: None,
        };
        let paths = resolve_paths(&cli, &config);
        assert_eq!(
            paths.path_for(Tab::ClaudeCode),
            PathBuf::from("/cli/claude")
        );
        assert_eq!(paths.path_for(Tab::Codex), config.codex_path);
    }
}
