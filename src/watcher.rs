//! File-system watcher for TUI reader tasks.
//!
//! Replaces fixed polling in `main.rs` with `notify` events + 50ms debounce
//! per platform. A configurable fallback poll runs alongside as a safety net
//! for FS edge cases.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use tokio::sync::mpsc;
use tracing::warn;

use crate::platforms;
use crate::state::{AgentPaths, Platform};

#[derive(Debug, Clone)]
pub enum WatcherMessage {
    Event { platform: Platform, path: PathBuf },
}

/// The inner `Debouncer` is wrapped in `Option` so `Drop` can call `stop()`
/// and join its background thread instead of leaving FSEvents callbacks in
/// flight on macOS.
struct PlatformWatcher {
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl PlatformWatcher {
    fn new(
        platform: Platform,
        path: PathBuf,
        tx: mpsc::Sender<WatcherMessage>,
        recent: Arc<Mutex<HashMap<(Platform, PathBuf), Instant>>>,
    ) -> Option<Self> {
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
                                should_send(&mut map, platform, path, now)
                            };
                            if should_send {
                                // This callback runs on an OS watcher thread.
                                // If the bounded channel is full, coalesce the
                                // event; the configured fallback poll catches it.
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
            Ok(debouncer) => debouncer,
            Err(e) => {
                warn!("Failed to create watcher for {platform:?}: {e}");
                return None;
            }
        };

        if let Err(e) = debouncer.watch(&path, RecursiveMode::Recursive) {
            warn!("Failed to watch {path:?}: {e}");
            return None;
        }

        Some(Self {
            debouncer: Some(debouncer),
        })
    }
}

impl Drop for PlatformWatcher {
    fn drop(&mut self) {
        if let Some(d) = self.debouncer.take() {
            d.stop();
        }
    }
}

/// Live watchers plus the sender state needed to add one when a platform's
/// data directory appears after startup.
pub struct PlatformWatchers {
    watchers: HashMap<Platform, PlatformWatcher>,
    tx: mpsc::Sender<WatcherMessage>,
    recent: Arc<Mutex<HashMap<(Platform, PathBuf), Instant>>>,
}

impl PlatformWatchers {
    /// Watch an existing platform path once. Failed registrations are not
    /// retained, so the fallback loop can retry them later.
    pub fn watch_platform(&mut self, platform: Platform, path: PathBuf) {
        if self.watchers.contains_key(&platform) || !path.exists() {
            return;
        }
        if let Some(watcher) =
            PlatformWatcher::new(platform, path, self.tx.clone(), self.recent.clone())
        {
            self.watchers.insert(platform, watcher);
        }
    }
}

/// Return false only when this platform already emitted this exact path inside
/// the debounce window. Different platforms sharing a directory must each
/// receive a refresh request.
fn should_send(
    recent: &mut HashMap<(Platform, PathBuf), Instant>,
    platform: Platform,
    path: &Path,
    now: Instant,
) -> bool {
    recent.retain(|_, last| now.duration_since(*last) < Duration::from_secs(5));
    let key = (platform, path.to_path_buf());
    if recent
        .get(&key)
        .is_some_and(|last| now.duration_since(*last) < Duration::from_millis(50))
    {
        return false;
    }
    recent.insert(key, now);
    true
}

pub fn start_watchers(paths: &AgentPaths) -> (PlatformWatchers, mpsc::Receiver<WatcherMessage>) {
    let (tx, rx) = mpsc::channel(64);
    let mut watchers = PlatformWatchers {
        watchers: HashMap::with_capacity(platforms::entries().len()),
        tx,
        recent: Arc::new(Mutex::new(HashMap::new())),
    };
    for entry in platforms::entries() {
        // Quota-only platforms (no local log reader) have nothing to watch;
        // registering them would flood the event channel with session writes
        // that no reader consumes.
        if !entry.has_reader() {
            continue;
        }
        watchers.watch_platform(entry.platform, paths.path_for(entry.platform));
    }
    (watchers, rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    /// Build synthetic paths whose every registered platform points at `root`.
    /// This lets us exercise `start_watchers` without touching user data.
    fn synthetic_paths(root: &std::path::Path) -> AgentPaths {
        let mut map = std::collections::HashMap::new();
        for entry in platforms::entries() {
            map.insert(entry.platform, root.to_path_buf());
        }
        AgentPaths::new(map)
    }

    #[test]
    fn dedup_keeps_events_for_distinct_platforms() {
        let path = PathBuf::from("/tmp/aum-shared-path");
        let now = Instant::now();
        let mut recent = HashMap::new();
        assert!(should_send(&mut recent, Platform::ClaudeCode, &path, now));
        assert!(!should_send(
            &mut recent,
            Platform::ClaudeCode,
            &path,
            now + Duration::from_millis(1)
        ));
        assert!(should_send(
            &mut recent,
            Platform::Codex,
            &path,
            now + Duration::from_millis(1)
        ));
    }

    #[tokio::test]
    async fn start_watchers_skips_unavailable_platforms() {
        let paths = synthetic_paths(std::path::Path::new(
            "/tmp/aum_test_definitely_does_not_exist_xyz",
        ));
        let (watchers, _rx) = start_watchers(&paths);
        assert_eq!(watchers.watchers.len(), 0, "no path exists => no watchers");
    }

    #[tokio::test]
    async fn start_watchers_creates_watcher_per_available_platform() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = synthetic_paths(tmp.path());
        let (watchers, _rx) = start_watchers(&paths);
        // Platforms without a local reader are not watched.
        let reader_platforms = crate::platforms::entries()
            .iter()
            .filter(|e| e.has_reader())
            .count();
        assert_eq!(watchers.watchers.len(), reader_platforms);
    }

    #[tokio::test]
    async fn watch_platform_adds_a_path_created_after_startup() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let later = tmp.path().join("appears-later");
        let mut paths = std::collections::HashMap::new();
        for entry in platforms::entries() {
            paths.insert(
                entry.platform,
                tmp.path()
                    .join(format!("missing-{}", entry.platform.label())),
            );
        }
        paths.insert(Platform::ClaudeCode, later.clone());
        let paths = AgentPaths::new(paths);
        let (mut watchers, mut rx) = start_watchers(&paths);
        assert_eq!(watchers.watchers.len(), 0);

        fs::create_dir_all(&later).expect("create later path");
        watchers.watch_platform(Platform::ClaudeCode, later.clone());
        assert_eq!(watchers.watchers.len(), 1);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let file = later.join("new.jsonl");
        fs::write(&file, b"hello\n").expect("write file");

        let saw_event = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(message) = rx.recv().await {
                if matches!(
                    message,
                    WatcherMessage::Event { platform: Platform::ClaudeCode, path }
                        if path.ends_with("new.jsonl")
                ) {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap_or(false);
        assert!(saw_event, "newly registered platform must receive events");
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
        let mut counts = HashMap::new();
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Some(WatcherMessage::Event { platform, .. })) => {
                    *counts.entry(platform).or_insert(0) += 1;
                }
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        assert!(
            counts.values().all(|&count| count <= 3),
            "5 writes should debounce to <= 3 events per platform, got {counts:?}"
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
            matches!(r, Ok(None) | Err(_)),
            "after dropping watchers, no file events should arrive"
        );
    }
}
