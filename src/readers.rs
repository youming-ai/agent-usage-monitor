//! Persistent per-platform usage readers for the TUI reader loop.
//!
//! Built **once** at startup and reused across every FS event and fallback
//! tick. An earlier revision rebuilt a reader per event
//! (`entry.build_reader(path)` inside the loop), which discarded each reader's
//! incremental state — byte offsets, dedup sets. With that
//! state gone, `poll_delta` degenerated into a full re-scan from zero and
//! double-counted every record on every tick. Keeping one reader alive per
//! platform is the fix; the shared `Mutex` also protects direct callers.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use tokio::{sync::mpsc, task};
use tracing::{info, warn};

use crate::platforms;
use crate::reader::{ReaderError, ReaderResult, UsageSource};
use crate::state::{AgentPaths, AppState, Platform};

/// A reader shared between the reader task's per-platform blocking jobs. The
/// `Mutex` guarantees a single platform never scans concurrently with itself.
pub type SharedReader = Arc<Mutex<Box<dyn UsageSource>>>;

/// One live reader per registered platform whose data path currently exists.
pub struct PlatformReaders {
    readers: HashMap<Platform, SharedReader>,
}

impl PlatformReaders {
    /// Build a reader for every registered platform whose path exists now.
    /// Platforms whose path is absent are skipped and simply have no entry —
    /// `get` returns `None` for them until [`discover_new`] picks them up.
    ///
    /// [`discover_new`]: PlatformReaders::discover_new
    pub fn build(paths: &AgentPaths) -> Self {
        let mut this = Self {
            readers: HashMap::new(),
        };
        this.discover_new(paths);
        this
    }

    /// Build readers for any registered platform whose path exists now but has
    /// no reader yet, and return the platforms newly added. Callers should
    /// `scan_reader_into` (not `poll`) those, since a fresh reader starts with
    /// an empty cursor. Lets a platform whose data dir is created *after*
    /// launch (e.g. `aum` started before `~/.claude/projects` exists) be
    /// picked up on the next fallback tick instead of being ignored until
    /// restart.
    pub fn discover_new(&mut self, paths: &AgentPaths) -> Vec<Platform> {
        let mut added = Vec::new();
        for entry in platforms::entries() {
            if self.readers.contains_key(&entry.platform) {
                continue;
            }
            let path = paths.path_for(entry.platform);
            if !entry.is_available_at(&path) {
                continue;
            }
            self.readers.insert(
                entry.platform,
                Arc::new(Mutex::new(entry.build_reader(path))),
            );
            added.push(entry.platform);
        }
        added
    }

    /// Shared handle to one platform's reader, if it was built.
    pub fn get(&self, platform: Platform) -> Option<SharedReader> {
        self.readers.get(&platform).cloned()
    }

    /// All platforms that have a live reader, in arbitrary order.
    pub fn platforms(&self) -> Vec<Platform> {
        self.readers.keys().copied().collect()
    }
}

/// One bounded refresh queue per platform. A running poll can have one queued
/// follow-up; additional file events are coalesced because that poll reads all
/// appended data since the reader's last cursor.
pub struct ReaderRefreshers {
    state: Arc<RwLock<AppState>>,
    senders: HashMap<Platform, RefreshTrigger>,
}

struct RefreshTrigger {
    tx: mpsc::Sender<()>,
    pending: Arc<Mutex<PendingRefresh>>,
}

enum PendingRefresh {
    Full,
    Paths(HashSet<PathBuf>),
}

impl ReaderRefreshers {
    pub fn new(state: Arc<RwLock<AppState>>) -> Self {
        Self {
            state,
            senders: HashMap::new(),
        }
    }

