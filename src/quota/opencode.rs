use super::QuotaInfo;

/// Reserved slot for opencode-go (Zen/Go) quota. There is no public
/// balance/usage API today — see github.com/anomalyco/opencode issues
/// #16017 (Go plan usage) and #10448 (Zen balance). When one ships, build a
/// `QuotaInfo` here and call this from the quota task in `main.rs`.
#[allow(dead_code)]
pub fn fetch_quota() -> Option<QuotaInfo> {
    None
}
