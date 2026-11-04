//! Complete original-wire rollback qualification. Outcome DTOs never escape or materialize.
use crate::{
    incremental_field_member::*,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::FieldOwner,
    incremental_record_tree,
    incremental_rollback_family_genesis::field,
    incremental_rollback_genesis::{LABELS, descriptor},
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
use std::{cell::RefCell, io::Cursor};
pub struct RollbackFamilyQualificationV1 {
    root_set_digest: [u8; 32],
    registry_root: StateRootV1,
    class: MemoryClass,
    consumed_count: u64,
    outcome_count: u64,
}
impl RollbackFamilyQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn consumed_nonce_count(&self) -> u64 {
        self.consumed_count
    }
    pub fn outcome_count(&self) -> u64 {
        self.outcome_count
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
    ValueTreeError::Invalid("rollback complete family")
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
    if key.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
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
    ctx.keys.bytes_remaining = ctx
        .keys
        .bytes_remaining
        .checked_sub(info.semantic_bytes)
        .ok_or(ValueTreeError::ResourceLimited(
            "rollback original key bytes",
        ))?;
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
fn markers<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    let mut reader = InventoryValueReader {
        values: ctx.values,
        key_work: ctx.keys,
        semantic_bytes: ctx.limits.max_record_bytes,
        record_bytes: ctx.limits.max_record_bytes,
        cancelled: &mut *ctx.cancelled,
    };
    let key = WorldFieldKeyV1::new(descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id =
        incremental_record_tree::lookup(root, &key, &mut reader, LABELS)?.ok_or_else(invalid)?;
    reader.presence(&id, &key, 0)?;
    let key = reader.key(descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    let id =
        incremental_record_tree::lookup(root, &key.0, &mut reader, LABELS)?.ok_or_else(invalid)?;
    reader.fixed::<0>(&id, &key.0, key.1, MemberPayloadCodecV1::Unit)?;
    Ok(())
}
fn qualify_inner<M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool>(
    roots: &RootSetV1,
    state: &M,
    scan: &mut StateScanBudget,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<ReservedValue<RollbackFamilyQualificationV1>, ValueTreeError> {
    let memory = ctx.values.memory;
    let class = ctx.values.class;
    if state.allocation_class() != Some(class)
        || !state
            .allocation_budget()
            .is_some_and(|pool| pool.same_pool(memory))
    {
        return Err(invalid());
    }
    let mut charge = memory.reserve(ctx.limits.max_output_bytes.max(512), class)?;
    let remaining = ctx
        .limits
        .max_output_bytes
        .checked_sub(std::mem::size_of::<RollbackFamilyQualificationV1>() + 256)
        .ok_or(ValueTreeError::ResourceLimited(
            "rollback collection header",
        ))?;
    let root_digest = roots.digest().map_err(|_| invalid())?;
    let mut consumed_count = 0u64;
    let mut outcome_count = 0u64;
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
                ctx.work.cells_remaining = ctx
                    .work
                    .cells_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("rollback family rows"))?;
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
                    Key::Text(_nonce) => match index {
                        1 => {
                            consumed_count = consumed_count.checked_add(1).ok_or_else(invalid)?;
                            if original.reference.is_some()
                                || original.codec != MemberPayloadCodecV1::Bool
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
                            stream::qualify_outcome(child, &mut local)?;
                            outcome_count = outcome_count.checked_add(1).ok_or_else(invalid)?;
                        }
                        _ => return Err(invalid()),
                    },
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
    validate_registry(&registry, state, scan, ctx)?;
    let _ = remaining;
    Ok(charge
        .split(512)
        .map_err(ValueTreeError::Invalid)?
        .retain(RollbackFamilyQualificationV1 {
            root_set_digest: root_digest,
            registry_root: registry,
            class,
            consumed_count,
            outcome_count,
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

#[path = "incremental_rollback_family_stream.rs"]
mod stream;

/// Re-admit only an authenticated Recovery proof for these exact roots and pool.
pub fn readmit_rollback_family_ordinary_v1(
    source: &ReservedValue<RollbackFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RollbackFamilyQualificationV1>, ValueTreeError> {
    let result = (|| {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if source.class != MemoryClass::Recovery
            || !source.uses_budget(memory)
            || source.reserved_bytes() < 512
        {
            return Err(invalid());
        }
        source.validate_binding(roots)?;
        let mut admission = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
        Ok(admission
            .split(512)
            .map_err(ValueTreeError::Invalid)?
            .retain(RollbackFamilyQualificationV1 {
                root_set_digest: source.root_set_digest,
                registry_root: source.registry_root.clone(),
                class: MemoryClass::Ordinary,
                consumed_count: source.consumed_count,
                outcome_count: source.outcome_count,
            }))
    })();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    result
}

fn validate_registry<M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool>(
    registry: &StateRootV1,
    state: &M,
    scan: &mut StateScanBudget,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    let memory = ctx.values.memory;
    let class = ctx.values.class;
    let _charge = memory.reserve(ctx.limits.max_output_bytes, class)?;
    let mut remaining = ctx.limits.max_output_bytes;
    let cancelled = RefCell::new(&mut *ctx.cancelled);
    let mut scan_context = StateScanContext {
        source: state,
        memory,
        class,
        io: scan,
    };
    let mut authorities = Vec::with_capacity(2);
    remaining = remaining
        .checked_sub(2 * std::mem::size_of::<RollbackAuthorityRecord>())
        .ok_or(ValueTreeError::ResourceLimited("rollback registry output"))?;
    let mut inner_metadata = false;
    let mut inner_marker = false;
    scan_context
        .visit_entries(
            registry,
            |entry| -> Result<(), ValueTreeError> {
                let key =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if key.owner != FieldOwner::Runtime
                    || key.name != "rollback_authority_registry"
                    || key.namespace != StateNamespaceV1::Authorization
                {
                    return Err(invalid());
                }
                ctx.work.cells_remaining = ctx
                    .work
                    .cells_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("rollback family rows"))?;
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
    Ok(())
}

#[path = "incremental_rollback_family_receiver.rs"]
mod receiver;
pub use receiver::accept_rollback_family_transition_v1;

/// Independently qualify all three rollback fields from persisted roots. This
/// establishes only rollback coverage, never global World completeness.
pub fn qualify_rollback_family_v1<M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool>(
    roots: &RootSetV1,
    state: &M,
    scan: &mut StateScanBudget,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
    mut take_state_error: impl FnMut() -> Option<ValueTreeError>,
) -> Result<ReservedValue<RollbackFamilyQualificationV1>, ValueTreeError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = take_state_error() {
        return Err(e);
    }
    let result = (|| {
        roots.validate().map_err(|_| invalid())?;
        qualify_inner(roots, state, scan, ctx)
    })();
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = take_state_error() {
        return Err(e);
    }
    result
}

#[cfg(test)]
#[path = "incremental_rollback_family_tests.rs"]
mod tests;

#[path = "incremental_rollback_family_vector_delta.rs"]
mod vector_delta;

pub(crate) use receiver::accept_rollback_family_verified_v1;
