//! Private Owner draft/tombstone component. There are no key, grant or dispatch methods.
use js_sys::Array;
use oasis7_custody_core::{DecimalU64, approval::bytes_digest, provisioning::*};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen::{JsCast, prelude::*};
use worker::*;

const DRAFT: &str = "custody-account-draft-v1";
const FREEZE: &str = "custody-account-freeze-v1";
const MAX_BYTES: usize = 32 * 1024;
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = accountTransition)]
    fn transition(
        state: &JsValue,
        key: &str,
        initial: &str,
        callback: &js_sys::Function,
    ) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = readTextRecord)]
    fn read_record(state: &JsValue, key: &str) -> std::result::Result<String, JsValue>;
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded_body(request: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<OpaqueRow>,
    table_names: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpaqueRow {
    id: String,
    payload: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftRecord {
    schema_version: u32,
    draft: Option<AccountDraftV1>,
    security_epoch: DecimalU64,
    authorizations: BTreeMap<String, serde_json::Value>,
    outbox: BTreeMap<String, serde_json::Value>,
    audit_sequence: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FreezeRecord {
    schema_version: u32,
    target: AccountFreezeTarget,
    frozen: bool,
    security_epoch: DecimalU64,
    receipt: Option<AccountFreezeReceipt>,
    audit_sequence: DecimalU64,
}
fn js_error() -> JsValue {
    JsValue::from_str("account transition rejected")
}
fn encode<T: Serialize>(value: &T) -> std::result::Result<String, JsValue> {
    let value = serde_json::to_string(value).map_err(|_| js_error())?;
    if value.len() > MAX_BYTES {
        return Err(js_error());
    }
    Ok(value)
}
fn digest<T: Serialize>(domain: &str, value: &T) -> std::result::Result<String, JsValue> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, value))
        .map(|bytes| bytes_digest(&bytes))
        .map_err(|_| js_error())
}
fn result_array(
    next: String,
    sequence: u64,
    operation: &str,
    account: &str,
) -> std::result::Result<Array, JsValue> {
    let result = Array::new();
    result.push(&next.into());
    result.push(&sequence.to_string().into());
    result.push(&encode(&(DecimalU64(sequence), operation, account))?.into());
    Ok(result)
}
fn valid_inventory(inventory: &Inventory) -> bool {
    inventory.records.len() <= 2
        && inventory
            .records
            .iter()
            .all(|row| row.id == DRAFT || row.id == FREEZE)
        && inventory.table_names.iter().all(|name| {
            matches!(
                name.as_str(),
                "table:custody_records"
                    | "table:custody_audit"
                    | "table:sqlite_sequence"
                    | "table:_cf_METADATA"
            )
        })
}
fn known_provision_draft_inventory(
    state: &JsValue,
    inventory: &Inventory,
    draft: &AccountDraftV1,
) -> bool {
    inventory.records.len() == 2
        && inventory
            .records
            .iter()
            .all(|row| row.id == DRAFT || row.id == "custody-provision-approval-v1")
        && crate::provision_approval_store::matches_persisted_draft(state, draft)
        && inventory.table_names.iter().all(|name| {
            matches!(
                name.as_str(),
                "table:custody_records"
                    | "table:custody_audit"
                    | "table:sqlite_sequence"
                    | "table:_cf_METADATA"
            )
        })
}

fn validate_draft(draft: &AccountDraftV1) -> std::result::Result<(), JsValue> {
    let entry = &draft.entry;
    let world = &draft.world_identity;
    if entry.environment != draft.plan.environment
        || entry.plan_digest != draft.plan.digest().map_err(|_| js_error())?
        || draft.plan.world_identity != world.world_id
        || [&world.world_id, &world.genesis_digest, &world.branch_id]
            .iter()
            .any(|v| v.trim().is_empty() || v.len() > 256)
        || world.service_public_key.len() != 64
        || !world
            .service_public_key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(js_error());
    }
    ProvisionRecord::draft(entry.clone(), draft.plan.clone()).map_err(|_| js_error())?;
    Ok(())
}
fn known_empty_draft(value: &str, target: &AccountFreezeTarget) -> Option<DraftRecord> {
    let draft: DraftRecord = serde_json::from_str(value).ok()?;
    if draft.schema_version != 1
        || draft.security_epoch.0 != 0
        || draft.audit_sequence.0 == 0
        || !draft.authorizations.is_empty()
        || !draft.outbox.is_empty()
    {
        return None;
    }
    let original = draft.draft.as_ref()?;
    validate_draft(original).ok()?;
    let entry = &original.entry;
    if entry.custody_account_id != target.custody_account_id
        || entry.manager != target.manager
        || entry.environment != target.environment
    {
        return None;
    }
    Some(draft)
}

