use super::QuotaInfo;

/// Reserved slot for Kimi Code quota. There is no public balance/usage API
/// today. When one ships, build a `QuotaInfo` here and call this from the
/// quota task in `main.rs`.
#[allow(dead_code)]
pub fn fetch_quota() -> Option<QuotaInfo> {
    None
}
