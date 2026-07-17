//! Shared helpers for the quota submodules: time formatting and JWT decoding.
//!
//! Both `claude.rs` and `codex.rs` previously carried their own copy of
//! `format_duration_short`; the JWT helpers were only in `codex.rs` but live
//! here too so future OAuth work has a single place to extend.

use super::{QuotaError, QuotaInfo};
use serde_json::Value;
use std::time::Instant;

/// Build the `QuotaInfo` shape shared by every platform whose "quota" is just
/// a local-credentials presence check (no real usage API): signed in with an
/// email (or the generic "Signed in" placeholder), or `NoCredentials` when
/// none were found.
pub(crate) fn signed_in(tool_name: &str, email: Option<String>) -> Option<QuotaInfo> {
    Some(QuotaInfo {
        tool_name: tool_name.to_string(),
        account_id: None,
        windows: vec![],
        fetched_at: Instant::now(),
        error: if email.is_none() {
            Some(QuotaError::NoCredentials)
        } else {
            None
        },
        email,
    })
}

/// Extract `email` from a JWT payload, falling back to an email already read
/// from the credentials file. Shared by the four platforms (Copilot, Grok,
/// Factory, Antigravity) whose token is a JWT and whose local credentials
/// file may also carry an email/user field directly.
pub(crate) fn read_email(token: &str, file_email: Option<String>) -> Option<String> {
    decode_jwt_payload(token)
        .and_then(|payload| {
            payload
                .get("email")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .or(file_email)
}

/// Classify a quota API's `error` field into a `QuotaError`, but only when
/// the caller has no windows to show — a stray `error` key alongside valid
/// windows must NOT hide real quota (the UI renders the error arm before the
/// windows arm). `data.get("error")` is `Some` for an explicit `"error":
/// null` (a common success sentinel) too — filtered out here so it isn't
/// surfaced as a bogus parse error that would also force a re-fetch every
/// tick. Authentication errors are classified separately so the UI can
/// prompt a re-login. Shared by the Claude and Codex quota APIs, whose
/// response shapes agree on this `error` convention.
pub(crate) fn classify_api_error(data: &Value, windows_are_empty: bool) -> Option<QuotaError> {
    if !windows_are_empty {
        return None;
    }
    data.get("error").filter(|e| !e.is_null()).map(|e| {
        let error_type = e.get("type").and_then(|v| v.as_str());
        let message = e
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        if error_type == Some("authentication_error") {
            QuotaError::Auth(message)
        } else {
            QuotaError::Parse(format!("{}: {message}", error_type.unwrap_or("error")))
        }
    })
}

/// Render a duration in seconds as a compact human string:
/// `0` → `None`, `90` → `1m`, `3725` → `1h2m`, `90000` → `1d1h`.
pub(crate) fn format_duration_short(total_seconds: i64) -> Option<String> {
    if total_seconds <= 0 {
        return None;
    }

    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;

    if days > 0 {
        Some(format!("{days}d{hours}h"))
    } else if hours > 0 {
        Some(format!("{hours}h{minutes}m"))
    } else {
        Some(format!("{minutes}m"))
    }
}

/// Decode the payload section of a JWT **without** verifying its signature.
/// Accepts both URL-safe-no-pad and standard base64 (the two real-world
/// variants). Only the body is needed for reading claims like email or
/// `chatgpt_account_id` from the local auth token.
pub(crate) fn decode_jwt_payload(token: &str) -> Option<Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload = parts[1];
    let decoded = base64_decode(payload)?;
    serde_json::from_str(&decoded).ok()
}

fn base64_decode(input: &str) -> Option<String> {
    use base64::Engine;
    // JWT segments are base64url and usually unpadded. The previous version
    // hand-padded with '=' and then tried URL_SAFE_NO_PAD (which *rejects* '='
    // padding) and STANDARD (which uses the '+/' alphabet, not '-_'), so any
    // payload that both needed padding (len % 4 != 0) and contained '-'/'_'
    // failed in BOTH engines. Strip any padding and decode with the no-pad
    // engines, url-safe first, falling back to the standard '+/' alphabet.
    let trimmed = input.trim_end_matches('=');
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(trimmed)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed))
        .ok()?;
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_zero_or_negative_is_none() {
        assert_eq!(format_duration_short(0), None);
        assert_eq!(format_duration_short(-30), None);
    }

    #[test]
    fn duration_picks_largest_unit() {
        assert_eq!(format_duration_short(45), Some("0m".into())); // 0m is intentional
        assert_eq!(format_duration_short(90), Some("1m".into()));
        assert_eq!(format_duration_short(3_600), Some("1h0m".into()));
        assert_eq!(format_duration_short(3_725), Some("1h2m".into()));
        assert_eq!(format_duration_short(86_400), Some("1d0h".into()));
        assert_eq!(format_duration_short(86_400 + 3_600), Some("1d1h".into()));
    }

    #[test]
    fn jwt_decode_handles_url_safe_no_pad() {
        // header.payload.signature — payload is a tiny known string
        let token = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0In0.sig";
        let payload = decode_jwt_payload(token).expect("decodes");
        assert_eq!(payload.get("sub").and_then(|v| v.as_str()), Some("test"));
    }

    #[test]
    fn jwt_decode_rejects_malformed() {
        assert!(decode_jwt_payload("not-a-jwt").is_none());
        assert!(decode_jwt_payload("only.two").is_none()); // no signature is fine
        assert!(decode_jwt_payload(".").is_none());
    }

    #[test]
    fn base64_decode_handles_url_safe_alphabet_and_padding() {
        use base64::Engine;
        // Regression guard: a payload that both needs padding (len % 4 != 0)
        // and uses the url-safe '-'/'_' alphabet must still decode. "\u{FFFF}x"
        // encodes to a 6-char url-safe string containing '-' and '_'; the old
        // hand-padding + STANDARD-fallback path returned None for it.
        for s in [
            "x",
            "xy",
            "hello world",
            "a-b_c",
            "user+tag@x.io",
            "\u{FFFF}x",
        ] {
            let url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s);
            assert_eq!(
                base64_decode(&url).as_deref(),
                Some(s),
                "url-safe failed for {s:?}"
            );
            let std = base64::engine::general_purpose::STANDARD.encode(s);
            assert_eq!(
                base64_decode(&std).as_deref(),
                Some(s),
                "standard(padded) failed for {s:?}"
            );
        }
    }
}
