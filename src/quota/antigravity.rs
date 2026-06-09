use super::QuotaInfo;

/// Reserved slot for Google Antigravity CLI quota. Token usage quotas are
/// tracked server-side by Google's CloudCode backend with no public API.
/// The `/usage` command inside the CLI shows per-model quota remaining.
#[allow(dead_code)]
pub fn fetch_quota() -> Option<QuotaInfo> {
    None
}
