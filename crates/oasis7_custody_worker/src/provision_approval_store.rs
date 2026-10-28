//! Private Account component: approval consumption and provision preparation share one SQL row.
//! The fixed Account approval consumer is wired; no dispatch, key generation, or closure acknowledgement is installed.
#[path = "provision_native_store.rs"]
pub mod native;

#[cfg(feature = "approval-harness")]
#[path = "account_restore_perf.rs"]
pub(crate) mod performance;
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
        ProvisionPolicy, ProvisionRoleKeyResultV1, ProvisionRoutes, PublicRoleKey,
    },
};
use oasis7_custody_core::{
    http::Role,
    provisioning::{ProvisionOriginInspectionV1, ProvisionRoleKeyPermitViewV1},
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
    fn update_raw(
        state: &JsValue,
        key: &str,
        initial: &str,
        transition: &js_sys::Function,
    ) -> Result<String>;
    #[wasm_bindgen(catch, js_name = readTextRecord)]
    fn read(state: &JsValue, key: &str) -> Result<String>;
}

// This FFI invokes its Rust SQL callback synchronously. The scope ends before
// any later await; SQL, Current, clock, inventory and every transition still run.
fn update(
    state: &JsValue,
    key: &str,
    initial: &str,
    transition: &js_sys::Function,
) -> Result<String> {
    oasis7_custody_core::with_owner_signature_verification_batch(|| {
        update_raw(state, key, initial, transition)
    })
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
    #[cfg(feature = "approval-harness")]
    let _decode = performance::decode();
    if value.len() > MAX_BYTES {
        return Err(rejected());
    }
    let value: ProvisionApprovalSnapshot = serde_json::from_str(value).map_err(|_| rejected())?;
    if value.schema_version != 1 {
        return Err(rejected());
    }
    Ok(value)
}
// Keep the validated immutable comparison baseline from the same SQL callback
// bytes. Re-decoding those identical bytes cannot observe a newer SQL state.
#[cfg(feature = "approval-harness")]
fn transition_snapshot(
    value: &str,
) -> Result<(ProvisionApprovalSnapshot, AccountProvisionApprovalState)> {
    let snapshot = decode(value)?;
    let previous = snapshot.state.clone();
    Ok((snapshot, previous))
}
/// Recognize private persisted approval state solely for exact historical draft retry.
pub(crate) fn matches_persisted_draft(state: &JsValue, draft: &AccountDraftV1) -> bool {
    read(state, KEY).is_ok_and(|raw| decode(&raw).is_ok_and(|saved| saved.state.draft() == draft))
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

#[derive(Default)]
struct SnapshotReader(std::cell::RefCell<Option<(String, ProvisionApprovalSnapshot)>>);
impl SnapshotReader {
    fn restore(&self, raw: &str) -> Result<ProvisionApprovalSnapshot> {
        if let Some((previous, snapshot)) = self.0.borrow().as_ref()
            && previous == raw
        {
            return Ok(snapshot.clone());
        }
        let snapshot = decode(raw)?;
        self.0.replace(Some((raw.to_owned(), snapshot.clone())));
        Ok(snapshot)
    }
    fn transition(
        &self,
        raw: &str,
    ) -> Result<(ProvisionApprovalSnapshot, AccountProvisionApprovalState)> {
        let snapshot = self.restore(raw)?;
        let previous = snapshot.state.clone();
        Ok((snapshot, previous))
    }
}

pub struct OwnerProvisionApprovalStore {
    state: JsValue,
    // Historical restore only. Every use still reads SQL and every transition
    // obtains current authority, clock and inventory independently.
    reader: SnapshotReader,
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
        Ok(Self {
            state,
            reader: SnapshotReader::default(),
        })
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
        Ok(Self {
            state,
            reader: SnapshotReader(std::cell::RefCell::new(Some((original, saved)))),
        })
    }
    pub fn snapshot(&self) -> Result<ProvisionApprovalSnapshot> {
        self.reader.restore(&read(&self.state, KEY)?)
    }
    // Budget/nonce reservations only. This port cannot return a send or key permit.
    fn local_reservation<R, T>(&self, event: &str, reference: &str, mut transition: T) -> Result<R>
    where
        T: FnMut(&mut AccountProvisionApprovalState, u64) -> Result<R>,
    {
        let initial = read(&self.state, KEY)?;
        let mut returned = None;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            #[cfg(feature = "approval-harness")]
            let _performance = performance::callback(event, &old, &self.state);
            let (mut next, previous) = self.reader.transition(&old)?;
            validate_inventory(&inventory, next.state.draft(), true)?;
            let result = transition(&mut next.state, now()?)?;
            let array = if next.state == previous {
                unchanged(&old)
            } else {
                next.audit_sequence =
                    DecimalU64(next.audit_sequence.0.checked_add(1).ok_or_else(rejected)?);
                let audit = serde_json::to_string(&(next.audit_sequence, event, reference))
                    .map_err(|_| rejected())?;
                let array = Array::new();
                array.push(&encode(&next)?.into());
                array.push(&next.audit_sequence.0.to_string().into());
                array.push(&audit.into());
                array
            };
            returned = Some(result);
            Ok(array)
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(rejected)
    }
    pub fn reserve_origin_inspection(&self, id: &str) -> Result<ProvisionOriginInspectionV1> {
        let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.local_reservation("origin_transport_reserved", id, |state, time| {
            state
                .reserve_origin_inspection(id, &nonce, time)
                .map_err(|_| rejected())
        })
    }
    pub fn reserve_role_key_permit_read(&self, permit_id: &str, role: Role) -> Result<u8> {
        self.local_reservation("role_oracle_reserved", permit_id, |state, time| {
            state
                .reserve_role_key_permit_read(permit_id, role, time)
                .map_err(|_| rejected())
        })
    }
    pub async fn authorize_reserved_role_key_permit_read<F>(
        &self,
        permit_id: &str,
        ticket: u8,
        role: Role,
        current: F,
    ) -> Result<ProvisionRoleKeyPermitViewV1>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        use oasis7_custody_core::approval_admission::ApprovalSignatureVerifier;
        let snapshot = self.snapshot()?;
        let trusted = current()?;
        let checks = snapshot
            .state
            .role_key_permit_checks(permit_id, role, &trusted.as_core(), now()?)
            .map_err(|_| rejected())?;
        let mut cache = verify(checks).await?;
        self.provision_step(
            &current,
            "role_oracle_authorized",
            permit_id,
            |state, context, time| {
                let dispatch = state
                    .role_key_dispatches()
                    .values()
                    .find(|d| d.permit_id == permit_id)
                    .ok_or_else(rejected)?;
                if ticket == 0 || ticket > dispatch.permit_reads {
                    return Err(rejected());
                }
                cache
                    .verify(
                        &state
                            .role_key_permit_checks(permit_id, role, context, time)
                            .map_err(|_| rejected())?,
                    )
                    .map_err(|_| rejected())?;
                state
                    .validate_role_key_permit(permit_id, role, context, time)
                    .map_err(|_| rejected())
            },
        )
    }

    fn provision_step<F, R, T>(
        &self,
        current: &F,
        event: &str,
        reference: &str,
        mut transition: T,
    ) -> Result<R>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &ProvisionApprovalContext<'_>,
            u64,
        ) -> Result<R>,
    {
        let initial = read(&self.state, KEY)?;
        let mut returned = None;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            #[cfg(feature = "approval-harness")]
            let _performance = performance::callback(event, &old, &self.state);
            let (mut next, previous) = self.reader.transition(&old)?;
            let trusted = current()?;
            let result = transition(&mut next.state, &trusted.as_core(), now()?)?;
            let array = if next.state == previous {
                validate_inventory(&inventory, next.state.draft(), false)?;
                unchanged(&old)
            } else {
                validate_inventory(&inventory, next.state.draft(), true)?;
                next.audit_sequence =
                    DecimalU64(next.audit_sequence.0.checked_add(1).ok_or_else(rejected)?);
                let audit = serde_json::to_string(&(
                    next.audit_sequence,
                    event,
                    reference,
                    trusted.approval.authority.service_epoch,
                ))
                .map_err(|_| rejected())?;
                let array = Array::new();
                array.push(&encode(&next)?.into());
                array.push(&next.audit_sequence.0.to_string().into());
                array.push(&audit.into());
                array
            };
            returned = Some(result);
            Ok(array)
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(rejected)
    }

    pub fn begin_origin_inspection<F>(
        &self,
        id: &str,
        current: F,
    ) -> Result<ProvisionOriginInspectionV1>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.provision_step(
            &current,
            "origin_inspection_pending",
            id,
            |state, context, time| {
                state
                    .begin_origin_inspection(id, &nonce, context, time)
                    .map_err(|_| rejected())
            },
        )
    }
    pub fn claim_origin_inspection<F>(&self, id: &str, current: F) -> Result<String>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        self.provision_step(
            &current,
            "origin_inspection_claimed",
            id,
            |state, context, time| {
                state
                    .claim_origin_inspection(id, context, time)
                    .map_err(|_| rejected())?;
                state
                    .origin_inspection_request_bytes(id)
                    .map(str::to_owned)
                    .map_err(|_| rejected())
            },
        )
    }
    pub async fn complete_origin_inspection<F>(
        &self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<ProvisionOriginInspectionV1>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        let snapshot = self.snapshot()?;
        let trusted = current()?;
        let checks = snapshot
            .state
            .origin_inspection_checks(id, &signed, &trusted.as_core(), now()?)
            .map_err(|_| rejected())?;
        let mut cache = verify(checks).await?;
        self.provision_step(
            &current,
            "origin_inspection_verified",
            id,
            |state, context, time| {
                state
                    .complete_origin_inspection(id, signed.clone(), context, time, &mut cache)
                    .map_err(|_| rejected())
            },
        )
    }
    pub async fn claim_role_key_dispatch<F>(
        &self,
        id: &str,
        role: Role,
        inspection_id: &str,
        current: F,
    ) -> Result<ProvisionRoleKeyPermitViewV1>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        let snapshot = self.snapshot()?;
        let trusted = current()?;
        let checks = snapshot
            .state
            .role_key_dispatch_checks(role, inspection_id, &trusted.as_core(), now()?)
            .map_err(|_| rejected())?;
        let mut cache = verify(checks).await?;
        self.provision_step(
            &current,
            "role_dispatch_claimed",
            id,
            |state, context, time| {
                state
                    .claim_role_key_dispatch(id, role, inspection_id, context, time, &mut cache)
                    .map_err(|_| rejected())
            },
        )
    }
    pub async fn claim_role_key_permit_read<F>(
        &self,
        permit_id: &str,
        role: Role,
        current: F,
    ) -> Result<ProvisionRoleKeyPermitViewV1>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        let snapshot = self.snapshot()?;
        let trusted = current()?;
        let checks = snapshot
            .state
            .role_key_permit_checks(permit_id, role, &trusted.as_core(), now()?)
            .map_err(|_| rejected())?;
        let mut cache = verify(checks).await?;
        self.provision_step(
            &current,
            "role_permit_read",
            permit_id,
            |state, context, time| {
                state
                    .claim_role_key_permit_read(permit_id, role, context, time, &mut cache)
                    .map_err(|_| rejected())
            },
        )
    }
    pub fn record_dispatched_role_key_result<F>(
        &self,
        dispatch_id: &str,
        result: oasis7_custody_core::provisioning::ProvisionRoleKeyResultV1,
        current: F,
    ) -> Result<oasis7_custody_core::provisioning::PublicRoleKey>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        self.provision_step(
            &current,
            "role_dispatch_settled",
            dispatch_id,
            |state, context, time| {
                state
                    .record_dispatched_role_key_result(dispatch_id, result.clone(), context, time)
                    .map_err(|_| rejected())
            },
        )
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
    /// Only the fixed-role coordinator may supply an authenticated role response.
    /// Current source observations must come from separately verified inspect_origin,
    /// not an expired approval proof or a caller-provided active flag.
    pub fn record_role_key_result<F>(
        &self,
        result: ProvisionRoleKeyResultV1,
        current: F,
    ) -> Result<PublicRoleKey>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
    {
        self.record_role_key_result_with_clock(result, current, now)
    }
    pub fn record_role_key_result_with_clock<F, C>(
        &self,
        result: ProvisionRoleKeyResultV1,
        current: F,
        clock: C,
    ) -> Result<PublicRoleKey>
    where
        F: Fn() -> Result<ProvisionApprovalCurrent>,
        C: Fn() -> Result<u64>,
    {
        let initial = read(&self.state, KEY)?;
        let mut returned = None;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            let mut next = decode(&old)?;
            let trusted = current()?;
            let public = next
                .state
                .record_role_key_result(result.clone(), &trusted.as_core(), clock()?)
                .map_err(|_| rejected())?;
            let response = if next.state == decode(&old)?.state {
                validate_inventory(&inventory, next.state.draft(), false)?;
                unchanged(&old)
            } else {
                validate_inventory(&inventory, next.state.draft(), true)?;
                next.audit_sequence =
                    DecimalU64(next.audit_sequence.0.checked_add(1).ok_or_else(rejected)?);
                let audit = serde_json::to_string(&(
                    next.audit_sequence,
                    "role_key_recorded",
                    &result.scope.operation_id,
                    &result.scope.provision_id,
                    result.scope.context.identity.role,
                    &public,
                ))
                .map_err(|_| rejected())?;
                let response = Array::new();
                response.push(&encode(&next)?.into());
                response.push(&next.audit_sequence.0.to_string().into());
                response.push(&audit.into());
                response
            };
            returned = Some(public);
            Ok(response)
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(rejected)
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
