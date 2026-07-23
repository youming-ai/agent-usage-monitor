pub mod claude;
pub mod codex;
pub mod cursor;
pub mod pricing;

use crate::state::{Platform, UsageRecord};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Read one newline-terminated line as raw bytes and decode it lossily.
///
/// `BufRead::read_line` requires valid UTF-8 and, per its docs, leaves the
/// reader's position in an indeterminate state on error — every reader here
/// used to just `break` on that error without advancing its tracked byte
/// offset, so a single invalid-UTF-8 byte anywhere in a file would wedge it
/// forever: every subsequent poll re-reads up to that same byte and stops,
/// silently dropping every later record. `read_until` operates on raw bytes
/// and cannot fail on invalid UTF-8, so decoding lossily afterwards (bad
/// bytes become `U+FFFD`, which fails JSON parsing and is simply skipped by
/// each reader's `parse_line`) keeps the offset always advancing.
///
/// Returns `None` at true EOF or on an incomplete trailing line (left as-is
/// for the next poll to retry once it's been fully written).
pub(crate) fn read_next_line(reader: &mut impl BufRead) -> Option<(String, u64)> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf).ok()?;
    if n == 0 || !buf.ends_with(b"\n") {
        return None;
    }
    Some((String::from_utf8_lossy(&buf).into_owned(), n as u64))
}

/// Read newline-terminated lines from `path` starting at `skip_bytes`,
/// invoking `on_line` for each complete line (trailing `\r`/`\n` stripped).
/// Returns the records `on_line` produced and the byte offset just past the
/// last complete line consumed — an incomplete trailing line is left as-is
/// for the next poll to retry once it's been fully written.
///
/// If `skip_bytes` is past the current file length (the file was truncated
/// or rewritten since the offset was recorded), re-reads from byte 0 instead
/// — callers that need to reset per-file parse state when that happens
/// (e.g. a truncation-triggered rescan invalidating a running token-count
/// baseline) must detect it themselves before calling this (see cursor.rs,
/// grok.rs) since this function has no hook for that.
///
/// Every reader that tails a single JSONL/log file used to carry its own
/// copy of this open -> check-truncation -> seek -> read-loop block,
/// differing only in what each line's parse callback did with the line (and
/// what state it captured) — now the closure's job.
pub(crate) fn read_lines_from_offset(
    path: &Path,
    skip_bytes: u64,
    mut on_line: impl FnMut(&str) -> Option<UsageRecord>,
) -> (Vec<UsageRecord>, u64) {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (Vec::new(), skip_bytes),
    };

    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if skip_bytes > file_len {
        return read_lines_from_offset(path, 0, on_line);
    }

    let mut reader = BufReader::new(file);
    if skip_bytes > 0 && reader.seek(SeekFrom::Start(skip_bytes)).is_err() {
        return read_lines_from_offset(path, 0, on_line);
    }

    let mut records = Vec::new();
    let mut offset = skip_bytes;

    while let Some((line, bytes)) = read_next_line(&mut reader) {
        if let Some(rec) = on_line(line.trim_end_matches(['\r', '\n'])) {
            records.push(rec);
        }
        offset += bytes;
    }

    (records, offset)
}

/// Shared incremental-scan driver: owns the per-file byte-offset map plus an
/// arbitrary per-file state map `S` (session id, tracked model, running
/// totals, ...), and drives exactly the pattern every JSONL-tailing reader
/// used to duplicate by hand — drop offsets/state for files that vanished
/// since the last scan, look up (or lazily build via `init`) each remaining
/// file's offset and state, read new records via a caller-supplied
/// `read_file`, collect, and sort by timestamp.
pub(crate) struct FileScanner<S> {
    positions: HashMap<PathBuf, u64>,
    state: HashMap<PathBuf, S>,
}

impl<S> FileScanner<S> {
    pub(crate) fn new() -> Self {
        Self {
            positions: HashMap::new(),
            state: HashMap::new(),
        }
    }

    /// Drop every tracked offset and per-file state — call before a full
    /// re-read so every file starts again from byte 0 with fresh state.
    pub(crate) fn reset(&mut self) {
        self.positions.clear();
        self.state.clear();
    }

