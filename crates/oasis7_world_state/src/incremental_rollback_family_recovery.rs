//! Fixed-root, budgeted cold rollback collection reconstruction.
use crate::{
    incremental_field_member::*,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::FieldOwner,
    incremental_rollback_family_genesis::field,
    incremental_rollback_recovery::*,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    snapshot::*,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget, StateScanContext,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
};
pub struct RollbackControlsValuesV1 {
    pub authority_registry: RollbackAuthorityRegistry,
    pub consumed_nonces: BTreeSet<String>,
    pub nonce_outcomes: BTreeMap<String, ReservedValue<RollbackNonceOutcome>>,
}
enum RollbackControlsDataV1 {
    Owned(RollbackControlsValuesV1),
    Readmitted(std::sync::Arc<ReservedValue<RecoveredRollbackControlsV1>>),
}
pub struct RecoveredRollbackControlsV1 {
    data: RollbackControlsDataV1,
    root_set_digest: [u8; 32],
    registry_root: StateRootV1,
    class: MemoryClass,
}
impl std::ops::Deref for RecoveredRollbackControlsV1 {
    type Target = RollbackControlsValuesV1;
    fn deref(&self) -> &Self::Target {
        self.values()
    }
}
impl RecoveredRollbackControlsV1 {
    fn values(&self) -> &RollbackControlsValuesV1 {
        match &self.data {
            RollbackControlsDataV1::Owned(values) => values,
            RollbackControlsDataV1::Readmitted(original) => original.values(),
        }
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }

    pub(crate) fn registry_root(&self) -> &StateRootV1 {
        &self.registry_root
    }

    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if roots.digest().map_err(|_| invalid())? == self.root_set_digest {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("rollback control collection")
}
enum Key {
    Unit,
    Text(String),
}
struct Row {
    key: Key,
    reference: Option<RecordRootRefV1>,
    codec: MemberPayloadCodecV1,
    _reservation: oasis7_proto::memory_budget::MemoryReservation,
}
fn row<R: ValueNodeReader, C: FnMut() -> bool>(
    entry: &oasis7_formal_protocol::incremental_state::StateEntryV1,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
    max_key_bytes: usize,
) -> Result<Row, ValueTreeError> {
    let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let owned_key = WorldFieldKeyV1::new(
        field(match key.name {
            "rollback_authority_registry" => "rollback_authority_registry",
            "consumed_rollback_nonces" => "consumed_rollback_nonces",
            "rollback_nonce_outcomes" => "rollback_nonce_outcomes",
            _ => return Err(invalid()),
        }),
        key.kind,
        key.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if owned_key.as_bytes() != entry.key || entry.namespace != StateNamespaceV1::Authorization {
        return Err(invalid());
    }
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: ctx.limits.max_record_bytes,
        payload_bytes: RECORD_ROOT_BYTES as u64,
        record_bytes: ctx.limits.max_record_bytes,
    };
    let info = read_member_record_header_v1(
        &entry.value,
        &owned_key,
        ctx.values,
        limits,
        &mut *ctx.cancelled,
    )?;
    let length = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    let key_bound = max_key_bytes
        .checked_add(crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN.len() + 9)
        .ok_or_else(invalid)?;
    if length > key_bound {
        return Err(ValueTreeError::ResourceLimited(
            "rollback recovered key bytes",
        ));
    }
    let _scratch = ctx.values.memory.reserve(
        length
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(8192))
            .ok_or_else(invalid)?,
        ctx.values.class,
    )?;
    let mut semantic = vec![0; length];
    let mut payload = [0; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &entry.value,
        &owned_key,
        ctx.values,
        MemberReadProfileV1 {
            codec: info.codec,
            semantic_bytes: info.semantic_bytes,
            limits,
        },
        &mut Cursor::new(semantic.as_mut_slice()),
        &mut Cursor::new(payload.as_mut_slice()),
        &mut *ctx.cancelled,
    )?;
    let original = match read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Unit => Key::Unit,
        SemanticKeyRefV1::Text(value) => Key::Text(value.to_owned()),
        _ => return Err(invalid()),
    };
    let reference = match info.codec {
        MemberPayloadCodecV1::Unit if info.payload_bytes == 0 => None,
        MemberPayloadCodecV1::Bool if info.payload_bytes == 1 && payload[0] == 1 => None,
        MemberPayloadCodecV1::RecordRoot if info.payload_bytes == RECORD_ROOT_BYTES as u64 => {
            Some(RecordRootRefV1::from_canonical_bytes(&payload).map_err(|_| invalid())?)
        }
        _ => return Err(invalid()),
    };
    Ok(Row {
        key: original,
        reference,
        codec: info.codec,
        _reservation: _scratch,
    })
}
/// Full fixed-root scan belongs to cold recovery only. Ordinary commits use
/// exact owner changes, never this scan. No partial collection escapes failure.
pub fn recover_rollback_controls_v1<M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool>(
    roots: &RootSetV1,
    state: &M,
    scan: &mut StateScanBudget,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<ReservedValue<RecoveredRollbackControlsV1>, ValueTreeError> {
    let memory = ctx.values.memory;
    let class = ctx.values.class;
    if state.allocation_class() != Some(class)
        || !state
            .allocation_budget()
            .is_some_and(|pool| pool.same_pool(memory))
    {
        return Err(invalid());
    }
    let charge = memory.reserve(ctx.limits.max_output_bytes, class)?;
    let mut remaining = ctx
        .limits
        .max_output_bytes
        .checked_sub(std::mem::size_of::<RecoveredRollbackControlsV1>() + 256)
        .ok_or(ValueTreeError::ResourceLimited(
            "rollback collection header",
        ))?;
    let root_digest = roots.digest().map_err(|_| invalid())?;
    let mut consumed = BTreeSet::new();
    let mut outcomes = BTreeMap::new();
    let mut registry: Option<StateRootV1> = None;
    let mut metadata = [false; 3];
    let mut markers = [false; 3];
    let cancelled = RefCell::new(&mut *ctx.cancelled);
    let mut scan_context = StateScanContext {
        source: state,
        memory,
        class,
        io: scan,
    };
    scan_context
        .visit_entries(
            roots.state_root(StateNamespaceV1::Authorization),
            |entry| -> Result<(), ValueTreeError> {
                // This namespace also contains independently registered raw
                // Agent keys (including absent IntentPresent). This owner
                // proves only the three rollback fields; the complete router
                // remains responsible for every other domain. Keep malformed
                // generic v1 keys strict, including a damaged NUL separator.
                if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1") {
                    return Ok(());
                }
                let key =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if key.owner != FieldOwner::Runtime {
                    return Ok(());
                }
                let index = match key.name {
                    "rollback_authority_registry" => 0,
                    "consumed_rollback_nonces" => 1,
                    "rollback_nonce_outcomes" => 2,
                    _ => return Ok(()),
                };
                let mut cancellation = || (*cancelled.borrow_mut())();
                let mut local = RollbackDecodeContextV1 {
                    values: ctx.values,
                    keys: ctx.keys,
                    work: ctx.work,
                    limits: RollbackDecodeLimitsV1 {
                        max_output_bytes: ctx.limits.max_output_bytes,
                        max_text_bytes: ctx.limits.max_text_bytes,
                        max_record_bytes: ctx.limits.max_record_bytes,
                    },
                    cancelled: &mut cancellation,
                };
                if key.kind == WorldFieldRecordKindV1::Metadata {
                    if metadata[index] {
                        return Err(invalid());
                    }
                    let expected = WorldFieldKeyV1::new(
                        field(match index {
                            0 => "rollback_authority_registry",
                            1 => "consumed_rollback_nonces",
                            _ => "rollback_nonce_outcomes",
                        }),
                        WorldFieldRecordKindV1::Metadata,
                        None,
                    )
                    .map_err(ValueTreeError::Invalid)?;
                    if expected.as_bytes() != entry.key {
                        return Err(invalid());
                    }
                    let mut reader = InventoryValueReader {
                        values: local.values,
                        key_work: local.keys,
                        semantic_bytes: local.limits.max_record_bytes,
                        record_bytes: local.limits.max_record_bytes,
                        cancelled: local.cancelled,
                    };
                    reader.presence(&entry.value, &expected, 0)?;
                    metadata[index] = true;
                    return Ok(());
                }
                let original = row(entry, &mut local, remaining.saturating_sub(256))?;
                match original.key {
                    Key::Unit => {
                        if markers[index] {
                            return Err(invalid());
                        }
                        markers[index] = true;
                        if index == 0 {
                            let reference = original.reference.ok_or_else(invalid)?;
                            registry = Some(
                                reference
                                    .validate_binding(
                                        blake3::hash(b"runtime.rollback.registry.v1").as_bytes(),
                                        &key.semantic_key.ok_or_else(invalid)?,
                                        StateNamespaceV1::Authorization,
                                    )
                                    .map_err(|_| invalid())?
                                    .clone(),
                            );
                        } else if original.reference.is_some()
                            || original.codec != MemberPayloadCodecV1::Unit
                        {
                            return Err(invalid());
                        }
                    }
                    Key::Text(nonce) => {
                        remaining = remaining
                            .checked_sub(nonce.capacity().checked_add(256).ok_or_else(invalid)?)
                            .ok_or(ValueTreeError::ResourceLimited(
                                "rollback recovered collection",
                            ))?;
                        match index {
                            1 => {
                                if original.reference.is_some()
                                    || original.codec != MemberPayloadCodecV1::Bool
                                    || !consumed.insert(nonce)
                                {
                                    return Err(invalid());
                                }
                            }
                            2 => {
                                let reference = original.reference.ok_or_else(invalid)?;
                                let child = reference
                                    .validate_binding(
                                        blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
                                        &key.semantic_key.ok_or_else(invalid)?,
                                        StateNamespaceV1::Authorization,
                                    )
                                    .map_err(|_| invalid())?;
                                let outcome = recover_rollback_outcome_v1(child, &mut local)?;
                                if outcomes.insert(nonce, outcome).is_some() {
                                    return Err(invalid());
                                }
                            }
                            _ => return Err(invalid()),
                        }
                    }
                }
                Ok(())
            },
            || (*cancelled.borrow_mut())(),
        )
        .map_err(scan_error)?;
    if metadata != [true; 3] || markers != [true; 3] {
        return Err(invalid());
    }
    let registry = registry.ok_or_else(invalid)?;
    let mut authorities = Vec::with_capacity(2);
    remaining = remaining
        .checked_sub(2 * std::mem::size_of::<RollbackAuthorityRecord>())
        .ok_or(ValueTreeError::ResourceLimited("rollback registry output"))?;
    let mut inner_metadata = false;
    let mut inner_marker = false;
    scan_context
        .visit_entries(
            &registry,
            |entry| -> Result<(), ValueTreeError> {
                let key =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if key.owner != FieldOwner::Runtime
                    || key.name != "rollback_authority_registry"
                    || key.namespace != StateNamespaceV1::Authorization
                {
                    return Err(invalid());
                }
                let mut cancellation = || (*cancelled.borrow_mut())();
                let mut local = RollbackDecodeContextV1 {
                    values: ctx.values,
                    keys: ctx.keys,
                    work: ctx.work,
                    limits: RollbackDecodeLimitsV1 {
                        max_output_bytes: ctx.limits.max_output_bytes,
                        max_text_bytes: ctx.limits.max_text_bytes,
                        max_record_bytes: ctx.limits.max_record_bytes,
                    },
                    cancelled: &mut cancellation,
                };
                if key.kind == WorldFieldRecordKindV1::Metadata {
                    let expected = WorldFieldKeyV1::new(
                        field("rollback_authority_registry"),
                        WorldFieldRecordKindV1::Metadata,
                        None,
                    )
                    .map_err(ValueTreeError::Invalid)?;
                    if inner_metadata || expected.as_bytes() != entry.key {
                        return Err(invalid());
                    }
                    let mut reader = InventoryValueReader {
                        values: local.values,
                        key_work: local.keys,
                        semantic_bytes: local.limits.max_record_bytes,
                        record_bytes: local.limits.max_record_bytes,
                        cancelled: local.cancelled,
                    };
                    reader.presence(&entry.value, &expected, 0)?;
                    inner_metadata = true;
                    return Ok(());
                }
                let original = row(entry, &mut local, remaining.saturating_sub(512) / 2)?;
                match original.key {
                    Key::Unit => {
                        if inner_marker
                            || original.reference.is_some()
                            || original.codec != MemberPayloadCodecV1::Unit
                        {
                            return Err(invalid());
                        }
                        inner_marker = true;
                    }
                    Key::Text(id) => {
                        if authorities.len() >= 2 {
                            return Err(invalid());
                        }
                        let reference = original.reference.ok_or_else(invalid)?;
                        let child = reference
                            .validate_binding(
                                blake3::hash(b"runtime.rollback.authority.v1").as_bytes(),
                                &key.semantic_key.ok_or_else(invalid)?,
                                StateNamespaceV1::Authorization,
                            )
                            .map_err(|_| invalid())?;
                        let authority = recover_rollback_authority_v1(child, &mut local)?;
                        if id != authority.authority_id {
                            return Err(invalid());
                        }
                        remaining = remaining
                            .checked_sub(
                                authority
                                    .authority_id
                                    .len()
                                    .checked_mul(2)
                                    .and_then(|bytes| {
                                        bytes.checked_add(authority.public_key_hex.len())
                                    })
                                    .and_then(|bytes| bytes.checked_add(512))
                                    .ok_or_else(invalid)?,
                            )
                            .ok_or(ValueTreeError::ResourceLimited("rollback registry strings"))?;
                        authorities.push((*authority).clone());
                    }
                }
                Ok(())
            },
            || (*cancelled.borrow_mut())(),
        )
        .map_err(scan_error)?;
    if !inner_metadata || !inner_marker {
        return Err(invalid());
    }
    let temporary_bytes = authorities
        .iter()
        .try_fold(65536usize, |sum, record| {
            sum.checked_add(
                record
                    .authority_id
                    .len()
                    .checked_add(record.public_key_hex.len())?
                    .checked_mul(4)?,
            )
        })
        .ok_or_else(invalid)?;
    let _registry_validation = memory.reserve(temporary_bytes, class)?;
    let authority_registry = if authorities.is_empty() {
        RollbackAuthorityRegistry::default()
    } else {
        RollbackAuthorityRegistry::new(authorities.iter().cloned()).map_err(|_| invalid())?
    };
    for record in &authorities {
        if authority_registry.get(&record.authority_id) != Some(record) {
            return Err(invalid());
        }
    }
    if (*cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    let _ = remaining;
    Ok(charge.retain(RecoveredRollbackControlsV1 {
        data: RollbackControlsDataV1::Owned(RollbackControlsValuesV1 {
            authority_registry,
            consumed_nonces: consumed,
            nonce_outcomes: outcomes,
        }),
        root_set_digest: root_digest,
        registry_root: registry,
        class,
    }))
}

fn scan_error(
    error: oasis7_formal_protocol::incremental_state::StateScanError<ValueTreeError>,
) -> ValueTreeError {
    use oasis7_formal_protocol::incremental_state::StateScanError;
    match error {
        StateScanError::Visitor(e) => e,
        StateScanError::Memory(e) => ValueTreeError::Memory(e),
        StateScanError::ResourceLimited => {
            ValueTreeError::ResourceLimited("rollback recovery scan")
        }
        StateScanError::Cancelled => ValueTreeError::Cancelled,
        _ => invalid(),
    }
}

#[path = "incremental_rollback_readmission.rs"]
mod readmission;
pub use readmission::readmit_rollback_controls_ordinary_v1;