#[durable_object]
pub struct AccountAuthority {
    state: JsValue,
    env: Option<Env>,
}

/// Match only the recognized, pristine draft payload. The caller must separately
/// check the complete Account inventory for a freeze row or unknown material.
pub(crate) fn matches_pristine_draft(value: &str, expected: &AccountDraftV1) -> bool {
    if value.len() > MAX_BYTES || validate_draft(expected).is_err() {
        return false;
    }
    serde_json::from_str::<DraftRecord>(value).is_ok_and(|record| {
        record.schema_version == 1
            && record.security_epoch.0 == 0
            && record.audit_sequence.0 > 0
            && record.authorizations.is_empty()
            && record.outbox.is_empty()
            && record.draft.as_ref() == Some(expected)
    })
}

/// Read only the recognized same-Account draft; final mutation separately checks inventory/freeze.
pub(crate) fn read_pristine_draft(state: &JsValue) -> std::result::Result<AccountDraftV1, JsValue> {
    let raw = read_record(state, DRAFT)?;
    if raw.len() > MAX_BYTES {
        return Err(js_error());
    }
    let saved: DraftRecord = serde_json::from_str(&raw).map_err(|_| js_error())?;
    let draft = saved.draft.as_ref().ok_or_else(js_error)?;
    if !matches_pristine_draft(&raw, draft) {
        return Err(js_error());
    }
    Ok(draft.clone())
}

#[cfg(feature = "approval-harness")]
pub(crate) fn initialize_fixture(state: JsValue, draft: AccountDraftV1) -> Result<Response> {
    AccountAuthority { state, env: None }.initialize_draft(draft)
}

