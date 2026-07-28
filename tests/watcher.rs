//! Integration tests for the FS watcher.
//!
//! Spawn the watcher against a real tempdir and verify file changes
//! produce `WatcherMessage::Event` on the channel.

use std::fs;
use std::time::Duration;

use agent_usage_monitor::state::AgentPaths;

use agent_usage_monitor::platforms;

use agent_usage_monitor::watcher::{WatcherMessage, start_watchers};

fn synthetic_paths(root: &std::path::Path) -> AgentPaths {
    let mut map = std::collections::HashMap::new();
    for entry in platforms::entries() {
        map.insert(entry.platform, root.to_path_buf());
    }
    AgentPaths::new(map)
}

#[tokio::test]
async fn watcher_detects_new_file_in_tempdir() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let paths = synthetic_paths(tmp.path());
    let (_watchers, mut rx) = start_watchers(&paths);

    tokio::time::sleep(Duration::from_millis(100)).await;

    let f = tmp.path().join("new.jsonl");
    fs::write(&f, b"{\"type\": \"test\"}\n").expect("write");

    let result = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match rx.recv().await {
                Some(WatcherMessage::Event { path, .. }) if path.ends_with("new.jsonl") => {
                    return true;
                }
                Some(_) => continue,
                None => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(result, "expected Event for new.jsonl within 2s");
}

#[tokio::test]
async fn watcher_detects_modify() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let f = tmp.path().join("modify.jsonl");
    fs::write(&f, b"initial\n").expect("write initial");

    let paths = synthetic_paths(tmp.path());
    let (_watchers, mut rx) = start_watchers(&paths);
    tokio::time::sleep(Duration::from_millis(100)).await;

    fs::write(&f, b"modified\n").expect("write modify");

    let result = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match rx.recv().await {
                Some(WatcherMessage::Event { path, .. }) if path.ends_with("modify.jsonl") => {
                    return true;
                }
                Some(_) => continue,
                None => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(result, "expected Event for modify.jsonl within 2s");
}
