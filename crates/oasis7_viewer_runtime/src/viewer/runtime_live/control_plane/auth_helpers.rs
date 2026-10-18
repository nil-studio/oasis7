pub(super) fn hosted_strong_auth_grant_public_key_from_env() -> Result<String, String> {
    std::env::var(super::HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "hosted strong auth backend grant signer is not configured on this runtime".to_string()
        })
}

pub(super) fn hosted_strong_auth_now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(in crate::viewer::runtime_live) fn map_auth_verify_error_code(message: &str) -> &'static str {
    if message.contains("nonce") {
        return "auth_nonce_invalid";
    }
    if message.contains("signature") || message.contains("awviewauth:v1") {
        return "auth_signature_invalid";
    }
    if message.contains("player_id") || message.contains("public_key") {
        return "auth_claim_mismatch";
    }
    if message.contains("required") || message.contains("empty") {
        return "auth_claim_invalid";
    }
    "auth_invalid"
}