#[cfg(feature = "approval-harness")]
pub(crate) fn freeze_fixture(state: JsValue, target: AccountFreezeTarget) -> Result<Response> {
    AccountAuthority { state, env: None }.freeze(target)
}
impl DurableObject for AccountAuthority {
    fn new(state: State, env: Env) -> Self {
        Self {
            state: state._inner().into(),
            env: Some(env),
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if request.method() != Method::Post
            || url.query().is_some()
            || !matches!(
                url.path(),
                "/internal/draft"
                    | "/internal/freeze"
                    | "/internal/approve"
                    | "/internal/advance"
                    | "/internal/provision-progress"
                    | "/internal/role-permit"
            )
        {
            return Response::error("not found", 404);
        }
        if request.headers().get("content-type")?.as_deref() != Some("application/json") {
            return Response::error("invalid content type", 415);
        }
        let body = match bounded_body(request.inner().as_ref(), MAX_BYTES as u32)
            .await
            .ok()
            .and_then(|v| v.as_string())
        {
            Some(value) => value,
            None => return Response::error("invalid or oversized body", 400),
        };
        match url.path() {
            "/internal/draft" => match serde_json::from_str::<AccountDraftV1>(&body) {
                Ok(entry) => self.initialize_draft(entry),
                Err(_) => Response::error("invalid request", 400),
            },
            "/internal/advance" => {
                let Some(env) = &self.env else {
                    return Response::error("not ready", 503);
                };
                let Ok(rpc) = serde_json::from_str::<crate::directory_rpc::DriveRpc>(&body) else {
                    return Response::error("invalid request", 400);
                };
                match crate::owner_provision::advance(&self.state, env, rpc).await {
                    Ok(view) => Response::from_json(&view),
                    Err(_) => Response::error("provision advance rejected or pending", 409),
                }
            }
            "/internal/provision-progress" => {
                let Some(env) = &self.env else {
                    return Response::error("not ready", 503);
                };
                let Ok(rpc) = serde_json::from_str::<crate::directory_rpc::ReadRpc>(&body) else {
                    return Response::error("invalid request", 400);
                };
                match crate::owner_provision::progress(&self.state, env, rpc) {
                    Ok(view) => Response::from_json(&view),
                    Err(_) => Response::error("progress unavailable", 409),
                }
            }
            "/internal/role-permit" => {
                let Some(env) = &self.env else {
                    return Response::error("not ready", 503);
                };
                let Ok(rpc) = serde_json::from_str::<crate::directory_rpc::RolePermitRpc>(&body)
                else {
                    return Response::error("invalid request", 400);
                };
                match crate::owner_provision::verify_role_permit(&self.state, env, rpc).await {
                    Ok(view) => Response::from_json(&view),
                    Err(_) => Response::error("role permit unavailable", 409),
                }
            }
            "/internal/approve" => {
                let Some(env) = &self.env else {
                    return Response::error("not ready", 503);
                };
                let Ok(rpc) = serde_json::from_str::<crate::directory_rpc::ApproveRpc>(&body)
                else {
                    return Response::error("invalid request", 400);
                };
                match crate::owner_provision::approve(&self.state, env, rpc).await {
                    Ok(view) => Response::from_json(&view),
                    Err(_) => Response::error("provision approval rejected or unavailable", 409),
                }
            }
            _ => match serde_json::from_str::<AccountFreezeTarget>(&body) {
                Ok(target) => self.freeze(target),
                Err(_) => Response::error("invalid request", 400),
            },
        }
    }
}
impl AccountAuthority {
    fn initialize_draft(&self, original_draft: AccountDraftV1) -> Result<Response> {
        let entry = &original_draft.entry;
        if validate_draft(&original_draft).is_err() {
            return Response::error("invalid draft", 400);
        }
        let initial = DraftRecord {
            schema_version: 1,
            draft: None,
            security_epoch: DecimalU64(0),
            authorizations: BTreeMap::new(),
            outbox: BTreeMap::new(),
            audit_sequence: DecimalU64(0),
        };
        let mut accepted = false;
        let mut callback = |old: String,
                            inventory: String|
         -> std::result::Result<Array, JsValue> {
            let inventory: Inventory = serde_json::from_str(&inventory).map_err(|_| js_error())?;
            if !(valid_inventory(&inventory)
                || known_provision_draft_inventory(&self.state, &inventory, &original_draft))
                || inventory.records.iter().any(|row| row.id == FREEZE)
            {
                return Err(js_error());
            }
            let mut draft: DraftRecord = serde_json::from_str(&old).map_err(|_| js_error())?;
            if draft.schema_version != 1
                || draft.security_epoch.0 != 0
                || !draft.authorizations.is_empty()
                || !draft.outbox.is_empty()
            {
                return Err(js_error());
            }
            if inventory
                .records
                .iter()
                .any(|row| row.id == "custody-provision-approval-v1")
                && draft.draft.is_none()
            {
                return Err(js_error());
            }
            if let Some(original) = &draft.draft {
                if draft.audit_sequence.0 == 0 || original != &original_draft {
                    return Err(js_error());
                }
            } else {
                if draft.audit_sequence.0 != 0 {
                    return Err(js_error());
                }
                draft.draft = Some(original_draft.clone());
                draft.audit_sequence =
                    DecimalU64(draft.audit_sequence.0.checked_add(1).ok_or_else(js_error)?);
            }
            accepted = true;
            result_array(
                encode(&draft)?,
                draft.audit_sequence.0,
                "draft_initialized",
                &entry.custody_account_id,
            )
        };
        let closure = Closure::borrow_mut(&mut callback);
        let result = transition(
            &self.state,
            DRAFT,
            &encode(&initial).map_err(|_| Error::RustError("invalid draft".into()))?,
            closure.as_ref().unchecked_ref(),
        );
        drop(closure);
        if result.is_err() || !accepted {
            return Response::error("conflict or frozen", 409);
        }
        Response::from_json(&ProvisionDraftViewV1 {
            api_version: 1,
            phase: DraftPhase::DraftInitialized,
            entry: original_draft.entry,
            plan: original_draft.plan,
            world_identity: original_draft.world_identity,
            security_epoch: DecimalU64(0),
        })
    }
    fn freeze(&self, target: AccountFreezeTarget) -> Result<Response> {
        if target.minimum_security_epoch.0 == 0 || target.revocation_digest.len() != 64 {
            return Response::error("invalid freeze target", 400);
        }
        let initial = FreezeRecord {
            schema_version: 1,
            target: target.clone(),
            frozen: true,
            security_epoch: target.minimum_security_epoch,
            receipt: None,
            audit_sequence: DecimalU64(0),
        };
        let mut receipt = None;
        let mut callback = |old: String,
                            inventory: String|
         -> std::result::Result<Array, JsValue> {
            // The freeze row's existence itself permanently blocks draft init. An
            // unknown existing row is preserved and can never prove empty state.
            let Ok(mut frozen) = serde_json::from_str::<FreezeRecord>(&old) else {
                return result_array(old, 0, "unknown_frozen", &target.custody_account_id);
            };
            if frozen.schema_version != 1
                || frozen.target != target
                || !frozen.frozen
                || frozen.security_epoch.0 < target.minimum_security_epoch.0
            {
                return result_array(old, 0, "unknown_frozen", &target.custody_account_id);
            }
            let inventory = serde_json::from_str::<Inventory>(&inventory).ok();
            let draft = inventory
                .as_ref()
                .and_then(|inventory| inventory.records.iter().find(|row| row.id == DRAFT));
            let known = inventory.as_ref().is_some_and(valid_inventory)
                && draft.is_none_or(|row| known_empty_draft(&row.payload, &target).is_some());
            if known {
                if let Some(row) = draft {
                    let draft = known_empty_draft(&row.payload, &target).ok_or_else(js_error)?;
                    frozen.security_epoch = DecimalU64(
                        frozen
                            .security_epoch
                            .0
                            .max(draft.security_epoch.0.checked_add(1).ok_or_else(js_error)?),
                    );
                }
                {
                    let inventory_digest =
                        digest("oasis7.custody.account-empty-authorizations.v1", &target)?;
                    let barrier_digest = digest(
                        "oasis7.custody.account-empty-outbox.v1",
                        &(&target, frozen.security_epoch),
                    )?;
                    let receipt_digest = digest(
                        "oasis7.custody.account-freeze-receipt.v1",
                        &(
                            &target,
                            frozen.security_epoch,
                            &inventory_digest,
                            &barrier_digest,
                        ),
                    )?;
                    let expected = AccountFreezeReceipt {
                        target: target.clone(),
                        frozen_security_epoch: frozen.security_epoch,
                        authorization_closure:
                            AccountAuthorizationClosure::AllManagerRootsAndChildrenRevoked {
                                inventory_digest,
                            },
                        dispatch_barrier: AccountDispatchBarrier::AllUnclaimedBlocked {
                            barrier_digest,
                        },
                        receipt_digest,
                        claimed_operation_ids: BTreeSet::new(),
                    };
                    if frozen.receipt.is_none() {
                        frozen.receipt = Some(expected.clone());
                    }
                    if frozen.receipt.as_ref() == Some(&expected) {
                        receipt = Some(expected);
                    }
                }
            }
            let next_changed = frozen.audit_sequence.0 == 0 || encode(&frozen)? != old;
            if next_changed {
                frozen.audit_sequence = DecimalU64(
                    frozen
                        .audit_sequence
                        .0
                        .checked_add(1)
                        .ok_or_else(js_error)?,
                );
            }
            result_array(
                encode(&frozen)?,
                frozen.audit_sequence.0,
                "account_frozen",
                &target.custody_account_id,
            )
        };
        let closure = Closure::borrow_mut(&mut callback);
        let result = transition(
            &self.state,
            FREEZE,
            &encode(&initial).map_err(|_| Error::RustError("invalid freeze".into()))?,
            closure.as_ref().unchecked_ref(),
        );
        drop(closure);
        if result.is_err() {
            return Response::error("freeze unavailable", 503);
        }
        match receipt {
            Some(receipt) => Response::from_json(&receipt),
            None => Response::error("freeze pending", 503),
        }
    }
}
