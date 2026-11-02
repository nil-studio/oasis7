//! Complete FENCES history adapter before every ordinary initializer/reader.
//! Profile records are never accepted using their own public key or capacity.
use oasis7_custody_core::{DecimalU64, read_fence_admission_v3::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};
use worker::Env;
const CONFIG_BYTES: usize = 65_536;
#[wasm_bindgen(module = "/platform/read_fence_admission_sql_v3.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readFenceOrdinaryInventoryV3)]
    fn ordinary(
        state: &JsValue,
        capacities: &js_sys::Array,
        row_limit: u32,
        initial: &str,
        project: &js_sys::Function,
    ) -> Result<String, JsValue>;
    #[wasm_bindgen(catch,js_name=transitionFenceOrdinaryV3)]
    fn transition(
        state: &JsValue,
        capacities: &js_sys::Array,
        row_limit: u32,
        project: &js_sys::Function,
        step: &js_sys::Function,
    ) -> Result<String, JsValue>;
    #[wasm_bindgen(catch,js_name=consumeReadFenceAdmissionV3)]
    fn consume(
        state: &JsValue,
        capacities: &js_sys::Array,
        row_limit: u32,
        step: &js_sys::Function,
    ) -> Result<String, JsValue>;
    #[wasm_bindgen(catch,js_name=consumeExecutionFenceAdmissionV3)]
    fn consume_execution(
        state: &JsValue,
        capacities: &js_sys::Array,
        row_limit: u32,
        step: &js_sys::Function,
    ) -> Result<String, JsValue>;
}
fn denied() -> JsValue {
    JsValue::from_str("fence admission unavailable")
}
fn profile_index(p: FenceAdmissionProfileV3) -> usize {
    match p {
        FenceAdmissionProfileV3::ExecutionV2 => 0,
        FenceAdmissionProfileV3::ReconcileReadV3 => 1,
    }
}
fn config(env: &Env, p: FenceAdmissionProfileV3) -> Result<FenceAdmissionConfigV3, JsValue> {
    let prefix = match p {
        FenceAdmissionProfileV3::ExecutionV2 => "CUSTODY_EXECUTION_FENCE_ADMISSION",
        FenceAdmissionProfileV3::ReconcileReadV3 => "CUSTODY_READ_FENCE_ADMISSION",
    };
    let text = env
        .var(&format!("{prefix}_CAPACITY"))
        .map_err(|_| denied())?
        .to_string();
    let capacity: u32 = text.parse().map_err(|_| denied())?;
    if capacity == 0 || capacity.to_string() != text {
        return Err(denied());
    }
    let raw = env
        .var(&format!("{prefix}_VERIFIER_HISTORY"))
        .map_err(|_| denied())?
        .to_string();
    if raw.len() > CONFIG_BYTES {
        return Err(denied());
    }
    let history: Vec<FenceAdmissionVerifierV3> =
        serde_json::from_str(&raw).map_err(|_| denied())?;
    let active = env
        .var(&format!("{prefix}_ACTIVE_VERIFIER"))
        .ok()
        .map(|v| v.to_string())
        .map(|raw| {
            if raw.len() > CONFIG_BYTES {
                return Err(denied());
            }
            serde_json::from_str(&raw).map_err(|_| denied())
        })
        .transpose()?;
    let recovery = crate::recovery_fence_store::policy(env)?;
    if history
        .iter()
        .any(|v: &FenceAdmissionVerifierV3| v.pin.environment != recovery.environment)
    {
        return Err(denied());
    }
    let c = FenceAdmissionConfigV3 {
        profile: p,
        capacity: DecimalU64(u64::from(capacity)),
        active,
        history,
    };
    c.validate().map_err(|_| denied())?;
    Ok(c)
}
fn enabled(env: &Env, profile: FenceAdmissionProfileV3) -> bool {
    let key = match profile {
        FenceAdmissionProfileV3::ExecutionV2 => "CUSTODY_EXECUTION_FENCE_ADMISSION_ENABLED",
        FenceAdmissionProfileV3::ReconcileReadV3 => "CUSTODY_READ_FENCE_ADMISSION_ENABLED",
    };
    env.var(key).ok().map(|v| v.to_string()).as_deref() == Some("true")
}

