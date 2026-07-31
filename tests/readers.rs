//! Regression test for the persistent-reader fix.
//!
//! Before the fix, the TUI reader loop rebuilt a reader per FS event, so each
//! refresh re-scanned from zero and `add_records` double-counted. This proves
//! that reusing one reader across a startup scan + a later poll does not
//! re-add records that were already counted.

use agent_usage_monitor::platforms;
use agent_usage_monitor::readers::{PlatformReaders, poll_reader_into, scan_reader_into};
use agent_usage_monitor::state::{AgentPaths, AppState, Platform};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Every platform pointed at `p`; caller overrides the one(s) it cares about.
fn all_pointing_at(p: PathBuf) -> HashMap<Platform, PathBuf> {
    platforms::entries()
        .iter()
        .map(|e| (e.platform, p.clone()))
        .collect()
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn reused_reader_does_not_double_count_on_refresh() {
    // Only Claude points at a real fixture (2 assistant records); every other
    // tab points at a path that does not exist, so no reader is built for it.
    let mut map = all_pointing_at(PathBuf::from("/nonexistent/aum/definitely/not/here"));
    map.insert(Platform::ClaudeCode, fixtures_root().join("claude"));
    let paths = AgentPaths::new(map);

    let readers = PlatformReaders::build(&paths);
    assert_eq!(readers.platforms(), vec![Platform::ClaudeCode]);

    let state = Arc::new(RwLock::new(AppState::with_capacity(10_000)));

    // Startup: full scan picks up the two fixture records.
    for p in readers.platforms() {
        let reader = readers.get(p).expect("reader was built");
        scan_reader_into(&reader, &state, p);
    }
    assert_eq!(
        state
            .read()
            .unwrap()
            .platform(Platform::ClaudeCode)
            .window_calls,
        2
    );

    // A later refresh with no new data (FS event / fallback tick) must add
    // nothing. The pre-fix code rebuilt the reader here and re-scanned to 4.
    for p in readers.platforms() {
        let reader = readers.get(p).expect("reader was built");
        poll_reader_into(&reader, &state, p);
    }
    assert_eq!(
        state
            .read()
            .unwrap()
            .platform(Platform::ClaudeCode)
            .window_calls,
        2,
        "refresh re-added already-counted records"
    );
}

#[test]
fn discover_new_picks_up_paths_created_after_launch() {
    // Point Claude at a dir that does not exist yet; all others stay absent.
    let tmp = tempfile::tempdir().expect("tempdir");
    let claude_dir = tmp.path().join("claude-appears-later");
    let mut map = all_pointing_at(tmp.path().join("nope"));
    map.insert(Platform::ClaudeCode, claude_dir.clone());
    let paths = AgentPaths::new(map);

    // At launch nothing exists -> no readers built.
    let mut readers = PlatformReaders::build(&paths);
    assert!(readers.platforms().is_empty());

    // The path appears after launch; a fallback tick discovers it exactly once.
    std::fs::create_dir_all(&claude_dir).expect("create claude dir");
    assert_eq!(readers.discover_new(&paths), vec![Platform::ClaudeCode]);
    assert_eq!(readers.platforms(), vec![Platform::ClaudeCode]);
    assert!(
        readers.discover_new(&paths).is_empty(),
        "an already-built platform must not be rediscovered"
    );
}
