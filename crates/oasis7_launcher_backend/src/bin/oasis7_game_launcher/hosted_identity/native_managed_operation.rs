//! Stable managed login IDs. Pending challenges expire; only successful PoP creates a permanent tombstone.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Outcome {
    operation_id: String,
    owner_context_digest: String,
    origin: String,
    namespace: IdentityNamespace,
    identity_instance_epoch: u64,
    public_key: [u8; 32],
    purpose: String,
    scope_digest: String,
    complete_digest: String,
    account_id: String,
    credential_ref: String,
    committed_at: u64,
}
fn outcome_key(id: &str) -> String {
    format!("native_managed_login_outcome:{id}")
}
pub(super) fn requested<'a>(
    service: &IdentityService,
    payload: &'a Value,
    purpose: &str,
) -> Result<Option<&'a str>, StoreError> {
    let Some(value) = payload.get("managed_operation_id") else {
        return Ok(None);
    };
    if let Some(code) = service.managed_adapter_configuration_error() {
        return Err(reject(code));
    }
    if !service.managed_login_adapter_enabled()
        || purpose != "login"
        || payload["intent"].as_str() != Some("login")
    {
        return Err(reject("invalid_request"));
    }
    let id = value
        .as_str()
        .filter(|v| v.len() == 43)
        .ok_or_else(|| reject("invalid_request"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(id)
        .map_err(|_| reject("invalid_request"))?;
    if bytes.len() != 32 || URL_SAFE_NO_PAD.encode(&bytes) != id {
        return Err(reject("invalid_request"));
    }
    Ok(Some(id))
}
pub(super) fn scope(
    config: &super::super::config::IdentityConfig,
    owner: &str,
    origin: &str,
    key: &[u8; 32],
    pin_fingerprint: &str,
) -> String {
    digest(&config.session_key, "native-managed-login-scope-v1", &json!({"namespace":config.namespace,"rp_version":config.rp.version,"identity_instance_epoch":config.rp.identity_instance_epoch,"owner":owner,"origin":origin,"public_key":hex::encode(key),"purpose":"login","owner_pin_fingerprint":pin_fingerprint}).to_string())
}
fn committed() -> Value {
    json!({"api_version":2,"ok":true,"phase":"operation_committed","next_action":"fresh_authentication"})
}
pub(super) fn complete_original(
    tx: &super::super::store::IdentityTransaction<'_>,
    config: &super::super::config::IdentityConfig,
    id: &str,
    owner: &str,
    origin: &str,
    complete_digest: &str,
) -> Result<Option<Value>, StoreError> {
    let Some(outcome) = tx.get::<Outcome>(&outcome_key(id))? else {
        return Ok(None);
    };
    let outcome = outcome.value;
    if outcome.operation_id != id
        || outcome.owner_context_digest != owner
        || outcome.origin != origin
        || outcome.namespace != config.namespace
        || outcome.purpose != "login"
        || outcome.complete_digest != complete_digest
    {
        return Err(reject("idempotency_conflict"));
    }
    // Historical non-secret fact only, even after expiry or RP epoch change. Never store or recover a SID.
    Ok(Some(committed()))
}
pub(super) fn options_original(
    tx: &super::super::store::IdentityTransaction<'_>,
    id: &str,
    scope_digest: &str,
    owner: &str,
    now: u64,
) -> Result<Option<Value>, StoreError> {
    if let Some(outcome) = tx.get::<Outcome>(&outcome_key(id))? {
        if outcome.value.operation_id != id
            || outcome.value.scope_digest != scope_digest
            || outcome.value.owner_context_digest != owner
        {
            return Err(reject("idempotency_conflict"));
        }
        return Ok(Some(committed()));
    }
    if tx
        .get::<OperationReceipt>(&format!("receipt:{id}"))?
        .is_some()
    {
        return Err(reject("idempotency_conflict"));
    }
    let Some(ceremony) = tx.get::<CeremonyRecord>(&format!("ceremony:{id}"))? else {
        return Ok(None);
    };
    let ceremony = ceremony.value;
    if ceremony.kind != "native_managed_login"
        || ceremony.target_digest.as_deref() != Some(scope_digest)
        || ceremony.owner_context_digest != owner
    {
        return Err(reject("idempotency_conflict"));
    }
    check_ceremony(&ceremony, owner, now)?;
    let options = ceremony.server_state.ok_or(StoreError::Corrupt)?;
    Ok(Some(
        json!({"api_version":2,"ok":true,"ceremony_id":id,"operation_id":id,"options":options,"expires_at":ceremony.expires_at}),
    ))
}
pub(super) fn validate_pending(
    config: &super::super::config::IdentityConfig,
    ceremony: &CeremonyRecord,
    owner: &str,
    origin: &str,
    key: &[u8; 32],
    pin_fingerprint: &str,
) -> Result<(), StoreError> {
    if ceremony.kind == "native_managed_login"
        && (ceremony.namespace != config.namespace
            || ceremony.rp_config_version != config.rp.version
            || ceremony.target_digest.as_deref()
                != Some(scope(config, owner, origin, key, pin_fingerprint).as_str()))
    {
        return Err(reject("authentication_failed"));
    }
    Ok(())
}
#[expect(
    clippy::too_many_arguments,
    reason = "Successful PoP tombstone retains exact HTTP, challenge and account provenance without a SID"
)]
pub(super) fn record(
    tx: &mut super::super::store::IdentityTransaction<'_>,
    config: &super::super::config::IdentityConfig,
    ceremony: &CeremonyRecord,
    owner: &str,
    origin: &str,
    key: &[u8; 32],
    complete_digest: &str,
    pin_fingerprint: &str,
    account: &AccountRecord,
    credential: &CredentialRecord,
    now: u64,
) -> Result<(), StoreError> {
    if ceremony.kind != "native_managed_login" {
        return Ok(());
    }
    validate_pending(config, ceremony, owner, origin, key, pin_fingerprint)?;
    let outcome = Outcome {
        operation_id: ceremony.ceremony_id.clone(),
        owner_context_digest: owner.into(),
        origin: origin.into(),
        namespace: config.namespace.clone(),
        identity_instance_epoch: config.rp.identity_instance_epoch,
        public_key: *key,
        purpose: "login".into(),
        scope_digest: scope(config, owner, origin, key, pin_fingerprint),
        complete_digest: complete_digest.into(),
        account_id: account.account_id.clone(),
        credential_ref: credential.credential_record_id.clone(),
        committed_at: now,
    };
    tx.create(
        &outcome_key(&ceremony.ceremony_id),
        "native_managed_login_outcome",
        Some(&account.account_id),
        None,
        &outcome,
    )
}
