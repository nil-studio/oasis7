//! Guarded original-nonce updates in the shared ExecutionTransaction.
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_rollback_delta::prepare_rollback_record_delta_v1,
    incremental_rollback_family_genesis::field,
    incremental_rollback_family_recovery::RecoveredRollbackControlsV1,
    incremental_rollback_genesis::{
        RollbackRecordRefV1, RollbackWorkV1, prepare_rollback_record_v1,
    },
    incremental_rollback_qualification::{
        RollbackRecordQualificationV1, inherit_delta, qualify_rollback_record_v1, validate_original,
    },
    incremental_rollback_recovery::{RollbackDecodeContextV1, RollbackDecodeLimitsV1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    snapshot::RollbackNonceOutcome,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::cell::RefCell;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("rollback ordinary overlay")
}
pub struct RollbackOutcomeChangeV1<'a> {
    pub nonce: &'a str,
    pub parent: &'a RootSetV1,
    pub family: &'a ReservedValue<RecoveredRollbackControlsV1>,
    pub before: Option<&'a RollbackNonceOutcome>,
    pub after: Option<&'a RollbackNonceOutcome>,
    pub record: Option<&'a ReservedValue<RollbackRecordQualificationV1>>,
}
pub fn change_rollback_outcome_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: RollbackOutcomeChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RollbackWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<RollbackRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = RefCell::new(&mut context.cancelled);
    let write_cancel = RefCell::new(&mut write.cancelled);
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if tx.parent_fence().parent_roots != change.parent.digest().map_err(|_| invalid())? {
            return Err(invalid().into());
        }
        change.family.validate_binding(change.parent)?;
        if tx.parent_state_root(StateNamespaceV1::Authorization)
            != change.parent.state_root(StateNamespaceV1::Authorization)
            || !tx.uses_budget(write.memory)
            || !change.family.uses_budget(write.memory)
            || change.family.allocation_class() != MemoryClass::Ordinary
            || change.family.reserved_bytes() < std::mem::size_of::<RecoveredRollbackControlsV1>()
            || !context.values.memory.same_pool(write.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let _charge = write
            .memory
            .reserve(32768, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut combined = || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())();
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut combined,
        };
        let descriptor = field("rollback_nonce_outcomes");
        let meta = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let metadata =
            lookup(tx, &meta, &mut records, &mut context.take_state_error)?.ok_or_else(invalid)?;
        records.presence(&metadata, &meta, 0)?;
        let marker = records.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
        let marker_id = lookup(tx, &marker.0, &mut records, &mut context.take_state_error)?
            .ok_or_else(invalid)?;
        records.fixed::<0>(&marker_id, &marker.0, marker.1, MemberPayloadCodecV1::Unit)?;
        let key = records.key(descriptor, WorldFieldRecordKindV1::Entry, change.nonce)?;
        let original = lookup(tx, &key.0, &mut records, &mut context.take_state_error)?;
        if original.is_some() != change.before.is_some()
            || change.before.is_some() != change.record.is_some()
        {
            return Err(invalid().into());
        }
        let child = if let Some(original) = original {
            let bytes = records.fixed::<RECORD_ROOT_BYTES>(
                &original,
                &key.0,
                key.1,
                MemberPayloadCodecV1::RecordRoot,
            )?;
            let parsed =
                WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
            let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
            Some(
                reference
                    .validate_binding(
                        blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
                        &parsed.semantic_key.ok_or_else(invalid)?,
                        StateNamespaceV1::Authorization,
                    )
                    .map_err(|_| invalid())?
                    .clone(),
            )
        } else {
            None
        };
        let mut read = RollbackDecodeContextV1 {
            values: &mut context.values,
            keys: context.key_work,
            work,
            limits: RollbackDecodeLimitsV1 {
                max_output_bytes: context.record_bytes.try_into().map_err(|_| invalid())?,
                max_text_bytes: context.record_bytes,
                max_record_bytes: context.record_bytes,
            },
            cancelled: &mut combined,
        };
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())(),
        };
        let next = match (change.before, change.after, child.as_ref(), change.record) {
            (None, None, None, None) => {
                if (read.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                return Ok(None);
            }
            (None, Some(after), None, None) => {
                let root = prepare_rollback_record_v1(
                    RollbackRecordRefV1::Outcome(after),
                    &mut output,
                    read.work,
                )?;
                let qualification = qualify_rollback_record_v1(
                    &root,
                    RollbackRecordRefV1::Outcome(after),
                    &mut read,
                )?;
                Some((root, qualification))
            }
            (Some(before), Some(after), Some(child), Some(record)) => {
                let root = prepare_rollback_record_delta_v1(
                    child,
                    record,
                    RollbackRecordRefV1::Outcome(before),
                    RollbackRecordRefV1::Outcome(after),
                    &mut read,
                    &mut output,
                )?;
                if *root == *child {
                    return Ok(None);
                }
                let qualification = inherit_delta(
                    (*root).clone(),
                    RollbackRecordRefV1::Outcome(after),
                    &mut read,
                )?;
                Some(((*root).clone(), qualification))
            }
            (Some(before), None, Some(child), Some(record)) => {
                if !record.uses_budget(write.memory)
                    || record.allocation_class() != MemoryClass::Ordinary
                    || record.reserved_bytes()
                        < std::mem::size_of::<RollbackRecordQualificationV1>()
                {
                    return Err(invalid().into());
                }
                record.validate_binding(child, &RollbackRecordRefV1::Outcome(before).schema())?;
                validate_original(record, RollbackRecordRefV1::Outcome(before), &mut read)?;
                None
            }
            _ => return Err(invalid().into()),
        };
        let result = if let Some((root, qualification)) = next {
            let parsed =
                WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
            let reference = RecordRootRefV1::new(
                *blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
                parsed.semantic_key.ok_or_else(invalid)?,
                root,
            )
            .map_err(|_| invalid())?;
            let row = write_member_record_v1(
                descriptor,
                WorldFieldRecordKindV1::Entry,
                change.nonce,
                MemberPayloadRefV1::RecordRoot(&reference),
                &mut output,
            )?;
            if row.field_key != key.0 {
                return Err(invalid().into());
            }
            tx.write(
                StateNamespaceV1::Authorization,
                key.0.as_bytes(),
                Some(row.value_root),
            )?;
            Some(qualification)
        } else {
            tx.write(StateNamespaceV1::Authorization, key.0.as_bytes(), None)?;
            None
        };
        if (read.cancelled)() || (output.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(result)
    })
}