    /// Start the refresh worker for a reader after its initial full scan.
    pub fn add(&mut self, platform: Platform, reader: SharedReader) {
        if self.senders.contains_key(&platform) {
            return;
        }

        let (tx, mut rx) = mpsc::channel(1);
        let state = self.state.clone();
        let pending = Arc::new(Mutex::new(PendingRefresh::Paths(HashSet::new())));
        let worker_pending = pending.clone();
        task::spawn(async move {
            while rx.recv().await.is_some() {
                let request = worker_pending
                    .lock()
                    .map(|mut pending| {
                        std::mem::replace(&mut *pending, PendingRefresh::Paths(HashSet::new()))
                    })
                    .unwrap_or(PendingRefresh::Full);
                let reader = reader.clone();
                let state = state.clone();
                let _ = task::spawn_blocking(move || match request {
                    PendingRefresh::Full => {
                        poll_reader_into(&reader, &state, platform);
                    }
                    PendingRefresh::Paths(paths) => {
                        let paths: Vec<_> = paths.into_iter().collect();
                        poll_changed_reader_into(&reader, &state, platform, &paths);
                    }
                })
                .await;
            }
        });
        self.senders
            .insert(platform, RefreshTrigger { tx, pending });
    }

    /// Request an incremental poll. A full queue already represents a pending
    /// poll, so dropping another event cannot lose appended records.
    pub fn request(&self, platform: Platform) {
        if let Some(trigger) = self.senders.get(&platform) {
            if let Ok(mut pending) = trigger.pending.lock() {
                *pending = PendingRefresh::Full;
            }
            let _ = trigger.tx.try_send(());
        }
    }

    /// Request a watcher-driven refresh narrowed to one changed path.
    pub fn request_changed(&self, platform: Platform, path: PathBuf) {
        if let Some(trigger) = self.senders.get(&platform) {
            if let Ok(mut pending) = trigger.pending.lock()
                && let PendingRefresh::Paths(paths) = &mut *pending
            {
                paths.insert(path);
            }
            let _ = trigger.tx.try_send(());
        }
    }
}

/// Full initial scan: read every record from scratch and merge into `state`.
/// Called once per platform at startup. Returns the number of records added.
pub fn scan_reader_into(
    reader: &SharedReader,
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
) -> usize {
    run_reader_operation(reader, state, platform, |reader| reader.scan_all())
}

/// Incremental refresh: read only records appended since the last poll and
/// merge into `state`. Called on FS events and fallback ticks — it MUST reuse
/// the same reader instance across calls, or it re-scans from zero.
pub fn poll_reader_into(
    reader: &SharedReader,
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
) -> usize {
    run_reader_operation(reader, state, platform, |reader| reader.poll_delta())
}

pub fn poll_changed_reader_into(
    reader: &SharedReader,
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
    paths: &[PathBuf],
) -> usize {
    run_reader_operation(reader, state, platform, |reader| reader.poll_changed(paths))
}

fn run_reader_operation(
    reader: &SharedReader,
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
    operation: impl FnOnce(&mut dyn UsageSource) -> ReaderResult<Vec<crate::state::UsageRecord>>,
) -> usize {
    let (result, tool_ops) = match reader.lock() {
        Ok(mut r) => {
            let result = operation(r.as_mut());
            let ops = r.take_tool_ops_delta();
            (result, ops)
        }
        Err(_) => (
            Err(ReaderError::new("reader lock poisoned")),
            crate::state::ToolOps::default(),
        ),
    };
    apply_result(result, tool_ops, state, platform)
}