fn configurations(env: &Env) -> [Option<FenceAdmissionConfigV3>; 2] {
    [
        config(env, FenceAdmissionProfileV3::ExecutionV2).ok(),
        config(env, FenceAdmissionProfileV3::ReconcileReadV3).ok(),
    ]
}
fn capacity_args(config: &[Option<FenceAdmissionConfigV3>; 2]) -> js_sys::Array {
    let out = js_sys::Array::new();
    for c in config {
        out.push(
            &c.as_ref()
                .map(|c| c.capacity.0.to_string())
                .unwrap_or_else(|| "0".into())
                .into(),
        );
    }
    out
}
fn row_limit() -> Result<u32, JsValue> {
    u32::try_from(fence_admission_row_limit_v3().map_err(|_| denied())?).map_err(|_| denied())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    payload: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalogue {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    sql: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileRows {
    profile: FenceAdmissionProfileV3,
    header: Option<Vec<Record>>,
    floor: Option<Vec<Record>>,
    count: Option<String>,
    once: Option<Vec<Record>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRecord {
    id: String,
    payload: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<StoredRecord>,
    catalogue: Vec<Catalogue>,
    audit_count: Option<DecimalU64>,
    audit_rows: Option<Vec<serde_json::Value>>,
    admissions: Vec<ProfileRows>,
}
fn exact<T: serde::de::DeserializeOwned + Serialize>(row: &Record) -> Result<T, JsValue> {
    let v: T = serde_json::from_str(&row.payload).map_err(|_| denied())?;
    if row.payload.len() > usize::try_from(row_limit()?).map_err(|_| denied())?
        || serde_json::to_string(&v).map_err(|_| denied())? != row.payload
    {
        return Err(denied());
    }
    Ok(v)
}
struct Checked {
    business: Option<String>,
    record_ids: Vec<String>,
    rows: [Option<FenceAdmissionRowsV3>; 2],
}
fn checked(
    raw: &str,
    config: &[Option<FenceAdmissionConfigV3>; 2],
    object: &str,
    business_payloads: bool,
) -> Result<Checked, JsValue> {
    let v: Inventory = serde_json::from_str(raw).map_err(|_| denied())?;
    if v.admissions.len() != 2
        || v.catalogue.len() >= fence_catalogue_probe_limit_v3().map_err(|_| denied())?
    {
        return Err(denied());
    }
    let mut rows: [Option<FenceAdmissionRowsV3>; 2] = [None, None];
    let mut seen = std::collections::BTreeSet::new();
    let mut profile_tables = std::collections::BTreeSet::new();
    for saved in &v.admissions {
        let i = profile_index(saved.profile);
        if !seen.insert(i) {
            return Err(denied());
        }
        match (&saved.header, &saved.floor, &saved.count, &saved.once) {
            (None, None, None, None) => (),
            (Some(header), Some(floor), Some(count), Some(claims)) => {
                let c = config[i].as_ref().ok_or_else(denied)?;
                if header.len() != 1
                    || floor.len() != 1
                    || header[0].id != "singleton"
                    || floor[0].id != "singleton"
                {
                    return Err(denied());
                }
                let count: DecimalU64 =
                    serde_json::from_value(count.clone().into()).map_err(|_| denied())?;
                if count.0 > c.capacity.0
                    || u64::try_from(claims.len()).map_err(|_| denied())? != count.0
                {
                    return Err(denied());
                }
                let birth = v
                    .records
                    .iter()
                    .find(|r| r.id == saved.profile.birth_key())
                    .ok_or_else(denied)?;
                let birth: FenceAdmissionBirthV3 = exact(&Record {
                    id: birth.id.clone(),
                    payload: birth.payload.clone().ok_or_else(denied)?,
                })?;
                let mut once = Vec::new();
                for claim in claims {
                    let row: FenceAdmissionOnceV3 = exact(claim)?;
                    if row.id != claim.id {
                        return Err(denied());
                    }
                    once.push(row);
                }
                let r = FenceAdmissionRowsV3 {
                    birth,
                    header: exact(&header[0])?,
                    floor: exact(&floor[0])?,
                    once,
                };
                r.validate(c, object).map_err(|_| denied())?;
                rows[i] = Some(r);
                for kind in ["header", "floor", "once"] {
                    profile_tables.insert(format!("{}_{}", saved.profile.table_prefix(), kind));
                }
            }
            _ => return Err(denied()),
        }
    }
    let mut names = std::collections::BTreeSet::new();
    let mut business_tables = Vec::new();
    let ordinary = [
        "custody_records",
        "custody_audit",
        "sqlite_sequence",
        "_cf_METADATA",
    ];
    for item in &v.catalogue {
        if !names.insert((item.kind.clone(), item.name.clone())) {
            return Err(denied());
        }
        if item.kind == "table" && profile_tables.contains(&item.name) {
            continue;
        }
        if item.kind == "index"
            && item.sql.is_none()
            && ordinary
                .iter()
                .copied()
                .chain(profile_tables.iter().map(String::as_str))
                .any(|name| item.name == format!("sqlite_autoindex_{name}_1"))
        {
            continue;
        }
        if item.kind != "table" || !ordinary.contains(&item.name.as_str()) {
            return Err(denied());
        }
        business_tables.push(format!("table:{}", item.name));
    }
    for name in &profile_tables {
        if !names.contains(&("table".into(), name.clone())) {
            return Err(denied());
        }
    }
    let selected = |i: usize| rows[i].as_ref().zip(config[i].as_ref());
    let keys: Vec<_> = v
        .records
        .iter()
        .map(|r| FenceAdmissionRecordKeyV3 {
            id: r.id.clone(),
            birth_payload: if r.id == FenceAdmissionProfileV3::ExecutionV2.birth_key()
                || r.id == FenceAdmissionProfileV3::ReconcileReadV3.birth_key()
            {
                r.payload.clone()
            } else {
                None
            },
        })
        .collect();
    verify_fence_birth_inventory_v3(&keys, selected(0), selected(1), object)
        .map_err(|_| denied())?;
    let record_ids = v.records.iter().map(|r| r.id.clone()).collect();
    let business = if business_payloads {
        let full = v
            .records
            .into_iter()
            .map(|r| {
                Ok(FenceBusinessRecordV3 {
                    id: r.id,
                    payload: r.payload.ok_or_else(denied)?,
                })
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        let records = project_fence_business_records_v3(&full, selected(0), selected(1), object)
            .map_err(|_| denied())?;
        Some(serde_json::to_string(&serde_json::json!({"records":records,"table_names":business_tables,"audit_count":v.audit_count.ok_or_else(denied)?,"audit_rows":v.audit_rows.ok_or_else(denied)?})).map_err(|_|denied())?)
    } else {
        if v.audit_count.is_some() || v.audit_rows.is_some() {
            return Err(denied());
        }
        if v.records.iter().any(|r| {
            (r.id == "recovery-fence-v1" || r.id == "recovery-fence-ledger-v1")
                && r.payload.is_some()
        }) {
            return Err(denied());
        }
        None
    };
    Ok(Checked {
        business,
        record_ids,
        rows,
    })
}
/// The callback runs before a missing legacy FENCE could be initialized.
pub(crate) fn ordinary_inventory(
    state: &JsValue,
    env: &Env,
    object: &str,
    initial_epoch: &str,
) -> Result<String, JsValue> {
    let epoch: DecimalU64 = serde_json::from_value(initial_epoch.into()).map_err(|_| denied())?;
    let initial = serde_json::to_string(&oasis7_custody_core::recovery::RecoveryFence::new(
        epoch.0, true,
    ))
    .map_err(|_| denied())?;
    let configured = configurations(env);
    let mut project = |raw: String| {
        checked(&raw, &configured, object, true).and_then(|v| v.business.ok_or_else(denied))
    };
    let callback = Closure::borrow_mut(&mut project);
    let result = ordinary(
        state,
        &capacity_args(&configured),
        row_limit()?,
        &initial,
        callback.as_ref().unchecked_ref(),
    );
    drop(callback);
    result
}
pub(crate) fn ordinary_transition(
    state: &JsValue,
    env: &Env,
    object: &str,
    step: &js_sys::Function,
) -> Result<String, JsValue> {
    let configured = configurations(env);
    let mut project = |raw: String| {
        checked(&raw, &configured, object, true).and_then(|v| v.business.ok_or_else(denied))
    };
    let callback = Closure::borrow_mut(&mut project);
    let result = transition(
        state,
        &capacity_args(&configured),
        row_limit()?,
        callback.as_ref().unchecked_ref(),
        step,
    );
    drop(callback);
    result
}
/// SQL commit is completed before the caller performs the authoritative business
/// read. Failure/expiry afterwards retains the paid once and never returns a retry.
pub(crate) fn consume_profile(
    state: &JsValue,
    env: &Env,
    object: &str,
    proof: FenceAdmissionProofV3,
    now: u64,
) -> Result<(), JsValue> {
    // Both paid profiles read the one deployment-owned service fence. A valid
    // signature bound to a different object of this class is not permission
    // to create admission state there. Namespace ID derivation performs no RPC.
    let canonical = env
        .durable_object("FENCES")
        .map_err(|_| denied())?
        .id_from_name("service-fence-v1")
        .map_err(|_| denied())?
        .to_string();
    if object != canonical {
        return Err(denied());
    }
    let profile = proof.profile();
    let index = profile_index(profile);
    if !enabled(env, profile) {
        return Err(denied());
    }
    let configured = configurations(env);
    let read = configured[index].as_ref().ok_or_else(denied)?;
    // Exact actual Actor/purpose/key/floor are checked before the first SQL.
    read.verify_current(&proof, object, now)
        .map_err(|_| denied())?;
    let mut step = |raw: String| -> Result<js_sys::Array, JsValue> {
        let old = checked(&raw, &configured, object, false)?;
        // Only key presence is read before once. Actual FENCE/LEDGER payloads
        // are fetched by the caller only after this SQL transaction commits.
        if !old.record_ids.iter().any(|id| id == "recovery-fence-v1") {
            return Err(denied());
        }
        let next = FenceAdmissionRowsV3::consume(
            old.rows[index].as_ref(),
            read,
            object,
            proof.clone(),
            now,
        )
        .map_err(|_| denied())?;
        let row = next
            .once
            .iter()
            .find(|v| v.ordinal == next.header.count)
            .ok_or_else(denied)?;
        let out = js_sys::Array::new();
        for value in [
            serde_json::to_string(&next.birth),
            serde_json::to_string(&next.header),
            serde_json::to_string(&next.floor),
            Ok(row.id.clone()),
            serde_json::to_string(row),
            Ok(row.claimed_at.0.to_string()),
            Ok(row.proof.expires_at().to_string()),
        ] {
            out.push(&value.map_err(|_| denied())?.into());
        }
        Ok(out)
    };
    let callback = Closure::borrow_mut(&mut step);
    let result = match profile {
        FenceAdmissionProfileV3::ReconcileReadV3 => consume(
            state,
            &capacity_args(&configured),
            row_limit()?,
            callback.as_ref().unchecked_ref(),
        ),
        FenceAdmissionProfileV3::ExecutionV2 => consume_execution(
            state,
            &capacity_args(&configured),
            row_limit()?,
            callback.as_ref().unchecked_ref(),
        ),
    };
    drop(callback);
    checked(&result?, &configured, object, false)?;
    Ok(())
}

pub(crate) fn verify_profile_current(
    env: &Env,
    object: &str,
    proof: &FenceAdmissionProofV3,
    now: u64,
) -> Result<(), JsValue> {
    if !enabled(env, proof.profile()) {
        return Err(denied());
    }
    config(env, proof.profile())?
        .verify_current(proof, object, now)
        .map_err(|_| denied())
}
