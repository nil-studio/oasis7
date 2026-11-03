//! Separate metadata actor/state; enrollment and session state never share this row.
use crate::{
    management_common::{clock, denied},
    management_metadata::AbuseRequest,
    management_metadata_policy::{BINDING, Config},
};
use oasis7_custody_core::management_metadata::ManagementMetadataQuotaV1;
use serde::Deserialize;
use wasm_bindgen::{JsCast, prelude::*};
use worker::Env;
const KEY: &str = "custody-management-metadata-abuse-v1";
#[wasm_bindgen(module = "/platform/client_enrollment_sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=enrollmentAbuseTransition)]
    fn update(state: &JsValue, key: &str, callback: &js_sys::Function) -> Result<String, JsValue>;
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<Row>,
    table_names: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    payload: String,
}
pub(crate) fn charge(
    state: &JsValue,
    env: &Env,
    actor: &str,
    r: &AbuseRequest,
) -> worker::Result<()> {
    let c = Config::enabled(env)?;
    if actor
        != env
            .durable_object(BINDING)?
            .id_from_name(&c.actor_name(&r.bucket)?)?
            .to_string()
    {
        return Err(denied());
    }
    let mut cb = |old: JsValue, inventory: String| -> Result<String, JsValue> {
        let action = || -> worker::Result<String> {
            Config::enabled(env)?;
            let raw = if old.is_null() {
                None
            } else {
                Some(old.as_string().ok_or_else(denied)?)
            };
            if inventory.len() > 8192 || raw.as_ref().is_some_and(|r| r.len() > 1024) {
                return Err(denied());
            }
            let inv: Inventory = serde_json::from_str(&inventory).map_err(|_| denied())?;
            if inv.records.len() != usize::from(raw.is_some())
                || inv
                    .records
                    .iter()
                    .any(|row| row.id != KEY || Some(&row.payload) != raw.as_ref())
                || inv.table_names.len() > 3
                || inv.table_names.iter().any(|name| {
                    !matches!(
                        name.as_str(),
                        "table:custody_records" | "table:_cf_METADATA"
                    )
                })
            {
                return Err(denied());
            }
            let old = raw
                .map(|r| serde_json::from_str::<ManagementMetadataQuotaV1>(&r))
                .transpose()
                .map_err(|_| denied())?;
            serde_json::to_string(
                &ManagementMetadataQuotaV1::charge(
                    old,
                    &r.bucket,
                    if r.provider { 32 } else { 1024 },
                    clock()?,
                )
                .map_err(|_| denied())?,
            )
            .map_err(Into::into)
        };
        action().map_err(|_| JsValue::from_str("management metadata rejected"))
    };
    let closure = Closure::borrow_mut(&mut cb);
    update(state, KEY, closure.as_ref().unchecked_ref()).map_err(|_| denied())?;
    Ok(())
}
