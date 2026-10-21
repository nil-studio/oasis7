//! Session-only imported Ed25519 credentials. Private material never enters this API.
use super::{digest, model::*, random_token, reject, service::*, store::StoreError};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::{Value, json};
#[path = "native_key_complete.rs"]
mod complete;
#[cfg(test)]
#[path = "native_key_tests.rs"]
mod tests;

pub(super) fn public_key(p: &Value) -> Result<[u8; 32], StoreError> {
    let raw = p["public_key"]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| reject("invalid_request"))?;
    let mut key = [0; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&raw[2 * i..2 * i + 2], 16)
            .map_err(|_| reject("invalid_request"))?;
    }
    VerifyingKey::from_bytes(&key).map_err(|_| reject("invalid_request"))?;
    Ok(key)
}
pub(super) fn lookup_key(key: &[u8; 32]) -> String {
    format!("lookup:native:{}", URL_SAFE_NO_PAD.encode(key))
}
#[expect(
    clippy::too_many_arguments,
    reason = "Protocol vector explicitly includes every domain-bound field"
)]
fn message(
    config: &super::config::IdentityConfig,
    audience: &str,
    purpose: &str,
    account: &str,
    id: &str,
    key: &[u8; 32],
    nonce: &[u8],
    now: u64,
    expires: u64,
) -> Result<Vec<u8>, StoreError> {
    use oasis7_agent_api::identity::{NativeIdentityChallengeV1, NativeIdentityPurposeV1};
    let purpose = match purpose {
        "login" => NativeIdentityPurposeV1::Login,
        "bind" => NativeIdentityPurposeV1::Bind,
        "reauth" => NativeIdentityPurposeV1::Reauthenticate,
        // Registration remains disabled at this legacy imported-key entry.
        _ => return Err(reject("invalid_request")),
    };
    NativeIdentityChallengeV1 {
        identity_service_id: &config.namespace.identity_service_id,
        environment_id: &config.namespace.environment_id,
        audience,
        purpose,
        account_context: account,
        operation_id: id,
        public_key: key,
        nonce: nonce.try_into().map_err(|_| StoreError::Corrupt)?,
        issued_at: now,
        expires_at: expires,
    }
    .signing_bytes()
    .map_err(|_| reject("invalid_request"))
}

