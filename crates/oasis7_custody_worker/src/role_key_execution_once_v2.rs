//! Independent V2 signer once inventory; the old schema1 slot stays unchanged.
use super::*;
use oasis7_custody_core::{
    approval::bytes_digest,
    provisioning::{ExecutionPrepareSignerOperationV2, VerifiedExecutionPreparePermitV2},
};
const MAX_ONCE: usize = 1024; // Existing 256 SessionV2 records times four fixed stages.
const MAX_ONCE_BYTES: usize = 4096;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateV2 {
    api_version: u8,
    scope_digest: String,
    immutable_slot_digest: String,
    count: DecimalU64,
    head: String,
    clock_floor: DecimalU64,
    initialized_at: DecimalU64,
    storage_version: u8,
    initialization_count: DecimalU64,
    initialization_head: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimV2 {
    api_version: u8,
    sequence: DecimalU64,
    operation_digest: String,
    permit_digest: String,
    stage_operation_hash: String,
    nonce_hashes: [String; 3],
    bundle_digest: String,
    previous: String,
    claimed_at: DecimalU64,
    issued_at: DecimalU64,
    expires_at: DecimalU64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RowV2 {
    operation_id: String,
    payload: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryV2 {
    sign: Vec<RowV2>,
    initializations: Vec<RowV2>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InitializationV2 {
    sequence: DecimalU64,
    operation_digest: String,
    permit_digest: String,
    previous: String,
    claimed_at: DecimalU64,
    issued_at: DecimalU64,
    expires_at: DecimalU64,
}
fn hash<T: Serialize>(v: &T) -> Result<String> {
    oasis7_wasm_abi::encode_canonical_cbor(v)
        .map(|b| bytes_digest(&b))
        .map_err(|_| rejected())
}
fn canonical_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn initial(scope: &RoleKeySlotScopeV1, slot: &str, at: u64) -> Result<StateV2> {
    let scope_digest = hash(&("oasis7.execution-signer.scope.v2", scope))?;
    let immutable_slot_digest = bytes_digest(slot.as_bytes());
    let head = hash(&(
        "oasis7.execution-signer.genesis.v2",
        &scope_digest,
        &immutable_slot_digest,
        at,
    ))?;
    Ok(StateV2 {
        api_version: 2,
        scope_digest: scope_digest.clone(),
        immutable_slot_digest: immutable_slot_digest.clone(),
        count: DecimalU64(0),
        head,
        clock_floor: DecimalU64(at),
        initialized_at: DecimalU64(at),
        storage_version: 3,
        initialization_count: DecimalU64(0),
        initialization_head: hash(&(
            "oasis7.execution-signer.initialization-genesis.v2",
            &scope_digest,
            &immutable_slot_digest,
            at,
        ))?,
    })
}
fn encode_v2<T: Serialize>(v: &T) -> Result<String> {
    let text = serde_json::to_string(v).map_err(|_| rejected())?;
    if text.len() > MAX_ONCE_BYTES {
        return Err(rejected());
    }
    Ok(text)
}
fn validate(raw: &str, rows: &str, expected: &StateV2) -> Result<StateV2> {
    if raw.len() > MAX_ONCE_BYTES
        || rows.len()
            > MAX_ONCE
                .checked_mul(2 * (MAX_ONCE_BYTES + 128))
                .ok_or_else(rejected)?
    {
        return Err(rejected());
    }
    let state: StateV2 = serde_json::from_str(raw).map_err(|_| rejected())?;
    let inventory: InventoryV2 = serde_json::from_str(rows).map_err(|_| rejected())?;
    let rows = inventory.sign;
    if state.storage_version != 3
        || inventory.initializations.len() > MAX_ONCE
        || state.initialization_count.0 != inventory.initializations.len() as u64
    {
        return Err(rejected());
    }
    let mut initialization_records = Vec::new();
    let mut initialization_ids = std::collections::BTreeSet::new();
    for row in inventory.initializations {
        if row.payload.len() > MAX_ONCE_BYTES
            || !canonical_hex(&row.operation_id)
            || !initialization_ids.insert(row.operation_id.clone())
        {
            return Err(rejected());
        }
        let record: InitializationV2 =
            serde_json::from_str(&row.payload).map_err(|_| rejected())?;
        if record.operation_digest != row.operation_id
            || !canonical_hex(&record.permit_digest)
            || record.issued_at.0 > record.claimed_at.0
            || record.claimed_at.0 >= record.expires_at.0
        {
            return Err(rejected());
        }
        initialization_records.push(record);
    }
    initialization_records.sort_by_key(|r| r.sequence.0);
    let mut initialization_head = hash(&(
        "oasis7.execution-signer.initialization-genesis.v2",
        &state.scope_digest,
        &state.immutable_slot_digest,
        state.initialized_at.0,
    ))?;
    let mut initialization_floor = state.initialized_at.0;
    for (i, record) in initialization_records.iter().enumerate() {
        if record.sequence.0 != (i as u64).checked_add(1).ok_or_else(rejected)?
            || record.previous != initialization_head
            || record.claimed_at.0 < initialization_floor
        {
            return Err(rejected());
        }
        initialization_head = hash(&("oasis7.execution-signer.initialization.v2", record))?;
        initialization_floor = record.claimed_at.0;
    }
    if initialization_head != state.initialization_head {
        return Err(rejected());
    }
    if state.api_version != 2
        || state.scope_digest != expected.scope_digest
        || state.immutable_slot_digest != expected.immutable_slot_digest
        || rows.len() > MAX_ONCE
        || state.count.0 != rows.len() as u64
    {
        return Err(rejected());
    }
    let mut ordered = Vec::with_capacity(rows.len());
    let mut ids = std::collections::BTreeSet::new();
    let mut identifiers = std::collections::BTreeSet::new();
    for row in rows {
        if row.payload.len() > MAX_ONCE_BYTES
            || !canonical_hex(&row.operation_id)
            || !ids.insert(row.operation_id.clone())
        {
            return Err(rejected());
        }
        let record: ClaimV2 = serde_json::from_str(&row.payload).map_err(|_| rejected())?;
        if record.api_version != 2
            || record.operation_digest != row.operation_id
            || !canonical_hex(&record.permit_digest)
            || !canonical_hex(&record.bundle_digest)
            || record.issued_at.0 > record.claimed_at.0
            || record.claimed_at.0 >= record.expires_at.0
        {
            return Err(rejected());
        }
        for identifier in
            std::iter::once(&record.stage_operation_hash).chain(record.nonce_hashes.iter())
        {
            if !canonical_hex(identifier) || !identifiers.insert(identifier.clone()) {
                return Err(rejected());
            }
        }
        ordered.push(record);
    }
    ordered.sort_by_key(|v| v.sequence.0);
    let mut head = hash(&(
        "oasis7.execution-signer.genesis.v2",
        &state.scope_digest,
        &state.immutable_slot_digest,
        state.initialized_at.0,
    ))?;
    let mut floor = state.initialized_at.0;
    for (i, record) in ordered.iter().enumerate() {
        if record.sequence.0 != (i as u64).checked_add(1).ok_or_else(rejected)?
            || record.previous != head
            || record.claimed_at.0 < floor
        {
            return Err(rejected());
        }
        head = hash(&("oasis7.execution-signer.claim.v2", record))?;
        floor = record.claimed_at.0;
    }
    if state.head != head || state.clock_floor.0 != floor.max(initialization_floor) {
        return Err(rejected());
    }
    Ok(state)
}
/// Private construction follows a committed INSERT, never a decoded row. The
/// verified token by itself cannot bypass the actual physical once admission.
pub(super) struct ClaimedExecutionPrepareV2 {
    pub(super) verified: VerifiedExecutionPreparePermitV2,
    pub(super) immutable_slot: String,
}
#[wasm_bindgen(module = "/platform/execution_signer_sql_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = initializeExecutionSignerV2)]
    fn initialize_sql(
        state: &JsValue,
        key: &str,
        slot: &str,
        initial: &str,
        validate: &js_sys::Function,
    ) -> Result<String>;
    #[wasm_bindgen(catch, js_name = claimExecutionSignerV2)]
    fn claim_sql(
        state: &JsValue,
        key: &str,
        slot: &str,
        transition: &js_sys::Function,
    ) -> Result<String>;
}
impl RoleKeyStore {
    fn execution_v2_slot(
        &self,
        verified: &VerifiedExecutionPreparePermitV2,
    ) -> Result<(String, StateV2)> {
        if COMPILED_ROLE != Role::Executor {
            return Err(rejected());
        }
        let raw = read(&self.state, KEY)?;
        let saved = decode(&raw)?;
        let slot = saved.slot.as_ref().ok_or_else(rejected)?;
        let proof = &verified.proof().permit;
        slot.validate_for(&proof.recipient_scope)
            .map_err(|_| rejected())?;
        if slot.public_ref != proof.recipient_key {
            return Err(rejected());
        }
        let genesis = initial(&proof.recipient_scope, &raw, 0)?;
        Ok((raw, genesis))
    }
    /// Explicit initialization is a separately counted fixed RPC in the caller
    /// graph. It never opens a key or upgrades the old signer automatically.
    pub(crate) fn initialize_execution_signer_v2(
        &self,
        verified: VerifiedExecutionPreparePermitV2,
    ) -> Result<()> {
        self.initialize_execution_signer_at_v2(verified, now()?)
    }
    fn initialize_execution_signer_at_v2(
        &self,
        verified: VerifiedExecutionPreparePermitV2,
        at: u64,
    ) -> Result<()> {
        let (slot, expected) = self.execution_v2_slot(&verified)?;
        let proof = &verified.proof().permit;
        if proof.operation != ExecutionPrepareSignerOperationV2::InitializeSigner {
            return Err(rejected());
        }
        if at < proof.issued_at.get() || at >= proof.expires_at.get() {
            return Err(rejected());
        }
        let genesis = initial(&proof.recipient_scope, &slot, at)?;
        let encoded = encode_v2(&genesis)?;
        let operation = verified.once_operation_digest().map_err(|_| rejected())?;
        let permit_digest = verified.proof_digest().map_err(|_| rejected())?;
        let mut callback = |old: String, rows: String| -> Result<Array> {
            let mut state = validate(&old, &rows, &expected)?;
            let inventory: InventoryV2 = serde_json::from_str(&rows).map_err(|_| rejected())?;
            if state.initialization_count.0 >= MAX_ONCE as u64
                || at < state.clock_floor.0
                || inventory
                    .initializations
                    .iter()
                    .any(|row| row.operation_id == operation)
            {
                return Err(rejected());
            }
            let record = InitializationV2 {
                sequence: DecimalU64(
                    state
                        .initialization_count
                        .0
                        .checked_add(1)
                        .ok_or_else(rejected)?,
                ),
                operation_digest: operation.clone(),
                permit_digest: permit_digest.clone(),
                previous: state.initialization_head.clone(),
                claimed_at: DecimalU64(at),
                issued_at: DecimalU64(proof.issued_at.get()),
                expires_at: DecimalU64(proof.expires_at.get()),
            };
            state.initialization_count = record.sequence;
            state.initialization_head =
                hash(&("oasis7.execution-signer.initialization.v2", &record))?;
            state.clock_floor = DecimalU64(at);
            let result = Array::new();
            result.push(&encode_v2(&state)?.into());
            result.push(&operation.clone().into());
            result.push(&encode_v2(&record)?.into());
            Ok(result)
        };
        let closure = Closure::borrow_mut(&mut callback);
        initialize_sql(
            &self.state,
            KEY,
            &slot,
            &encoded,
            closure.as_ref().unchecked_ref(),
        )?;
        drop(closure);
        Ok(())
    }
    pub(super) fn claim_execution_prepare_v2(
        &self,
        verified: VerifiedExecutionPreparePermitV2,
        at: u64,
    ) -> Result<ClaimedExecutionPrepareV2> {
        let (slot, genesis) = self.execution_v2_slot(&verified)?;
        let proof = &verified.proof().permit;
        if proof.operation != ExecutionPrepareSignerOperationV2::SignPrepare {
            return Err(rejected());
        }
        if at < proof.issued_at.get() || at >= proof.expires_at.get() {
            return Err(rejected());
        }
        let operation = verified.once_operation_digest().map_err(|_| rejected())?;
        let permit_digest = verified.proof_digest().map_err(|_| rejected())?;
        let bundle = verified.bundle();
        let oasis7_custody_core::provisioning::ExecutionSigningActionV2::Prepare {
            original,
            observation_nonce,
        } = bundle.action.as_ref()
        else {
            return Err(rejected());
        };
        let gateway = bundle
            .observation
            .observation_nonce
            .as_ref()
            .ok_or_else(rejected)?;
        let identifier_hash = |value: &str| {
            hash(&(
                "oasis7.execution-signer.identifier.v2",
                &genesis.scope_digest,
                value,
            ))
        };
        let stage_operation_hash = identifier_hash(&bundle.stage_operation_id)?;
        let nonce_hashes = [
            identifier_hash(&original.client_nonce)?,
            identifier_hash(observation_nonce)?,
            identifier_hash(gateway)?,
        ];
        let mut unique = std::collections::BTreeSet::new();
        for id in std::iter::once(&stage_operation_hash).chain(nonce_hashes.iter()) {
            if !unique.insert(id) {
                return Err(rejected());
            }
        }

        let mut callback = |old: String, rows: String| -> Result<Array> {
            let mut state = validate(&old, &rows, &genesis)?;
            let previous: InventoryV2 = serde_json::from_str(&rows).map_err(|_| rejected())?;
            for row in previous.sign {
                let old: ClaimV2 = serde_json::from_str(&row.payload).map_err(|_| rejected())?;
                for id in std::iter::once(&stage_operation_hash).chain(nonce_hashes.iter()) {
                    if id == &old.stage_operation_hash || old.nonce_hashes.contains(id) {
                        return Err(rejected());
                    }
                }
            }

            if state.initialization_count.0 == 0
                || state.count.0 >= MAX_ONCE as u64
                || at < state.clock_floor.0
            {
                return Err(rejected());
            }
            let record = ClaimV2 {
                api_version: 2,
                sequence: DecimalU64(state.count.0.checked_add(1).ok_or_else(rejected)?),
                operation_digest: operation.clone(),
                permit_digest: permit_digest.clone(),
                stage_operation_hash: stage_operation_hash.clone(),
                nonce_hashes: nonce_hashes.clone(),
                bundle_digest: proof.bundle_digest.clone(),
                previous: state.head.clone(),
                claimed_at: DecimalU64(at),
                issued_at: DecimalU64(proof.issued_at.get()),
                expires_at: DecimalU64(proof.expires_at.get()),
            };
            state.count = record.sequence;
            state.head = hash(&("oasis7.execution-signer.claim.v2", &record))?;
            state.clock_floor = DecimalU64(at);
            let result = Array::new();
            result.push(&encode_v2(&state)?.into());
            result.push(&operation.clone().into());
            result.push(&encode_v2(&record)?.into());
            Ok(result)
        };
        let closure = Closure::borrow_mut(&mut callback);
        claim_sql(&self.state, KEY, &slot, closure.as_ref().unchecked_ref())?;
        drop(closure);
        Ok(ClaimedExecutionPrepareV2 {
            verified,
            immutable_slot: slot,
        })
    }
}

#[path = "role_key_execution_original_v2.rs"]
mod original_v2;
