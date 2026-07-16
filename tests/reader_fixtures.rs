//! Integration tests that scan committed on-disk fixtures. Catches silent
//! regressions when upstream agents change their log formats.

use agent_usage_monitor::platforms;
use agent_usage_monitor::reader::UsageSource;
use agent_usage_monitor::reader::antigravity::AntigravityReader;
use agent_usage_monitor::reader::claude::ClaudeReader;
use agent_usage_monitor::reader::codex::CodexReader;
use agent_usage_monitor::reader::copilot::CopilotReader;
use agent_usage_monitor::reader::cursor::CursorReader;
use agent_usage_monitor::reader::factory::FactoryReader;
use agent_usage_monitor::reader::grok::GrokReader;
use agent_usage_monitor::state::resolve;
use agent_usage_monitor::state::{Platform, Tab};
use std::path::PathBuf;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn claude_fixture_parses_assistant_records() {
    let mut reader = ClaudeReader::new(fixtures_root().join("claude"));
    let records = reader.scan_all();
    assert_eq!(records.len(), 2, "expected two assistant lines");
    assert_eq!(records[0].platform, Platform::ClaudeCode);
    assert_eq!(resolve(records[0].model), "claude-opus-4");
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
    assert_eq!(records[0].cost_usd, 0.01);
    assert_eq!(resolve(records[0].session), "agent-usage-monitor a3f2c1d8");
    assert!(reader.poll_delta().is_empty());
}

#[test]
fn codex_fixture_parses_token_count() {
    let mut reader = CodexReader::new(fixtures_root().join("codex"));
    let records = reader.scan_all();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].platform, Platform::Codex);
    assert_eq!(resolve(records[0].model), "gpt-5.4");
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
}

#[test]
fn factory_fixture_parses_uuid_session() {
    let mut reader = FactoryReader::new(fixtures_root().join("factory"));
    let records = reader.scan_all();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].platform, Platform::Factory);
    assert_eq!(resolve(records[0].model), "claude-sonnet-4-5");
    assert_eq!(records[0].input_tokens, 1200);
    assert_eq!(resolve(records[0].session), "project abc123");
}

#[test]
fn grok_fixture_parses_prompt_deltas() {
    let mut reader = GrokReader::new(fixtures_root().join("grok"));
    let records = reader.scan_all();
    // A full scan flushes the trailing prompt too (C11 fix) — it's no longer
    // left uncounted forever just because no third prompt arrived.
    assert_eq!(records.len(), 2, "completed prompt plus the trailing one");
    assert_eq!(records[0].platform, Platform::Grok);
    assert_eq!(resolve(records[0].model), "grok-build");
    assert_eq!(records[0].input_tokens, 1500);
    assert_eq!(records[1].input_tokens, 1000);
    assert_eq!(resolve(records[0].session), "project 019ea524");
}

#[test]
fn cursor_fixture_parses_transcript_turns() {
    let mut reader = CursorReader::new(fixtures_root().join("cursor"));
    let records = reader.scan_all();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].platform, Platform::Cursor);
    assert!(records[0].input_tokens > 0);
    assert!(records[0].output_tokens > 0);
    assert_eq!(resolve(records[0].session), "myproject a3f2c1d8");
}

#[test]
fn copilot_fixture_parses_events() {
    let mut reader = CopilotReader::new(fixtures_root().join("copilot"));
    let records = reader.scan_all();
    // One tool.execution_complete + one session.compaction_complete
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].platform, Platform::Copilot);
    assert_eq!(resolve(records[0].model), "gpt-4.1");
    assert_eq!(records[1].input_tokens, 52000); // 50000 + 2000
    assert_eq!(resolve(records[1].session), "myproject abc12345");
}

#[test]
fn antigravity_fixture_parses_transcript() {
    let mut reader = AntigravityReader::new(fixtures_root().join("antigravity"));
    let records = reader.scan_all();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].platform, Platform::Antigravity);
    assert_eq!(resolve(records[0].model), "gemini-3");
    assert!(records[0].output_tokens > 0);
    assert_eq!(records[0].input_tokens, 0);
}

#[test]
fn registry_creates_readers_for_all_fixture_paths() {
    for entry in platforms::entries() {
        let path = fixtures_root().join(entry.config_key.trim_end_matches("_path"));
        if !path.exists() {
            continue;
        }
        let mut reader = entry.build_reader(path);
        let _ = reader.scan_all();
        assert_eq!(reader.platform(), entry.platform);
    }
}

#[test]
fn registry_covers_all_tabs() {
    let tabs: Vec<_> = platforms::entries().iter().map(|e| e.tab).collect();
    for tab in Tab::all() {
        assert!(tabs.contains(tab), "missing registry entry for {tab:?}");
    }
}
