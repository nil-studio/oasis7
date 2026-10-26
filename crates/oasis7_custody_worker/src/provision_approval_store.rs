//! Private Account component: approval consumption and provision preparation share one SQL row.
//! No production caller, dispatch, key generation, or closure acknowledgement is installed.
use crate::approval_store::{now, verify};
use js_sys::Array;
use oasis7_custody_core::{
    DecimalU64,
    approval::{ApprovedWorldV1, SignedApprovalVerificationV1},
    approval_admission::{
        ApprovalAdmissionContext, ApprovalAdmissionInput, ApprovalAdmissionRecord,
    },
    provisioning::{
        AccountDraftV1, AccountProvisionApprovalState, BootstrapManager, ProvisionApprovalContext,
        ProvisionPolicy, ProvisionRoutes,
    },
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};

const KEY: &str = "custody-provision-approval-v1";
const DRAFT: &str = "custody-account-draft-v1";
const FREEZE: &str = "custody-account-freeze-v1";
const MAX_BYTES: usize = 2 * 1024 * 1024;
type Result<T> = std::result::Result<T, JsValue>;

#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = accountTransition)]
    fn update(
        state: &JsValue,
        key: &str,
        initial: &str,
        transition: &js_sys::Function,
    ) -> Result<String>;
    #[wasm_bindgen(catch, js_name = readTextRecord)]
    fn read(state: &JsValue, key: &str) -> Result<String>;
}