fn pending_action(
    tx: &super::store::IdentityTransaction<'_>,
    id: &str,
    a: &AccountRecord,
    s: &AccountSession,
    now: u64,
) -> Result<super::store::Versioned<SensitiveAction>, StoreError> {
    let action = tx
        .get::<SensitiveAction>(&format!("action:{id}"))?
        .ok_or_else(|| reject("reauth_required"))?;
    if action.value.account_id != a.account_id
        || action.value.session_id != s.session_id
        || action.value.account_policy_version != a.policy_version
        || action.value.expires_at <= now
        || action.value.state != "pending"
    {
        return Err(reject("reauth_required"));
    }
    Ok(action)
}
fn native_credential(
    tx: &super::store::IdentityTransaction<'_>,
    key: &[u8; 32],
) -> Result<super::store::Versioned<CredentialRecord>, StoreError> {
    let lookup = tx
        .get::<CredentialLookup>(&lookup_key(key))?
        .ok_or_else(|| reject("authentication_failed"))?
        .value;
    let c = tx
        .get::<CredentialRecord>(&credential_key(&lookup.credential_record_id))?
        .ok_or_else(|| reject("authentication_failed"))?;
    if lookup.state != "active"
        || lookup.canonical_key.as_slice() != key.as_slice()
        || lookup.account_id != c.value.account_id
        || c.value.state != "active"
        || !matches!(c.value.public_material, CredentialMaterial::NativeKey { public_key } if public_key == *key)
    {
        return Err(reject("authentication_failed"));
    }
    Ok(c)
}
impl IdentityService {
    #[expect(
        clippy::too_many_arguments,
        reason = "Route context retains origin, owner, session and rate-limit boundaries"
    )]
    pub(super) fn native_key_route(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        owner: &str,
        p: &Value,
        source: &str,
        origin: &str,
        now: u64,
    ) -> Option<Result<(Value, Option<String>), StoreError>> {
        match (method, path) {
            ("POST", "/native-key/options") => Some(
                self.native_options(sid, owner, p, source, origin, now)
                    .map(|v| (v, None)),
            ),
            ("POST", "/native-key/complete") => {
                Some(self.native_complete(sid, owner, p, origin, now))
            }
            _ => None,
        }
    }
    fn native_options(
        &mut self,
        sid: Option<&str>,
        owner: &str,
        p: &Value,
        source: &str,
        origin: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        let key = public_key(p)?;
        let purpose = p["intent"].as_str().unwrap_or("login");
        if !["login", "bind", "reauth"].contains(&purpose) {
            return Err(reject("invalid_request"));
        }
        let session = if purpose != "login" {
            Some(self.session(sid, now)?)
        } else {
            None
        };
        let action_id = if purpose == "login" {
            None
        } else {
            Some(
                p["action_id"]
                    .as_str()
                    .filter(|s| s.len() == 43)
                    .ok_or_else(|| reject("reauth_required"))?
                    .to_owned(),
            )
        };
        let account = session.as_ref().map(|(a, _)| a.account_id.clone());
        let id = random_token();
        let nonce = random_token();
        let expires = now + 120_000;
        let bytes = message(
            &self.config,
            origin,
            purpose,
            account.as_deref().unwrap_or(""),
            &id,
            &key,
            &URL_SAFE_NO_PAD
                .decode(&nonce)
                .map_err(|_| StoreError::Corrupt)?,
            now,
            expires,
        )?;
        let options = json!({"identity_service_id":self.config.namespace.identity_service_id,"environment_id":self.config.namespace.environment_id,"audience":origin,"purpose":purpose,"account_context":account.as_deref().unwrap_or(""),"operation_id":id,"public_key":p["public_key"].as_str().unwrap().to_ascii_lowercase(),"nonce":nonce,"issued_at":now,"expires_at":expires,"signed_bytes":URL_SAFE_NO_PAD.encode(bytes)});
        let ceremony = CeremonyRecord {
            ceremony_id: id.clone(),
            kind: format!("native_{purpose}"),
            namespace: self.config.namespace.clone(),
            rp_config_version: self.config.rp.version,
            owner_context_digest: owner.into(),
            target_account_id: account.clone(),
            target_digest: action_id.clone(),
            expires_at: expires,
            failed_attempts: 0,
            status: "pending".into(),
            server_state: Some(options.clone()),
            result_ref: None,
        };
        let secret = self.config.session_key;
        self.store.transaction(|tx| {
            consume_quota(
                tx,
                if purpose == "bind" {
                    "register-source"
                } else {
                    "login-source"
                },
                &digest(&secret, "source", source),
                now,
                60_000,
                60,
            )?;
            consume_quota(
                tx,
                if purpose == "bind" {
                    "register-total"
                } else {
                    "login-total"
                },
                "deployment",
                now,
                60_000,
                300,
            )?;
            if let Some((a, s)) = &session {
                let action_id = action_id.as_deref().ok_or(StoreError::Corrupt)?;
                if purpose == "bind" {
                    authorized_action(tx, action_id, a, s, "bind", None, now)?;
                    if active_keys(tx, &a.account_id)?.len() >= 10 {
                        return Err(reject("credential_limit"));
                    }
                } else {
                    pending_action(tx, action_id, a, s, now)?;
                }
            }
            tx.create(
                &format!("ceremony:{id}"),
                "ceremony",
                account.as_deref(),
                Some(ceremony_storage_expiry(&ceremony)),
                &ceremony,
            )
        })?;
        Ok(
            json!({"api_version":2,"ok":true,"ceremony_id":id,"operation_id":id,"options":options,"expires_at":expires}),
        )
    }
}