    /// (Re)scan `files`: for each, look up its last byte offset (0 if never
    /// seen) and its per-file state (built via `init` the first time a file
    /// is seen), read new records via `read_file`, and store back the
    /// updated offset/state. Offsets/state for files no longer present in
    /// `files` are dropped first. Returned records are sorted by timestamp.
    pub(crate) fn scan(
        &mut self,
        files: Vec<PathBuf>,
        mut init: impl FnMut(&Path) -> S,
        mut read_file: impl FnMut(&Path, u64, &mut S) -> (Vec<UsageRecord>, u64),
    ) -> Vec<UsageRecord> {
        let current_files: HashSet<PathBuf> = files.iter().cloned().collect();
        self.positions
            .retain(|path, _| current_files.contains(path));
        self.state.retain(|path, _| current_files.contains(path));

        let mut records = Vec::new();
        for file in files {
            let offset = self.positions.get(&file).copied().unwrap_or(0);
            let mut st = self.state.remove(&file).unwrap_or_else(|| init(&file));
            let (entries, bytes_read) = read_file(&file, offset, &mut st);
            self.positions.insert(file.clone(), bytes_read);
            self.state.insert(file, st);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }
}

/// Last non-empty path component, e.g. `/Users/me/repo` -> `repo`.
pub(crate) fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

/// True when `path` has an ancestor directory named `name`.
pub(crate) fn is_under_dir_named(path: &Path, name: &str) -> bool {
    path.ancestors()
        .any(|a| a.file_name().is_some_and(|n| n == name))
}

/// True when `stem` matches `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
pub(crate) fn is_uuid(stem: &str) -> bool {
    if stem.len() != 36 {
        return false;
    }
    let bytes = stem.as_bytes();
    const DASHES: [usize; 4] = [8, 13, 18, 23];
    bytes.iter().enumerate().all(|(i, &b)| {
        if DASHES.contains(&i) {
            b == b'-'
        } else {
            b.is_ascii_hexdigit()
        }
    })
}

/// Label for a single conversation: working-dir basename plus a short id
/// suffix so multiple sessions in the same directory stay distinct.
pub(crate) fn session_label(dir: &str, id: &str) -> String {
    // Take the first 8 *characters*, not bytes: byte-slicing `&id[..8]` panics
    // when byte 8 lands inside a multibyte char, and `id` is free-form data
    // read from on-disk JSONL (a panic here would poison the reader mutex).
    let short: String = id.chars().take(8).collect();
    if short.is_empty() {
        dir.to_string()
    } else {
        format!("{dir} {short}")
    }
}

/// Walk `dir` recursively, pushing files for which `keep(path)` returns true.
/// The two readers (claude/codex) used to ship near-identical copies of this
/// loop, differing only in the file filter — collapsed here behind a closure.
pub(crate) fn find_recursive(dir: &Path, files: &mut Vec<PathBuf>, keep: &dyn Fn(&Path) -> bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_recursive(&path, files, keep);
        } else if keep(&path) {
            files.push(path);
        }
    }
}

/// A source of usage records, abstracting over the backing store (JSONL files
/// for Claude/Codex, SQLite for opencode). `main.rs` drives every source the
/// same way: an initial `scan_all`, then a `poll_delta` loop. The file watcher
/// watches each platform's own resolved path directly (see `watcher.rs`) —
/// it does not go through this trait.
pub trait UsageSource: Send {
    fn platform(&self) -> Platform;
    fn scan_all(&mut self) -> Vec<UsageRecord>;
    fn poll_delta(&mut self) -> Vec<UsageRecord>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_label_handles_multibyte_id() {
        // 'é' is 2 bytes, so it straddles byte index 8 here; byte-slicing would
        // panic. Char-based truncation must keep the first 8 chars intact.
        let label = session_label("repo", "abcdefg\u{00e9}xyz");
        assert_eq!(label, "repo abcdefg\u{00e9}");
    }

    #[test]
    fn session_label_empty_id_is_dir_only() {
        assert_eq!(session_label("repo", ""), "repo");
    }

    #[test]
    fn basename_uses_path_file_name() {
        assert_eq!(basename("/Users/me/repo"), "repo");
        assert_eq!(basename("repo"), "repo");
    }
}