/// Owned trusted reads allow every admission phase to obtain fresh authority/configuration.
/// Deliberately not Deserialize: an HTTP request cannot supply current authority.
#[derive(Clone)]
pub struct ProvisionApprovalCurrent {
    pub approval: ApprovalAdmissionContext,
    pub routes: ProvisionRoutes,
    pub managers: Vec<BootstrapManager>,
    pub policy_version: DecimalU64,
    pub world_identity: ApprovedWorldV1,
    pub native_identity_namespace: String,
}
impl ProvisionApprovalCurrent {
    fn as_core(&self) -> ProvisionApprovalContext<'_> {
        ProvisionApprovalContext {
            approval: &self.approval,
            policy: ProvisionPolicy {
                routes: &self.routes,
                managers: &self.managers,
                version: self.policy_version.0,
            },
            world_identity: &self.world_identity,
            native_identity_namespace: &self.native_identity_namespace,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionApprovalSnapshot {
    pub schema_version: u8,
    pub state: AccountProvisionApprovalState,
    pub audit_sequence: DecimalU64,
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
fn rejected() -> JsValue {
    JsValue::from_str("provision approval rejected")
}
fn decode(value: &str) -> Result<ProvisionApprovalSnapshot> {
    if value.len() > MAX_BYTES {
        return Err(rejected());
    }
    let value: ProvisionApprovalSnapshot = serde_json::from_str(value).map_err(|_| rejected())?;
    if value.schema_version != 1 {
        return Err(rejected());
    }
    Ok(value)
}
fn encode(value: &ProvisionApprovalSnapshot) -> Result<String> {
    let value = serde_json::to_string(value).map_err(|_| rejected())?;
    if value.len() > MAX_BYTES {
        return Err(rejected());
    }
    Ok(value)
}
fn validate_inventory(raw: &str, draft: &AccountDraftV1, require_open: bool) -> Result<()> {
    let inventory: Inventory = serde_json::from_str(raw).map_err(|_| rejected())?;
    // Keep the old production freezer conservative: this new private row is intentionally
    // unknown there until a later explicit inventory/closure implementation exists.
    let row = inventory
        .records
        .iter()
        .find(|row| row.id == DRAFT)
        .ok_or_else(rejected)?;
    if !crate::owner_account::matches_pristine_draft(&row.payload, draft) {
        return Err(rejected());
    }
    if require_open
        && (inventory.records.len() != 2
            || inventory
                .records
                .iter()
                .any(|row| row.id != DRAFT && row.id != KEY)
            || inventory.records.iter().any(|row| row.id == FREEZE)
            || inventory.table_names.iter().any(|name| {
                !matches!(
                    name.as_str(),
                    "table:custody_records"
                        | "table:custody_audit"
                        | "table:sqlite_sequence"
                        | "table:_cf_METADATA"
                )
            }))
    {
        return Err(rejected());
    }
    Ok(())
}
fn unchanged(raw: &str) -> Array {
    let result = Array::new();
    for value in [raw, "", ""] {
        result.push(&value.into());
    }
    result
}

pub struct OwnerProvisionApprovalStore {
    state: JsValue,
}
impl OwnerProvisionApprovalStore {
    /// The exact draft must already exist in this Account's SQL storage.
    pub fn initialize(
        state: JsValue,
        draft: AccountDraftV1,
        _current: ProvisionApprovalCurrent,
    ) -> Result<Self> {
        let initial = encode(&ProvisionApprovalSnapshot {
            schema_version: 1,
            state: AccountProvisionApprovalState::new(draft.clone()).map_err(|_| rejected())?,
            audit_sequence: DecimalU64(0),
        })?;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            let saved = decode(&old)?;
            if saved.state.draft() != &draft {
                return Err(rejected());
            }
            validate_inventory(&inventory, &draft, saved.audit_sequence.0 == 0)?;
            Ok(unchanged(&old))
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        Ok(Self { state })
    }
    /// Reopen only an existing component record; never inserts an empty approval authority.
    pub fn open(state: JsValue) -> Result<Self> {
        let original = read(&state, KEY)?;
        let saved = decode(&original)?;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            if old != original {
                return Err(rejected());
            }
            validate_inventory(&inventory, saved.state.draft(), false)?;
            Ok(unchanged(&old))
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&state, KEY, &original, closure.as_ref().unchecked_ref())?;
        drop(closure);
        Ok(Self { state })
    }
    pub fn snapshot(&self) -> Result<ProvisionApprovalSnapshot> {
        decode(&read(&self.state, KEY)?)
    }

    fn transaction<F, C, T>(
        &self,
        current: &F,
        clock: &C,
        operation: &str,
        mut transition: T,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
        C: Fn() -> Result<u64>,
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &ProvisionApprovalContext<'_>,
            u64,
        ) -> Result<ApprovalAdmissionRecord>,
    {
        let initial = read(&self.state, KEY)?;
        let mut returned = None;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            let mut next = decode(&old)?;
            let trusted = current()?;
            let record = transition(&mut next.state, &trusted.as_core(), clock()?)?;
            let previous = decode(&old)?;
            let result = if next.state == previous.state {
                // Historical exact retries return original evidence without new authority or writes.
                validate_inventory(&inventory, next.state.draft(), false)?;
                unchanged(&old)
            } else {
                validate_inventory(&inventory, next.state.draft(), true)?;
                next.audit_sequence =
                    DecimalU64(next.audit_sequence.0.checked_add(1).ok_or_else(rejected)?);
                let audit = serde_json::to_string(&(
                    next.audit_sequence,
                    operation,
                    &record.input().operation_id,
                    &record.input().approval.proof.approval_id,
                    trusted.approval.authority.service_epoch,
                ))
                .map_err(|_| rejected())?;
                let result = Array::new();
                result.push(&encode(&next)?.into());
                result.push(&next.audit_sequence.0.to_string().into());
                result.push(&audit.into());
                result
            };
            returned = Some(record);
            Ok(result)
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(rejected)
    }
    fn tighten<T>(&self, operation: &str, mut transition: T) -> Result<ProvisionApprovalSnapshot>
    where
        T: FnMut(&mut AccountProvisionApprovalState) -> Result<()>,
    {
        let initial = read(&self.state, KEY)?;
        let mut returned = None;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            let mut next = decode(&old)?;
            validate_inventory(&inventory, next.state.draft(), false)?;
            transition(&mut next.state)?;
            let result = if next.state == decode(&old)?.state {
                unchanged(&old)
            } else {
                next.audit_sequence =
                    DecimalU64(next.audit_sequence.0.checked_add(1).ok_or_else(rejected)?);
                let audit = serde_json::to_string(&(
                    next.audit_sequence,
                    operation,
                    &next.state.draft().entry.custody_account_id,
                ))
                .map_err(|_| rejected())?;
                let result = Array::new();
                result.push(&encode(&next)?.into());
                result.push(&next.audit_sequence.0.to_string().into());
                result.push(&audit.into());
                result
            };
            returned = Some(next);
            Ok(result)
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(rejected)
    }
    /// Trusted internal restriction only; does not interpret inventory as a closed receipt.
    pub fn freeze(&self, epoch: u64) -> Result<ProvisionApprovalSnapshot> {
        self.tighten("frozen", |state| {
            state.freeze(epoch).map_err(|_| rejected())
        })
    }
    pub fn cancel(&self, operation_id: &str, epoch: u64) -> Result<ProvisionApprovalSnapshot> {
        self.cancel_with_clock(operation_id, epoch, now)
    }
    pub fn cancel_with_clock<C>(
        &self,
        operation_id: &str,
        epoch: u64,
        clock: C,
    ) -> Result<ProvisionApprovalSnapshot>
    where
        C: Fn() -> Result<u64>,
    {
        self.tighten("cancelled", |state| {
            state
                .cancel(operation_id, epoch, clock()?)
                .map_err(|_| rejected())
        })
    }
    pub async fn begin<F>(
        &self,
        input: ApprovalAdmissionInput,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        self.begin_with_clock(input, current, now).await
    }
    pub async fn begin_with_clock<F, C>(
        &self,
        input: ApprovalAdmissionInput,
        current: F,
        clock: C,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
        C: Fn() -> Result<u64>,
    {
        let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        for attempt in 0..2 {
            let snapshot = self.snapshot()?;
            let trusted = current()?;
            let checks = snapshot
                .state
                .begin_checks(&input, &nonce, &trusted.as_core(), clock()?)
                .map_err(|_| rejected())?;
            let mut cache = verify(checks).await?;
            let result = self.transaction(&current, &clock, "pending", |state, trusted, time| {
                state
                    .begin(input.clone(), nonce.clone(), trusted, time, &mut cache)
                    .map_err(|_| rejected())
            });
            if result.is_ok() || attempt == 1 {
                return result;
            }
            let latest = self.snapshot()?;
            if !latest
                .state
                .original(&input.operation_id)
                .is_some_and(|record| {
                    record.input() == &input && record.verify_nonce() != cache.verify_nonce()
                })
            {
                return result;
            }
        }
        unreachable!()
    }
    pub async fn commit<F>(
        &self,
        id: &str,
        verification: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        self.commit_with_clock(id, verification, current, now).await
    }
    pub async fn commit_with_clock<F, C>(
        &self,
        id: &str,
        verification: SignedApprovalVerificationV1,
        current: F,
        clock: C,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
        C: Fn() -> Result<u64>,
    {
        let snapshot = self.snapshot()?;
        let trusted = current()?;
        let checks = snapshot
            .state
            .commit_checks(id, &verification, &trusted.as_core(), clock()?)
            .map_err(|_| rejected())?;
        let mut cache = verify(checks).await?;
        self.transaction(&current, &clock, "approved", |state, trusted, time| {
            state
                .commit(id, verification.clone(), trusted, time, &mut cache)
                .map_err(|_| rejected())
        })
    }
}
