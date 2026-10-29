//! Opaque immutable context envelopes. Storage facts are never context authority.
use oasis7_custody_core::context_body::*;
use serde::Deserialize;
use wasm_bindgen::{JsCast, prelude::*};

pub(super) const RECORD: &str = "custody-context-body-v1";
type Result<T> = std::result::Result<T, JsValue>;
fn denied() -> JsValue {
    JsValue::from_str("private context storage rejected")
}
#[wasm_bindgen(module = "/platform/context_body_sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readContextBodyInventory)]
    fn inventory(state: &JsValue) -> Result<String>;
    #[wasm_bindgen(catch, js_name = contextBodyTransition)]
    fn transition(
        state: &JsValue,
        key: &str,
        initial: &str,
        callback: &js_sys::Function,
    ) -> Result<String>;
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
fn parse_inventory(raw: &str) -> Result<Inventory> {
    if raw.len() > 4 * CONTEXT_BODY_MAX_WIRE_BYTES {
        return Err(denied());
    }
    let value: Inventory = serde_json::from_str(raw).map_err(|_| denied())?;
    if value.records.len() > 1
        || value.table_names.len() > 8
        || value.table_names.iter().any(|name| {
            !matches!(
                name.as_str(),
                "table:custody_records"
                    | "table:custody_audit"
                    | "table:sqlite_sequence"
                    | "table:_cf_METADATA"
            )
        })
        || value
            .records
            .iter()
            .any(|row| row.id != RECORD || row.payload.len() > CONTEXT_BODY_MAX_WIRE_BYTES)
    {
        return Err(denied());
    }
    Ok(value)
}
fn record(raw: &str, scope: &ContextBodyScopeV1) -> Result<ContextBodyRecordV1> {
    if raw.len() > CONTEXT_BODY_MAX_WIRE_BYTES {
        return Err(denied());
    }
    let saved: ContextBodyRecordV1 = serde_json::from_str(raw).map_err(|_| denied())?;
    saved.validate_for(scope).map_err(|_| denied())?;
    if saved.wire_bytes().map_err(|_| denied())? != raw.as_bytes() {
        return Err(denied());
    }
    Ok(saved)
}
pub(super) fn read(
    state: &JsValue,
    scope: &ContextBodyScopeV1,
) -> Result<Option<ContextBodyRecordV1>> {
    scope.validate().map_err(|_| denied())?;
    let saved = parse_inventory(&inventory(state)?)?;
    saved
        .records
        .first()
        .map(|row| record(&row.payload, scope))
        .transpose()
}
pub(super) fn metadata(
    state: &JsValue,
    scope: &ContextBodyScopeV1,
) -> Result<ContextBodyStorageFactV1> {
    // No KEK or plaintext is available in this component, including after expiry.
    match read(state, scope)? {
        Some(saved) => saved.storage_fact().map_err(|_| denied()),
        None => Ok(ContextBodyStorageFactV1 {
            scope: scope.clone(),
            stored: None,
        }),
    }
}
pub(super) fn put(
    state: &JsValue,
    candidate: &ContextBodyRecordV1,
) -> Result<ContextBodyStorageFactV1> {
    candidate.validate().map_err(|_| denied())?;
    let scope = &candidate.scope;
    let wire =
        String::from_utf8(candidate.wire_bytes().map_err(|_| denied())?).map_err(|_| denied())?;
    // This empty marker exists only inside the first SQL transaction. A failed
    // callback rolls it back; a successful write always replaces it with record.
    let initial =
        serde_json::to_string(&("oasis7.context-body.empty.v1", scope)).map_err(|_| denied())?;
    let mut callback = |old: String, raw_inventory: String| -> Result<js_sys::Array> {
        let saved = parse_inventory(&raw_inventory)?;
        if saved.records.len() != 1 || saved.records[0].payload != old {
            return Err(denied());
        }
        if old != initial {
            let existing = record(&old, scope)?;
            if &existing != candidate || old != wire {
                return Err(denied());
            }
        }
        let changed = old != wire;
        let result = js_sys::Array::new();
        result.push(&JsValue::from_str(&wire));
        result.push(&JsValue::from_str(if changed { "1" } else { "" }));
        result.push(&JsValue::from_str(if changed {
            "context_body_original_saved"
        } else {
            ""
        }));
        Ok(result)
    };
    let closure = Closure::borrow_mut(&mut callback);
    let returned = transition(state, RECORD, &initial, closure.as_ref().unchecked_ref())?;
    drop(closure);
    if returned != wire {
        return Err(denied());
    }
    candidate.storage_fact().map_err(|_| denied())
}
