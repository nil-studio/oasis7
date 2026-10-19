//! Short-lived identity authorization. The runtime never reads the identity database.
use super::*;
use sha2::Digest;

pub const HOSTED_IDENTITY_V2_ENABLED_ENV: &str = "OASIS7_HOSTED_IDENTITY_V2_ENABLED";
pub const HOSTED_LEGACY_REGISTRATION_ENABLED_ENV: &str =
    "OASIS7_HOSTED_LEGACY_REGISTRATION_ENABLED";
pub const HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY_ENV: &str =
    "OASIS7_HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY";
const PREFIX: &str = "hosted-auth-v2";
pub const HOSTED_AUTHORIZATION_MAX_AGE_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedAuthContextV2 {
    pub version: u8,
    pub identity_service_id: String,
    pub environment_id: String,
    pub identity_instance_epoch: u64,
    pub account_id: String,
    pub account_session_id: String,
    pub credential_record_id: String,
    pub credential_version: u64,
    pub account_security_epoch: u64,
    pub auth_method: String,
    pub auth_time: u64,
    pub recovery_context: Option<String>,
    pub world_identity_scope: String,
    pub world_id: String,
    pub player_id: String,
    pub device_session_id: String,
    pub session_public_key: String,
    pub world_session_epoch: u64,
    pub capabilities: Vec<String>,
    pub checked_at: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub nonce: String,
    pub issuer_key_id: String,
}

pub fn hosted_identity_v2_enabled_checked() -> Result<bool, String> {
    match std::env::var(HOSTED_IDENTITY_V2_ENABLED_ENV).as_deref() {
        Ok("1") => Ok(true),
        Ok("0" | "false" | "") | Err(std::env::VarError::NotPresent) => Ok(false),
        _ => Err(format!(
            "invalid identity v2 flag: {HOSTED_IDENTITY_V2_ENABLED_ENV} must be exactly 1, 0, false, or empty"
        )),
    }
}

pub fn hosted_identity_v2_enabled() -> bool {
    // Startup rejects invalid settings; standalone runtime checks must also
    // fail closed instead of silently re-enabling the legacy lane.
    hosted_identity_v2_enabled_checked().unwrap_or(true)
}

pub fn require_legacy_registration_opt_in() -> Result<(), String> {
    if std::env::var(HOSTED_IDENTITY_V2_ENABLED_ENV).is_ok()
        && !std::env::var(HOSTED_LEGACY_REGISTRATION_ENABLED_ENV).is_ok_and(|value| value == "1")
    {
        return Err("hosted legacy registration disabled".into());
    }
    Ok(())
}

impl HostedAuthContextV2 {
    pub fn validate_at(&self, now: u64) -> Result<(), String> {
        if self.version != 2
            || self.checked_at > self.issued_at
            || self.issued_at >= self.expires_at
            || self.expires_at.saturating_sub(self.checked_at) > HOSTED_AUTHORIZATION_MAX_AGE_MS
            || now < self.issued_at
            || now >= self.expires_at
        {
            return Err("hosted_authorization_expired: invalid freshness window".into());
        }
        for field in [
            &self.identity_service_id,
            &self.environment_id,
            &self.account_id,
            &self.account_session_id,
            &self.credential_record_id,
            &self.world_identity_scope,
            &self.world_id,
            &self.player_id,
            &self.device_session_id,
            &self.nonce,
            &self.issuer_key_id,
        ] {
            if field.trim().is_empty()
                || field.trim() != field
                || field.len() > 256
                || field.chars().any(char::is_control)
            {
                return Err("hosted_authorization_invalid: invalid binding".into());
            }
        }
        normalize_public_key_field(&self.session_public_key, "authorization session key")?;
        if self.world_session_epoch == 0
            || (self.capabilities != ["player_session"]
                && self.capabilities != ["player_session_release"])
            || !matches!(self.auth_method.as_str(), "webauthn" | "email")
            || self.auth_time > self.checked_at
        {
            return Err("hosted_authorization_invalid: unsupported authority".into());
        }
        Ok(())
    }
}