pub struct RollbackNonceChangeV1<'a> {
    pub nonce: &'a str,
    pub parent: &'a RootSetV1,
    pub family: &'a ReservedValue<RecoveredRollbackControlsV1>,
    pub expected: bool,
    pub after: bool,
}
pub fn change_consumed_rollback_nonce_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: RollbackNonceChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if tx.parent_fence().parent_roots != change.parent.digest().map_err(|_| invalid())? {
            return Err(invalid().into());
        }
        change.family.validate_binding(change.parent)?;
        if tx.parent_state_root(StateNamespaceV1::Authorization)
            != change.parent.state_root(StateNamespaceV1::Authorization)
            || !tx.uses_budget(write.memory)
            || !change.family.uses_budget(write.memory)
            || change.family.allocation_class() != MemoryClass::Ordinary
            || change.family.reserved_bytes() < std::mem::size_of::<RecoveredRollbackControlsV1>()
            || !context.values.memory.same_pool(write.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let _charge = write
            .memory
            .reserve(16384, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let descriptor = field("consumed_rollback_nonces");
        let (key, present) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let meta = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
            let id = lookup(tx, &meta, &mut records, &mut context.take_state_error)?
                .ok_or_else(invalid)?;
            records.presence(&id, &meta, 0)?;
            let marker = records.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
            let id = lookup(tx, &marker.0, &mut records, &mut context.take_state_error)?
                .ok_or_else(invalid)?;
            records.fixed::<0>(&id, &marker.0, marker.1, MemberPayloadCodecV1::Unit)?;
            let key = records.key(descriptor, WorldFieldRecordKindV1::Entry, change.nonce)?;
            let id = lookup(tx, &key.0, &mut records, &mut context.take_state_error)?;
            if let Some(id) = id
                && records.fixed::<1>(&id, &key.0, key.1, MemberPayloadCodecV1::Bool)? != [1]
            {
                return Err(invalid().into());
            }
            (key, id.is_some())
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if present != change.expected {
            return Err(invalid().into());
        }
        if change.expected == change.after {
            return Ok(());
        }
        let after = if change.after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let row = write_member_record_v1(
                descriptor,
                WorldFieldRecordKindV1::Entry,
                change.nonce,
                MemberPayloadRefV1::Bool(true),
                &mut combined,
            )?;
            if row.field_key != key.0 {
                return Err(invalid().into());
            }
            Some(row.value_root)
        } else {
            None
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(StateNamespaceV1::Authorization, key.0.as_bytes(), after)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
