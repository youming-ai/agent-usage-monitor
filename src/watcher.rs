//! File-system watcher for TUI reader tasks.
//!
//! Replaces the 1s `tokio::time::interval` polling in `main.rs` with
//! `notify` events + 50ms debounce per platform. A 30s fallback poll
//! runs alongside as a safety net for FS edge cases.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use tokio::sync::mpsc;
use tracing::warn;

use crate::platforms;
use crate::state::{AgentPaths, Platform, Tab};

#[derive(Debug, Clone)]
pub enum WatcherMessage {
    Event { platform: Platform, path: PathBuf },
}

/// One watcher per registered platform.
///
/// The inner `Debouncer` is wrapped in `Option` so the `Drop` impl can
/// take ownership and call `stop()` (which joins the background thread)
/// — the default `Debouncer::drop` only sets a stop flag without
/// joining, which leaves FSEvents callbacks in flight on macOS.
pub struct PlatformWatcher {
    platform: Platform,
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl PlatformWatcher {
    pub fn platform(&self) -> Platform {
        self.platform
    }
}

impl Drop for PlatformWatcher {
    fn drop(&mut self) {
        if let Some(d) = self.debouncer.take() {
            d.stop();
        }
    }
}

// Per-thread keep-alive for the mpsc sender. When all `PlatformWatcher`s
thread_local! {
    static KEEP_ALIVE: RefCell<Option<mpsc::Sender<WatcherMessage>>> =
        const { RefCell::new(None) };
}

use std::cell::RefCell;

pub fn start_watchers(
    paths: &AgentPaths,
) -> (Vec<PlatformWatcher>, mpsc::Receiver<WatcherMessage>) {
    let (tx, rx) = mpsc::channel::<WatcherMessage>(64);
    // Park one sender clone in thread-local storage so dropping every
    // PlatformWatcher does not close the channel.
    KEEP_ALIVE.with(|cell| {
        *cell.borrow_mut() = Some(tx.clone());
    });
    // Cross-watcher dedup: when multiple platforms resolve to the same
    // directory (e.g. tests, or a user pointing two readers at one path),
    // collapse duplicate events for the same path within the debounce
    // window so downstream consumers see one event per logical change.
    let recent: Arc<Mutex<HashMap<PathBuf, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut watchers = Vec::with_capacity(14);

    for entry in platforms::entries() {
        let path = paths.path_for(entry.tab);
        if !path.exists() {
            continue;
        }
        // Every platform watches exactly its own resolved path (a directory
        // for JSONL readers, the db file itself for SQLite ones) — no need to
        // build a full reader (opening SQLite connections and all) just to
        // ask it; `watch_dirs` used to come from `UsageSource::get_watch_directories`,
        // which every impl just echoed back from this same `path`.
        let watch_dirs = [path.clone()];

        let is_sqlite = matches!(entry.tab, Tab::OpenCode | Tab::Hermes | Tab::MimoCode);
        let tx = tx.clone();
        let platform = entry.platform;
        let recent = recent.clone();
        // Suppress FSEvents "watch started" events that fire during
        // initialization. The first real test event arrives after the
        // 100ms test sleep, so a 50ms warmup is safe.
        let created = Instant::now();

        let debouncer = new_debouncer(
            Duration::from_millis(50),
            None,
            move |res: DebounceEventResult| {
                if let Ok(events) = res {
                    if Instant::now().duration_since(created) < Duration::from_millis(50) {
                        return;
                    }
                    let now = Instant::now();
                    for event in events {
                        for path in &event.paths {
                            let should_send = {
                                let mut map = recent.lock().expect("dedup lock");
                                // Prune stale entries opportunistically — well
                                // beyond the 50ms dedup window below, so this
                                // never affects dedup behavior, but keeps
                                // `recent` from growing unbounded over a
                                // long-running session touching many distinct
                                // paths.
                                map.retain(|_, last| {
                                    now.duration_since(*last) < Duration::from_secs(5)
                                });
                                match map.get(path) {
                                    Some(last)
                                        if now.duration_since(*last)
                                            < Duration::from_millis(50) =>
                                    {
                                        false
                                    }
                                    _ => {
                                        map.insert(path.clone(), now);
                                        true
                                    }
                                }
                            };
                            if should_send {
                                // try_send, not blocking_send: this callback
                                // runs on the OS's FSEvents thread. If the
                                // receiver stalls and the 64-slot channel
                                // fills up, blocking here would stall the OS
                                // callback thread itself — drop/coalesce the
                                // event instead (a 30s fallback poll exists
                                // as a safety net for exactly this).
                                let _ = tx.try_send(WatcherMessage::Event {
                                    platform,
                                    path: path.clone(),
                                });
                            }
                        }
                    }
                }
            },
        );
        let mut debouncer = match debouncer {
            Ok(d) => d,
            Err(e) => {
                warn!(
                    "Failed to create watcher for {:?}: {e}; skipping platform",
                    platform
                );
                continue;
            }
        };

        for dir in &watch_dirs {
            let candidate = if is_sqlite && dir.is_file() {
                dir.parent().unwrap_or(dir)
            } else {
                dir.as_path()
            };
            let Some(watch_path) = candidate
                .ancestors()
                .find(|p| p.exists())
                .map(|p| p.to_path_buf())
            else {
                warn!("No existing ancestor for {candidate:?}; skipping watch dir");
                continue;
            };
            if let Err(e) = debouncer.watch(&watch_path, RecursiveMode::Recursive) {
                warn!("Failed to watch {watch_path:?}: {e}; skipping watch dir");
                continue;
            }
        }

        watchers.push(PlatformWatcher {
            platform,
            debouncer: Some(debouncer),
        });
    }

    (watchers, rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    /// Build a synthetic AgentPaths whose every registered Tab points at
    /// `root`. This lets us exercise `start_watchers` without touching the
    /// user's real data directories.
    fn synthetic_paths(root: &std::path::Path) -> AgentPaths {
        let mut map = std::collections::HashMap::new();
        for entry in platforms::entries() {
            map.insert(entry.tab, root.to_path_buf());
        }
        AgentPaths::new(map)
    }

    #[tokio::test]
    async fn start_watchers_skips_unavailable_platforms() {
        let paths = synthetic_paths(std::path::Path::new(
            "/tmp/aum_test_definitely_does_not_exist_xyz",
        ));
        let (watchers, _rx) = start_watchers(&paths);
        assert_eq!(watchers.len(), 0, "no path exists => no watchers");
    }

    #[tokio::test]
    async fn start_watchers_creates_watcher_per_available_platform() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = synthetic_paths(tmp.path());
        let (watchers, _rx) = start_watchers(&paths);
        assert_eq!(watchers.len(), 14);
    }

    #[tokio::test]
    async fn start_watchers_emits_event_on_file_create() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = synthetic_paths(tmp.path());
        let (_watchers, mut rx) = start_watchers(&paths);

        tokio::time::sleep(Duration::from_millis(100)).await;
        let f = tmp.path().join("test.jsonl");
        fs::write(&f, b"hello\n").expect("write file");

        let msg = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("channel closed");
        match msg {
            WatcherMessage::Event { path, .. } => {
                assert!(path.ends_with("test.jsonl") || path.to_string_lossy().contains("test"));
            }
        }
    }

    #[tokio::test]
    async fn start_watchers_debounces_burst() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = synthetic_paths(tmp.path());
        let (_watchers, mut rx) = start_watchers(&paths);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let f = tmp.path().join("burst.jsonl");
        for i in 0..5 {
            fs::write(&f, format!("line {i}\n")).expect("write");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        let mut count = 0;
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Some(WatcherMessage::Event { .. })) => count += 1,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        assert!(
            count <= 3,
            "5 writes should debounce to <= 3 events, got {count}"
        );
    }

    #[tokio::test]
    async fn platform_watcher_drop_stops_watching() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = synthetic_paths(tmp.path());
        let (watchers, mut rx) = start_watchers(&paths);

        drop(watchers);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let f = tmp.path().join("after_drop.jsonl");
        fs::write(&f, b"after drop\n").expect("write");

        let r = tokio::time::timeout(Duration::from_millis(300), rx.recv()).await;
        assert!(
            r.is_err(),
            "after dropping watchers, no file events should arrive"
        );
    }
}
