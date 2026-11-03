//! Strict finite-lifecycle Directory admission. Public routing reference only;
//! no seed, Native observation or FullCurrent is produced here.
use crate::directory_config::{DirectoryConfig, digest};
use crate::directory_rpc::NativeInspectReference;
use oasis7_custody_core::{
    DecimalU64, execution_pending_current_v2::*, native_cleanup_v2::*,
    provisioning::ManagerPrincipal,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use worker::Env;
fn denied() -> JsValue {
    JsValue::from_str("pending Directory unavailable")
}
fn clock() -> Result<u64, JsValue> {
    let n = js_sys::Date::now();
    if !n.is_finite() || n <= 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
        return Err(denied());
    }
    Ok(n as u64)
}
fn capacity(env: &Env) -> Result<u32, JsValue> {
    let raw = env
        .var("CUSTODY_EXECUTION_DIRECTORY_ADMISSION_CAPACITY")
        .map_err(|_| denied())?
        .to_string();
    if raw.is_empty()
        || raw.len() > 10
        || raw.starts_with('0')
        || !raw.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(denied());
    }
    let n = raw.parse::<u32>().map_err(|_| denied())?;
    if n == 0 || n.to_string() != raw {
        return Err(denied());
    }
    Ok(n)
}
fn reference(proof: &SignedExecutionPendingCurrentLeafV2) -> NativeInspectReference {
    let m = &proof.claim.manager;
    NativeInspectReference {
        identity_namespace: m.identity_namespace.clone(),
        identity_instance_epoch: m.identity_instance_epoch,
        account_id: m.account_id.clone(),
        session_ref: m.session_ref.clone(),
        credential_ref: m.credential_ref.clone(),
        credential_version: m.credential_version,
        security_version: m.security_version,
        policy_version: m.policy_version,
        expires_at: m.expires_at,
        idle_expires_at: m.idle_expires_at,
    }
}
fn manager_name(
    config: &DirectoryConfig,
    proof: &SignedExecutionPendingCurrentLeafV2,
) -> Result<String, JsValue> {
    let m = &proof.claim.manager;
    if m.identity_namespace != config.namespace
        || m.identity_instance_epoch != config.epoch
        || proof.claim.scope.context.identity.environment != config.environment
        || proof.claim.scope.context.identity.role != oasis7_custody_core::http::Role::Identity
    {
        return Err(denied());
    }
    let principal = ManagerPrincipal {
        identity_namespace: m.identity_namespace.clone(),
        native_account_id: m.account_id.clone(),
    };
    digest(
        "oasis7.custody.manager-directory.v1",
        &(&config.environment, &principal),
    )
    .map_err(|_| denied())
}
fn config_digest(config: &DirectoryConfig) -> Result<String, JsValue> {
    digest(
        "oasis7.custody.pending-directory-config.v2",
        &(
            &config.namespace,
            config.epoch,
            &config.environment,
            &config.routes,
            &config.world,
            config.version,
        ),
    )
    .map_err(|_| denied())
}
fn historical_reference(
    env: &Env,
    proof: &SignedExecutionPendingCurrentLeafV2,
    now: u64,
) -> Result<
    (
        DirectoryConfig,
        ManagerPrincipal,
        String,
        AccountMachineIssuerPinV2,
        AccountMachineIssuerFloorV2,
    ),
    JsValue,
