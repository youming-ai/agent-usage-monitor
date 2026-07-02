//! Persistent per-platform usage readers for the TUI reader loop.
//!
//! Built **once** at startup and reused across every FS event and fallback
//! tick. An earlier revision rebuilt a reader per event
//! (`entry.build_reader(path)` inside the loop), which discarded each reader's
//! incremental state — byte offsets, SQLite cursors, dedup sets. With that
//! state gone, `poll_delta` degenerated into a full re-scan from zero and
//! `AppState::add_records` (which does not dedup) double-counted every record
//! on every tick. Keeping one reader alive per platform is the fix; the shared
//! `Mutex` also serializes a platform's concurrent event + fallback refreshes,
//! so they can no longer both full-scan in parallel.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use tracing::info;

use crate::platforms;
use crate::reader::UsageSource;
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
            let path = paths.path_for(entry.tab);
            if !path.exists() {
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

/// Full initial scan: read every record from scratch and merge into `state`.
/// Called once per platform at startup. Returns the number of records added.
pub fn scan_reader_into(
    reader: &SharedReader,
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
) -> usize {
    let records = match reader.lock() {
        Ok(mut r) => r.scan_all(),
        Err(_) => return 0,
    };
    merge(records, state, platform)
}

/// Incremental refresh: read only records appended since the last poll and
/// merge into `state`. Called on FS events and fallback ticks — it MUST reuse
/// the same reader instance across calls, or it re-scans from zero.
pub fn poll_reader_into(
    reader: &SharedReader,
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
) -> usize {
    let records = match reader.lock() {
        Ok(mut r) => r.poll_delta(),
        Err(_) => return 0,
    };
    merge(records, state, platform)
}

fn merge(
    records: Vec<crate::state::UsageRecord>,
    state: &Arc<RwLock<AppState>>,
    platform: Platform,
) -> usize {
    let n = records.len();
    if n == 0 {
        return 0;
    }
    info!("{:?}: Found {} new records", platform, n);
    if let Ok(mut s) = state.write() {
        s.add_records(platform, records);
    }
    n
}
