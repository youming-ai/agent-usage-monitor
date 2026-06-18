pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod factory;
pub mod grok;
pub mod hermes;
pub mod jsonl_reader;
pub mod kimi_code;
pub mod mimo_code;
pub mod openclaw;
pub mod opencode;
pub mod pi;
pub mod pricing;
pub(crate) mod session_jsonl;
pub(crate) mod sqlite_message_reader;

use crate::state::{Platform, UsageRecord};
use jsonl_reader::JsonlReader;
use std::fs;
use std::path::{Path, PathBuf};

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

/// True when the file stem matches `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
pub(crate) fn is_uuid_jsonl(path: &Path) -> bool {
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return false,
    };
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
/// same way: an initial `scan_all`, then a `poll_delta` loop.
pub trait UsageSource: Send {
    fn platform(&self) -> Platform;
    fn scan_all(&mut self) -> Vec<UsageRecord>;
    fn poll_delta(&mut self) -> Vec<UsageRecord>;
}

impl UsageSource for claude::ClaudeReader {
    fn platform(&self) -> Platform {
        Platform::ClaudeCode
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        JsonlReader::scan_all(self)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        JsonlReader::poll_delta(self)
    }
}

impl UsageSource for codex::CodexReader {
    fn platform(&self) -> Platform {
        Platform::Codex
    }
    fn scan_all(&mut self) -> Vec<UsageRecord> {
        JsonlReader::scan_all(self)
    }
    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        JsonlReader::poll_delta(self)
    }
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

    #[test]
    fn is_uuid_jsonl_matches_session_files() {
        use std::path::Path;
        assert!(is_uuid_jsonl(Path::new(
            "a3f2c1d8-10e5-4b2a-9c1d-ef0123456789.jsonl"
        )));
        assert!(!is_uuid_jsonl(Path::new("session.jsonl")));
    }
}