> {
    let _capacity = capacity(env)?;
    let (pin, floor) = crate::execution_context_issuer_pin_v2::historical_pin(
        env,
        AccountMachinePurposeV2::ExecutionPendingCurrentLeafV2,
    )
    .map_err(|_| denied())?;
    proof
        .verify_issuer(
            &pin,
            &floor,
            PendingCurrentReceiverV2::ManagerAssociationsActor,
            now,
        )
        .map_err(|_| denied())?;
    let config = DirectoryConfig::from_env(env)
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    if pin.environment != config.environment
        || env
            .var("CUSTODY_SERVICE_EPOCH")
            .map_err(|_| denied())?
            .to_string()
            != proof.claim.expected_service_epoch.0.to_string()
    {
        return Err(denied());
    }
    let principal = config
        .inspection_reference_principal(&reference(proof))
        .map_err(|_| denied())?;
    let name = manager_name(&config, proof)?;
    Ok((config, principal, name, pin, floor))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    api_version: u8,
    object_id: String,
    directory_name: String,
    environment: String,
    config_digest: String,
    capacity: u32,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    installed_at: DecimalU64,
    birth_digest: String,
    count: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Birth {
    api_version: u8,
    object_id: String,
    directory_name: String,
    environment: String,
    config_digest: String,
    capacity: u32,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    proof: SignedExecutionPendingCurrentLeafV2,
    claimed_at: DecimalU64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    api_version: u8,
    proof: SignedExecutionPendingCurrentLeafV2,
    claimed_at: DecimalU64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    payload: String,
}
fn key(proof: &SignedExecutionPendingCurrentLeafV2, object: &str) -> Result<String, JsValue> {
    let c = &proof.claim;
    digest(
        "oasis7.custody.pending-directory-once.v2",
        &(
            &c.issuer.namespace,
            &c.issuer.service_id,
            &c.issuer.environment,
            object,
            &c.physical_attempt_id,
        ),
    )
    .map_err(|_| denied())
}
fn verify_saved_inventory(
    env: &Env,
    actual_object: &str,
    saved: (Vec<Row>, Vec<Row>, String, &Row),
    at: u64,
    rejected_key: Option<&str>,
) -> Result<(Metadata, String), JsValue> {
    let (metadata, rows, count, birth_row) = saved;
    if birth_row.payload.len() > 32768 {
        return Err(denied());
    }
    let first: Birth = serde_json::from_str(&birth_row.payload).map_err(|_| denied())?;
    let (config, _, name, pin, floor) =
        historical_reference(env, &first.proof, first.claimed_at.0)?;
    let expected = env
        .durable_object("MANAGER_ASSOCIATIONS")
        .map_err(|_| denied())?
        .id_from_name(&name)
        .map_err(|_| denied())?
        .to_string();
    if expected != actual_object {
        return Err(denied());
    }
    let capacity = capacity(env)?;
    let config_hash = config_digest(&config)?;
    if metadata.len() != 1
        || metadata[0].id != "singleton"
        || metadata[0].payload.len() > 32768
        || birth_row.id != "manager-directory-pending-birth-v2"
        || birth_row.payload.len() > 32768
        || rows.len() > usize::try_from(capacity).map_err(|_| denied())?
        || count != rows.len().to_string()
    {
        return Err(denied());
    }
    let meta: Metadata = serde_json::from_str(&metadata[0].payload).map_err(|_| denied())?;
    let birth: Birth = serde_json::from_str(&birth_row.payload).map_err(|_| denied())?;
    let birth_hash =
        digest("oasis7.custody.pending-directory-birth.v2", &birth).map_err(|_| denied())?;
    if meta.api_version != 2
        || meta.object_id != actual_object
        || meta.directory_name != name
        || meta.environment != config.environment
        || meta.config_digest != config_hash
        || meta.capacity != capacity
        || meta.pin != pin
        || meta.floor != floor
        || meta.installed_at.0 == 0
        || meta.installed_at.0 > at
        || meta.count != u32::try_from(rows.len()).map_err(|_| denied())?
        || meta.birth_digest != birth_hash
        || birth.api_version != 2
        || birth.object_id != actual_object
        || birth.directory_name != name
        || birth.environment != config.environment
        || birth.config_digest != config_hash
        || birth.capacity != capacity
        || birth.pin != pin
        || birth.floor != floor
        || birth.claimed_at != meta.installed_at
        || manager_name(&config, &birth.proof)? != name
    {
        return Err(denied());
    }
    birth
        .proof
        .verify_issuer(
            &pin,
            &floor,
            PendingCurrentReceiverV2::ManagerAssociationsActor,
            birth.claimed_at.0,
        )
        .map_err(|_| denied())?;
    let birth_key = key(&birth.proof, actual_object)?;
    let mut found_birth = false;
    let mut keys = std::collections::BTreeSet::new();
    for row in rows {
        if row.payload.len() > 32768 {
            return Err(denied());
        }
        let c: Claim = serde_json::from_str(&row.payload).map_err(|_| denied())?;
        if c.api_version != 2
            || c.claimed_at.0 < meta.installed_at.0
            || c.claimed_at.0 > at
            || row.id != key(&c.proof, actual_object)?
            || rejected_key == Some(row.id.as_str())
            || !keys.insert(row.id.clone())
            || manager_name(&config, &c.proof)? != name
        {
            return Err(denied());
        }
        c.proof
            .verify_issuer(
                &pin,
                &floor,
                PendingCurrentReceiverV2::ManagerAssociationsActor,
                c.claimed_at.0,
            )
            .map_err(|_| denied())?;
        if row.id == birth_key {
            if c.proof != birth.proof || c.claimed_at != birth.claimed_at || found_birth {
                return Err(denied());
            }
            found_birth = true;
        }
    }
    if !found_birth {
        return Err(denied());
    }
    let canonical = serde_json::to_string(&birth).map_err(|_| denied())?;
    if canonical != birth_row.payload {
        return Err(denied());
    }
    Ok((meta, canonical))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalInventory {
    birth: Vec<Row>,
    header: Vec<Row>,
    once: Vec<Row>,
}
/// Historical storage validation only. Absence is not evidence of a fresh
/// installation or permission to reissue a first handle. No Current is returned.
pub(crate) fn historical_inventory(
    env: &Env,
    actual_object: &str,
    raw: &str,
) -> Result<(), JsValue> {
    let cap = capacity(env)?;
    let bound = usize::try_from(cap)
        .map_err(|_| denied())?
        .checked_add(3)
        .and_then(|n| n.checked_mul(32768 * 2 + 512))
        .ok_or_else(denied)?;
    if raw.len() > bound {
        return Err(denied());
    }
    let saved: HistoricalInventory = serde_json::from_str(raw).map_err(|_| denied())?;
    if saved.birth.len() != 1 {
        return Err(denied());
    }
    let count = saved.once.len().to_string();
    verify_saved_inventory(
        env,
        actual_object,
        (saved.header, saved.once, count, &saved.birth[0]),
        clock()?,
        None,
    )?;
    Ok(())
}
