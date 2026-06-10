use super::QuotaInfo;

/// Reserved slot for Factory Droid CLI quota. Factory uses a BYOK model
/// (users bring their own API keys), so usage tracking is done at the
/// provider level (OpenAI/Anthropic). The `/cost` and `/stats` slash
/// commands in the CLI show local usage but there is no public REST API
/// for programmatic balance/usage queries.
#[allow(dead_code)]
pub fn fetch_quota() -> Option<QuotaInfo> {
    None
}
