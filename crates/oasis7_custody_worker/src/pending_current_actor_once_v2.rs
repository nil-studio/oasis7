//! Strict finite-lifecycle Directory admission. Public routing reference only;
//! no seed, Native observation or FullCurrent is produced here.
use crate::directory_config::{DirectoryConfig, digest};
use crate::directory_rpc::NativeInspectReference;
use oasis7_custody_core::{
    DecimalU64, execution_pending_current_v2::*, native_cleanup_v2::*,
    provisioning::ManagerPrincipal,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};
use worker::Env;
const META: &str = "table:custody_pending_current_floor_v2";
const ONCE: &str = "table:custody_pending_current_once_v2";
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
pub(crate) fn reference(proof: &SignedExecutionPendingCurrentLeafV2) -> NativeInspectReference {
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
pub(crate) fn verify_reference(
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
    if env
        .var("CUSTODY_EXECUTION_PENDING_START_V2_ENABLED")
        .ok()
        .is_none_or(|v| v.to_string() != "true")
    {
        return Err(denied());
    }
    let _capacity = capacity(env)?;
    let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
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
#[wasm_bindgen(module = "/platform/execution_pending_actor_sql_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=claimExecutionPendingActorV2)]
    fn claim_sql(
        state: &JsValue,
        capacity: u32,
        transition: &js_sys::Function,
        read_capacity: u32,
        read_row_bound: f64,
        validate_all: &js_sys::Function,
    ) -> Result<String, JsValue>;
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordId {
    id: String,
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    tables: Vec<String>,
    records: Option<Vec<RecordId>>,
    birth: Option<Vec<Row>>,
    metadata: Option<Vec<Row>>,
    count: Option<String>,
    claims: Option<Vec<Row>>,
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
pub(crate) struct ClaimedDirectoryObservationV2 {
    claimed_at: u64,
    until: u64,
}
impl ClaimedDirectoryObservationV2 {
    pub(crate) fn require_observed(&self, observed: u64, now: u64) -> Result<(), JsValue> {
        if observed < self.claimed_at || now < observed || now >= self.until {
            return Err(denied());
        }
        Ok(())
    }
}
/// First success commits once before the sole caller reads its actual view.
pub(crate) fn verify_and_claim(
    state: &JsValue,
    env: &Env,
    actual_object: &str,
    proof: &SignedExecutionPendingCurrentLeafV2,
) -> Result<ClaimedDirectoryObservationV2, JsValue> {
    let (config, _, name, pin, floor) = verify_reference(env, proof, clock()?)?;
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
    let policy = crate::directory_admission_policy_v3::DirectoryAdmissionPolicyV3::load(env)
        .map_err(|_| denied())?;
    if policy.execution_capacity != capacity {
        return Err(denied());
    }
    let mut validate_all = |raw: String| -> Result<(), JsValue> {
        policy
            .verify(env, actual_object, &raw)
            .map(|_| ())
            .map_err(|_| denied())
    };
    let validation = Closure::borrow_mut(&mut validate_all);
    let config_hash = config_digest(&config)?;
    let id = key(proof, actual_object)?;
    let mut transition = |raw: String| -> Result<js_sys::Array, JsValue> {
        let bound = usize::try_from(capacity)
            .map_err(|_| denied())?
            .checked_add(3)
            .and_then(|n| n.checked_mul(32768 * 2 + 512))
            .ok_or_else(denied)?;
        if raw.len() > bound {
            return Err(denied());
        }
        let inventory: Inventory = serde_json::from_str(&raw).map_err(|_| denied())?;
        if inventory.tables.iter().any(|v| {
            !matches!(
                v.as_str(),
                "table:_cf_METADATA"
                    | "table:sqlite_sequence"
                    | "table:custody_records"
                    | "table:custody_audit"
                    | "table:custody_management_metadata"
            ) && v != META
                && v != ONCE
        }) {
            return Err(denied());
        }
        let at = clock()?;
        verify_reference(env, proof, at)?;
        let records = inventory.records.ok_or_else(denied)?;
        if records.is_empty()
            || records.len() > 2
            || !records.iter().any(|r| r.id == "manager-directory-v1")
            || records.iter().any(|r| {
                !matches!(
                    r.id.as_str(),
                    "manager-directory-v1" | "manager-directory-pending-birth-v2"
                )
            })
        {
            return Err(denied());
        }
        let births = inventory.birth.ok_or_else(denied)?;
        let (mut meta, birth_wire) = match (
            inventory.metadata,
            inventory.claims,
            inventory.count,
            births.as_slice(),
        ) {
            (None, None, None, []) => {
                let birth = Birth {
                    api_version: 2,
                    object_id: actual_object.into(),
                    directory_name: name.clone(),
                    environment: config.environment.clone(),
                    config_digest: config_hash.clone(),
                    capacity,
                    pin: pin.clone(),
                    floor: floor.clone(),
                    proof: proof.clone(),
                    claimed_at: DecimalU64(at),
                };
                let hash = digest("oasis7.custody.pending-directory-birth.v2", &birth)
                    .map_err(|_| denied())?;
                let wire = serde_json::to_string(&birth).map_err(|_| denied())?;
                (
                    Metadata {
                        api_version: 2,
                        object_id: actual_object.into(),
                        directory_name: name.clone(),
                        environment: config.environment.clone(),
                        config_digest: config_hash.clone(),
                        capacity,
                        pin: pin.clone(),
                        floor: floor.clone(),
                        installed_at: DecimalU64(at),
                        birth_digest: hash,
                        count: 0,
                    },
                    wire,
                )
            }
            (Some(metadata), Some(rows), Some(count), [birth_row]) => {
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
                let meta: Metadata =
                    serde_json::from_str(&metadata[0].payload).map_err(|_| denied())?;
                let birth: Birth =
                    serde_json::from_str(&birth_row.payload).map_err(|_| denied())?;
                let birth_hash = digest("oasis7.custody.pending-directory-birth.v2", &birth)
                    .map_err(|_| denied())?;
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
                        || row.id == id
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
                        if c.proof != birth.proof || c.claimed_at != birth.claimed_at || found_birth
                        {
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
                (meta, canonical)
            }
            _ => return Err(denied()),
        };
        if meta.count >= capacity {
            return Err(denied());
        }
        meta.count = meta.count.checked_add(1).ok_or_else(denied)?;
        let result = js_sys::Array::new();
        result.push(&serde_json::to_string(&meta).map_err(|_| denied())?.into());
        result.push(&id.clone().into());
        result.push(
            &serde_json::to_string(&Claim {
                api_version: 2,
                proof: proof.clone(),
                claimed_at: DecimalU64(at),
            })
            .map_err(|_| denied())?
            .into(),
        );
        result.push(&birth_wire.into());
        Ok(result)
    };
    let callback = Closure::borrow_mut(&mut transition);
    let saved = claim_sql(
        state,
        capacity,
        callback.as_ref().unchecked_ref(),
        policy.read_capacity(),
        policy.row_bound as f64,
        validation.as_ref().unchecked_ref(),
    )?;
    drop(callback);
    let c: Claim = serde_json::from_str(&saved).map_err(|_| denied())?;
    if c.proof != *proof {
        return Err(denied());
    }
    Ok(ClaimedDirectoryObservationV2 {
        claimed_at: c.claimed_at.0,
        until: proof.claim.expires_at.0,
    })
}
