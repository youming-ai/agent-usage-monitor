pub mod claude;
pub mod codex;
pub mod pricing;

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
    let short = &id[..id.len().min(8)];
    if short.is_empty() {
        dir.to_string()
    } else {
        format!("{dir} {short}")
    }
}
