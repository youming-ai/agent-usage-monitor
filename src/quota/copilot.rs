use super::QuotaInfo;

/// Reserved slot for GitHub Copilot CLI quota. There is no public
/// balance/usage API today — premium request consumption is tracked
/// server-side by GitHub's billing system.
#[allow(dead_code)]
pub fn fetch_quota() -> Option<QuotaInfo> {
    None
}
