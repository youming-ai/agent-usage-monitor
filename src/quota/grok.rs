use super::QuotaInfo;

/// Reserved slot for Grok Build (xAI) quota. xAI provides inference APIs
/// only — there is no public billing, credits, or usage endpoint. Credits
/// and billing are managed exclusively through the xAI console.
#[allow(dead_code)]
pub fn fetch_quota() -> Option<QuotaInfo> {
    None
}
