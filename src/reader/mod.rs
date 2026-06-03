pub mod claude;
pub mod codex;
pub mod jsonl_reader;
pub mod pricing;

use std::fs;
use std::path::{Path, PathBuf};

/// Last non-empty path component, e.g. `/Users/me/repo` -> `repo`.
pub(crate) fn basename(path: &str) -> String {
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string()
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
}
