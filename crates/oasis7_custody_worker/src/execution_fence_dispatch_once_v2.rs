//! One dedicated object/one permanent claim. No network, capability, key loan
//! or repair path exists in this module. The first token exists only after SQL.
use crate::execution_fence_config_v2::{clock, pin, rejected};
use oasis7_custody_core::{DecimalU64, execution_fence_dispatch_v2::*, native_cleanup_v2::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};
use worker::Env;
#[wasm_bindgen(module = "/platform/execution_fence_dispatch_sql_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=claimExecutionFenceDispatch)]
    fn claim_sql(state: &JsValue, transition: &js_sys::Function) -> Result<String, JsValue>;
}
fn required_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
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
    #[serde(deserialize_with = "required_nullable")]
    rows: Option<Vec<Row>>,
    #[serde(deserialize_with = "required_nullable")]
    header: Option<Vec<Row>>,
}
#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claim {
    api_version: u8,
    proof: SignedExecutionFenceDispatchV2,
    claimed_at: DecimalU64,
}
/// Private first-return handle. Neither stored JSON nor a second request creates it.
pub(crate) struct ClaimedExecutionFenceDispatchV2 {
    proof: SignedExecutionFenceDispatchV2,
}
impl ClaimedExecutionFenceDispatchV2 {
    pub(crate) fn proof(&self) -> &SignedExecutionFenceDispatchV2 {
        &self.proof
    }
}
pub(crate) fn claim(
    state: &JsValue,
    env: &Env,
    actual_id: &str,
    raw: &str,
) -> Result<ClaimedExecutionFenceDispatchV2, JsValue> {
    if raw.is_empty() || raw.len() > 32768 {
        return Err(rejected());
    }
    let proof: SignedExecutionFenceDispatchV2 =
        serde_json::from_str(raw).map_err(|_| rejected())?;
    let (pin, floor) = pin(env, AccountMachinePurposeV2::ExecutionFenceDispatchV2)?;
    let expected = env
        .durable_object("EXECUTION_FENCE_DISPATCHERS")
        .map_err(|_| rejected())?
        .id_from_name(&proof.binding.receiver_name().map_err(|_| rejected())?)
        .map_err(|_| rejected())?
        .to_string();
    if actual_id != expected {
        return Err(rejected());
    }
    let actual_fence = env
        .durable_object("FENCES")
        .map_err(|_| rejected())?
        .id_from_name("service-fence-v1")
        .map_err(|_| rejected())?
        .to_string();
    if actual_fence != proof.binding.fence_object_id {
        return Err(rejected());
    }
    let (read_pin, read_floor) =
        crate::execution_fence_config_v2::pin(env, AccountMachinePurposeV2::ExecutionFenceReadV2)?;
    if read_pin.public_key != pin.public_key {
        return Err(rejected());
    }
    proof
        .binding
        .issuer
        .verify_pin(
            &read_pin,
            &read_floor,
            AccountMachinePurposeV2::ExecutionFenceReadV2,
            proof.binding.issued_at.0,
            proof.binding.expires_at.0,
            clock()?,
        )
        .map_err(|_| rejected())?;
    proof
        .verify(&pin, &floor, actual_id, clock()?)
        .map_err(|_| rejected())?;
    let mut transition = |raw: String| -> Result<String, JsValue> {
        if raw.len() > 4 * 65536 {
            return Err(rejected());
        }
        let inventory: Inventory = serde_json::from_str(&raw).map_err(|_| rejected())?;
        // Existing complete history is strongly rechecked but never returns a
        // dispatch token. Partial/foreign state is rejected without repair.
        if inventory.rows.is_some() || inventory.header.is_some() {
            let rows = inventory.rows.ok_or_else(rejected)?;
            let header = inventory.header.ok_or_else(rejected)?;
            if inventory.tables.iter().any(|t| {
                !matches!(
                    t.as_str(),
                    "table:_cf_METADATA"
                        | "table:custody_execution_fence_dispatch_once_v2"
                        | "table:custody_execution_fence_dispatch_header_v2"
                )
            }) || rows.len() != 1
                || rows[0].id != "singleton"
                || rows[0].payload.len() > 65536
                || header.len() != 1
                || header[0].id != "singleton"
                || header[0].payload != rows[0].payload
            {
                return Err(rejected());
            }
            let stored: Claim = serde_json::from_str(&rows[0].payload).map_err(|_| rejected())?;
            if stored.api_version != 2
                || serde_json::to_string(&stored).map_err(|_| rejected())? != rows[0].payload
                || stored.proof.binding != proof.binding
            {
                return Err(rejected());
            }
            stored
                .proof
                .verify(&pin, &floor, actual_id, stored.claimed_at.0)
                .map_err(|_| rejected())?;
            return Err(rejected());
        }
        if inventory.tables.iter().any(|t| t != "table:_cf_METADATA") {
            return Err(rejected());
        }
        let at = clock()?;
        proof
            .verify(&pin, &floor, actual_id, at)
            .map_err(|_| rejected())?;
        serde_json::to_string(&Claim {
            api_version: 2,
            proof: proof.clone(),
            claimed_at: DecimalU64(at),
        })
        .map_err(|_| rejected())
    };
    let function = Closure::borrow_mut(&mut transition);
    let saved = claim_sql(state, function.as_ref().unchecked_ref())?;
    drop(function);
    let claim: Claim = serde_json::from_str(&saved).map_err(|_| rejected())?;
    if claim.proof != proof {
        return Err(rejected());
    }
    // A failed SQL transaction returns before this sole constructor.
    Ok(ClaimedExecutionFenceDispatchV2 { proof })
}
