//! Integration tests that scan committed on-disk fixtures. Catches silent
//! regressions when upstream agents change their log formats.

use agent_usage_monitor::platforms;
use agent_usage_monitor::reader::UsageSource;
use agent_usage_monitor::reader::claude::ClaudeReader;
use agent_usage_monitor::reader::codex::CodexReader;
use agent_usage_monitor::reader::cursor::CursorReader;
use agent_usage_monitor::reader::pi::PiReader;
use agent_usage_monitor::state::Platform;
use agent_usage_monitor::state::resolve;
use std::path::PathBuf;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn claude_fixture_parses_assistant_records() {
    let mut reader = ClaudeReader::new(fixtures_root().join("claude"));
    let records = reader.scan_all().unwrap();
    assert_eq!(records.len(), 2, "expected two assistant lines");
    assert_eq!(resolve(records[0].model), "claude-opus-4");
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
    assert_eq!(records[0].cost_usd, 0.01);
    assert_eq!(resolve(records[0].session), "agent-usage-monitor a3f2c1d8");
    // Full identifiers must survive for session resume, not just the label.
    assert_eq!(resolve(records[0].session_id), "a3f2c1d8");
    assert_eq!(resolve(records[0].cwd), "/Users/me/agent-usage-monitor");
    assert!(reader.poll_delta().unwrap().is_empty());
}

#[test]
fn codex_fixture_parses_token_count() {
    let mut reader = CodexReader::new(fixtures_root().join("codex"));
    let records = reader.scan_all().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(resolve(records[0].model), "gpt-5.4");
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
}

#[test]
fn cursor_fixture_parses_transcript_turns() {
    let mut reader = CursorReader::new(fixtures_root().join("cursor"));
    let records = reader.scan_all().unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].input_tokens > 0);
    assert!(records[0].output_tokens > 0);
    assert_eq!(resolve(records[0].session), "myproject a3f2c1d8");
}

#[test]
fn pi_fixture_parses_assistant_messages() {
    let mut reader = PiReader::new(fixtures_root().join("pi"));
    let records = reader.scan_all().unwrap();
    assert_eq!(records.len(), 1, "expected one assistant line");
    assert_eq!(resolve(records[0].model), "claude-sonnet-4-5");
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
    assert_eq!(records[0].cache_read_tokens, 10);
    assert_eq!(records[0].cache_creation_tokens, 5);
    assert_eq!(resolve(records[0].session), "agent-usage-monitor a3f2c1d8");
    assert_eq!(resolve(records[0].session_id), "a3f2c1d8");
    assert_eq!(resolve(records[0].cwd), "/Users/me/agent-usage-monitor");
    assert!(reader.poll_delta().unwrap().is_empty());
}

/// Records each platform's fixture must yield — see the per-reader tests above.
/// Matched exhaustively so a new platform can't be added without stating it.
fn expected_fixture_records(platform: Platform) -> usize {
    match platform {
        Platform::ClaudeCode => 2,
        Platform::Codex => 1,
        Platform::Pi => 1,
        Platform::Cursor => 1,
    }
}

/// A registry row wiring the wrong reader type (e.g. `Platform::Codex` built
/// with `ClaudeReader::new`) compiles and parses nothing, filing one agent's
/// usage under another. `UsageRecord` no longer carries a platform to check,
/// so bind the row to its reader by what that reader actually parses.
#[test]
fn registry_rows_build_the_reader_that_parses_their_own_fixture() {
    let mut checked = 0;
    for entry in platforms::entries() {
        let path = fixtures_root().join(entry.config_key.trim_end_matches("_path"));
        if !path.exists() {
            continue;
        }
        let mut reader = entry.build_reader(path);
        let records = reader.scan_all().unwrap();
        assert_eq!(
            records.len(),
            expected_fixture_records(entry.platform),
            "{:?}'s registry row does not build a reader that parses {:?} logs",
            entry.platform,
            entry.platform
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        Platform::all().len(),
        "every platform needs a fixture for this to bind anything"
    );
}

#[test]
fn registry_covers_all_platforms() {
    let platforms: Vec<_> = platforms::entries().iter().map(|e| e.platform).collect();
    for platform in Platform::all() {
        assert!(
            platforms.contains(platform),
            "missing registry entry for {platform:?}"
        );
    }
}