fn apply_result(
    result: ReaderResult<Vec<crate::state::UsageRecord>>,
    tool_ops: crate::state::ToolOps,
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
) -> usize {
    let records = match result {
        Ok(records) => {
            if let Ok(mut s) = state.write() {
                s.mark_reader_success(platform);
            }
            records
        }
        Err(error) => {
            warn!("{platform:?} reader failed: {error}");
            if let Ok(mut s) = state.write() {
                s.mark_reader_error(platform, error.to_string());
            }
            return 0;
        }
    };
    let n = records.len();
    if n > 0 {
        info!("{:?}: Found {} new records", platform, n);
    }
    if let Ok(mut s) = state.write() {
        if n > 0 {
            s.add_records(platform, records);
        }
        s.add_tool_ops(platform, tool_ops);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc as std_mpsc,
    };
    use std::time::Duration;

    struct BlockingSource {
        polls: Arc<AtomicUsize>,
        started: mpsc::UnboundedSender<usize>,
        release: Option<std_mpsc::Receiver<()>>,
    }

    struct FailingSource;
    struct HintSource {
        paths: mpsc::UnboundedSender<Vec<PathBuf>>,
    }

    impl UsageSource for FailingSource {
        fn scan_all(&mut self) -> ReaderResult<Vec<crate::state::UsageRecord>> {
            Err(ReaderError::new("fixture read failure"))
        }

        fn poll_delta(&mut self) -> ReaderResult<Vec<crate::state::UsageRecord>> {
            Err(ReaderError::new("fixture read failure"))
        }
    }

    impl UsageSource for HintSource {
        fn scan_all(&mut self) -> ReaderResult<Vec<crate::state::UsageRecord>> {
            Ok(Vec::new())
        }

        fn poll_delta(&mut self) -> ReaderResult<Vec<crate::state::UsageRecord>> {
            Ok(Vec::new())
        }

        fn poll_changed(
            &mut self,
            paths: &[PathBuf],
        ) -> ReaderResult<Vec<crate::state::UsageRecord>> {
            let _ = self.paths.send(paths.to_vec());
            Ok(Vec::new())
        }
    }

    impl UsageSource for BlockingSource {
        fn scan_all(&mut self) -> ReaderResult<Vec<crate::state::UsageRecord>> {
            Ok(Vec::new())
        }

        fn poll_delta(&mut self) -> ReaderResult<Vec<crate::state::UsageRecord>> {
            let poll = self.polls.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = self.started.send(poll);
            if let Some(release) = self.release.take() {
                release.recv().unwrap();
            }
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn refresh_requests_coalesce_while_polling() {
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();
        let (release_tx, release_rx) = std_mpsc::channel();
        let polls = Arc::new(AtomicUsize::new(0));
        let reader: SharedReader = Arc::new(Mutex::new(Box::new(BlockingSource {
            polls: polls.clone(),
            started: started_tx,
            release: Some(release_rx),
        })));
        let mut refreshers = ReaderRefreshers::new(Arc::new(RwLock::new(AppState::new())));
        refreshers.add(Platform::ClaudeCode, reader);

        refreshers.request(Platform::ClaudeCode);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
                .await
                .expect("first poll should start")
                .expect("worker should stay alive"),
            1
        );

        refreshers.request(Platform::ClaudeCode);
        refreshers.request(Platform::ClaudeCode);
        refreshers.request(Platform::ClaudeCode);
        release_tx.send(()).unwrap();

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
                .await
                .expect("one coalesced poll should start")
                .expect("worker should stay alive"),
            2
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), started_rx.recv())
                .await
                .is_err(),
            "extra queued refreshes must be coalesced"
        );
        assert_eq!(polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn reader_failures_are_visible_in_platform_state() {
        let state = Arc::new(RwLock::new(AppState::new()));
        let reader: SharedReader = Arc::new(Mutex::new(Box::new(FailingSource)));

        assert_eq!(scan_reader_into(&reader, &state, Platform::ClaudeCode), 0);
        assert_eq!(
            state
                .read()
                .unwrap()
                .platform(Platform::ClaudeCode)
                .reader_error
                .as_deref(),
            Some("fixture read failure")
        );
    }

    #[tokio::test]
    async fn watcher_paths_reach_the_reader() {
        let (paths_tx, mut paths_rx) = mpsc::unbounded_channel();
        let reader: SharedReader = Arc::new(Mutex::new(Box::new(HintSource { paths: paths_tx })));
        let mut refreshers = ReaderRefreshers::new(Arc::new(RwLock::new(AppState::new())));
        refreshers.add(Platform::ClaudeCode, reader);

        let changed = PathBuf::from("/tmp/session.jsonl");
        refreshers.request_changed(Platform::ClaudeCode, changed.clone());

        let received = tokio::time::timeout(Duration::from_secs(1), paths_rx.recv())
            .await
            .expect("targeted poll should run")
            .expect("worker should stay alive");
        assert_eq!(received, vec![changed]);
    }
}