/// Call only after an authoritative current account/credential/session read.
/// `checked_at` is that read's start time, not a cache hit or signing time.
pub fn issue_hosted_authorization_grant(
    context: &HostedAuthContextV2,
    issuer_private_key_hex: &str,
) -> Result<String, String> {
    context.validate_at(context.issued_at)?;
    let bytes = serde_json::to_vec(context).map_err(|_| "encode hosted authorization failed")?;
    let mut signing_bytes = format!("{PREFIX}:").into_bytes();
    signing_bytes.extend_from_slice(&bytes);
    let key = signing_key_from_hex(issuer_private_key_hex, "authorization issuer private key")?;
    Ok(format!(
        "{PREFIX}.{}.{}",
        hex::encode(bytes),
        hex::encode(key.sign(&signing_bytes).to_bytes())
    ))
}

fn configured(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| format!("hosted_authorization_configuration_missing: {name}"))
}

pub fn verify_hosted_authorization_grant_signature(
    grant: &str,
    player: &str,
    public_key: &str,
) -> Result<HostedAuthContextV2, String> {
    let context = verify_grant_signature(
        grant,
        &configured(HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY_ENV)?,
        now_unix_ms(),
    )?;
    if context.player_id != player
        || context.session_public_key != public_key
        || format!(
            "{}:{}:{}",
            context.identity_service_id.len(),
            context.identity_service_id,
            context.environment_id
        ) != configured("OASIS7_HOSTED_IDENTITY_NAMESPACE")?
        || context.world_id != configured("OASIS7_HOSTED_WORLD_ID")?
        || context.world_identity_scope != configured("OASIS7_HOSTED_WORLD_IDENTITY_SCOPE")?
        || context.identity_instance_epoch.to_string()
            != configured("OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH")?
    {
        return Err("hosted_authorization_binding_mismatch".into());
    }
    Ok(context)
}

pub fn authorization_nonce(context: &HostedAuthContextV2) -> String {
    let bytes = serde_json::to_vec(&(
        &context.identity_service_id,
        &context.environment_id,
        context.identity_instance_epoch,
        &context.nonce,
    ))
    .expect("serialize authorization nonce tuple");
    format!(
        "hosted-auth-v2:{}",
        hex::encode(sha2::Sha256::digest(bytes))
    )
}

pub fn verify_hosted_release_grant(grant: &str) -> Result<HostedAuthContextV2, String> {
    let context = verify_grant_signature(
        grant,
        &configured(HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY_ENV)?,
        now_unix_ms(),
    )?;
    let context = verify_hosted_authorization_grant_signature(
        grant,
        &context.player_id,
        &context.session_public_key,
    )?;
    if context.capabilities != ["player_session_release"] {
        return Err("hosted release authorization required".into());
    }
    Ok(context)
}

fn verify_grant_signature(
    grant: &str,
    trusted_key: &str,
    now: u64,
) -> Result<HostedAuthContextV2, String> {
    if grant.len() > 16_384 {
        return Err("hosted authorization oversized".into());
    }
    let mut parts = grant.split('.');
    if parts.next() != Some(PREFIX) {
        return Err("hosted authorization v2 required".into());
    }
    let bytes = hex::decode(parts.next().ok_or("authorization payload missing")?)
        .map_err(|_| "invalid authorization payload")?;
    let signature = hex::decode(parts.next().ok_or("authorization signature missing")?)
        .map_err(|_| "invalid authorization signature")?;
    if parts.next().is_some() {
        return Err("invalid authorization format".into());
    }
    let public = decode_hex_array::<32>(trusted_key, "authorization issuer public key")?;
    let key = VerifyingKey::from_bytes(&public).map_err(|_| "invalid authorization issuer")?;
    let mut signing_bytes = format!("{PREFIX}:").into_bytes();
    signing_bytes.extend_from_slice(&bytes);
    key.verify(
        &signing_bytes,
        &Signature::from_slice(&signature).map_err(|_| "invalid authorization signature")?,
    )
    .map_err(|_| "authorization signature rejected")?;
    let context: HostedAuthContextV2 =
        serde_json::from_slice(&bytes).map_err(|_| "invalid authorization context")?;
    context.validate_at(now)?;
    Ok(context)
}

#[cfg(test)]
#[path = "hosted_authorization_tests.rs"]
pub mod tests;
