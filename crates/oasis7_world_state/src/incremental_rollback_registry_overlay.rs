//! Guarded complete registry replacement with unchanged authority subtrees retained.
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self, RecordTreeWritePolicyV1},
    incremental_rollback_delta::prepare_rollback_record_delta_v1,
    incremental_rollback_family_genesis::field,
    incremental_rollback_family_recovery::RecoveredRollbackControlsV1,
    incremental_rollback_genesis::{
        LABELS, RollbackRecordRefV1, RollbackWorkV1, prepare_rollback_record_v1,
    },
    incremental_rollback_qualification::qualify_rollback_record_v1,
    incremental_rollback_recovery::{RollbackDecodeContextV1, RollbackDecodeLimitsV1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    snapshot::RollbackAuthorityRegistry,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{cell::RefCell, collections::BTreeSet};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("rollback registry overlay")
}
pub struct RollbackRegistryQualificationV1 {
    root: StateRootV1,
    registry: RollbackAuthorityRegistry,
    class: MemoryClass,
}
pub enum RollbackRegistryBeforeV1<'a> {
    Parent(&'a ReservedValue<RecoveredRollbackControlsV1>),
    Overlay(&'a ReservedValue<RollbackRegistryQualificationV1>),
}
pub struct RollbackRegistryChangeV1<'a> {
    pub parent: &'a RootSetV1,
    pub before: RollbackRegistryBeforeV1<'a>,
    pub after: &'a RollbackAuthorityRegistry,
}
pub fn change_rollback_registry_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: RollbackRegistryChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RollbackWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<RollbackRegistryQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = RefCell::new(&mut context.cancelled);
    let write_cancel = RefCell::new(&mut write.cancelled);
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if tx.parent_fence().parent_roots != change.parent.digest().map_err(|_| invalid())? {
            return Err(invalid().into());
        }
        if tx.parent_state_root(StateNamespaceV1::Authorization)
            != change.parent.state_root(StateNamespaceV1::Authorization)
            || !tx.uses_budget(write.memory)
            || !context.values.memory.same_pool(write.memory)
            || context.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let (before_root, before) = match change.before {
            RollbackRegistryBeforeV1::Parent(family) => {
                if !family.uses_budget(write.memory)
                    || family.allocation_class() != MemoryClass::Ordinary
                    || family.reserved_bytes() < std::mem::size_of::<RecoveredRollbackControlsV1>()
                {
                    return Err(invalid().into());
                }
                family.validate_binding(change.parent)?;
                (family.registry_root(), &family.authority_registry)
            }
            RollbackRegistryBeforeV1::Overlay(qualification) => {
                if !qualification.uses_budget(write.memory)
                    || qualification.class != MemoryClass::Ordinary
                    || qualification.reserved_bytes()
                        < std::mem::size_of::<RollbackRegistryQualificationV1>()
                {
                    return Err(invalid().into());
                }
                (&qualification.root, &qualification.registry)
            }
        };
        let total_bytes = before
            .records()
            .chain(change.after.records())
            .try_fold(65536usize, |sum, record| {
                sum.checked_add(
                    record
                        .authority_id
                        .len()
                        .checked_add(record.public_key_hex.len())?
                        .checked_mul(8)?,
                )
            })
            .ok_or_else(invalid)?;
        let operation = write
            .memory
            .reserve(total_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        change.after.validate().map_err(|_| invalid())?;
        let mut combined = || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())();
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut combined,
        };
        let descriptor = field("rollback_authority_registry");
        let meta = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let id =
            lookup(tx, &meta, &mut records, &mut context.take_state_error)?.ok_or_else(invalid)?;
        records.presence(&id, &meta, 0)?;
        let outer = records.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
        let original = lookup(tx, &outer.0, &mut records, &mut context.take_state_error)?
            .ok_or_else(invalid)?;
        let bytes = records.fixed::<RECORD_ROOT_BYTES>(
            &original,
            &outer.0,
            outer.1,
            MemberPayloadCodecV1::RecordRoot,
        )?;
        let semantic = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
        let actual = reference
            .validate_binding(
                &*blake3::hash(b"runtime.rollback.registry.v1").as_bytes(),
                &semantic,
                StateNamespaceV1::Authorization,
            )
            .map_err(|_| invalid())?;
        if actual != before_root {
            return Err(invalid().into());
        }
        let unchanged = before == change.after;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(None);
        }
        let mut root = actual.clone();
        let ids: BTreeSet<_> = before
            .records()
            .chain(change.after.records())
            .map(|record| record.authority_id.as_str())
            .collect();
        for id in ids {
            let old = before.get(id);
            let new = change.after.get(id);
            let equal = old == new;
            if combined() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if equal {
                continue;
            }
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let key = records.key(descriptor, WorldFieldRecordKindV1::Entry, id)?;
            let previous = incremental_record_tree::lookup(&root, &key.0, &mut records, LABELS)?;
            if previous.is_some() != old.is_some() {
                return Err(invalid().into());
            }
            let child = if let Some(previous) = previous {
                let bytes = records.fixed::<RECORD_ROOT_BYTES>(
                    &previous,
                    &key.0,
                    key.1,
                    MemberPayloadCodecV1::RecordRoot,
                )?;
                let key = WorldFieldKeyV1::from_bytes(key.0.as_bytes())
                    .map_err(ValueTreeError::Invalid)?;
                let reference =
                    RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
                Some(
                    reference
                        .validate_binding(
                            &*blake3::hash(b"runtime.rollback.authority.v1").as_bytes(),
                            &key.semantic_key.ok_or_else(invalid)?,
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
            let next = match (old, new, child.as_ref()) {
                (None, Some(after), None) => Some(prepare_rollback_record_v1(
                    RollbackRecordRefV1::Authority(after),
                    &mut output,
                    read.work,
                )?),
                (Some(before), Some(after), Some(child)) => {
                    let token = qualify_rollback_record_v1(
                        child,
                        RollbackRecordRefV1::Authority(before),
                        &mut read,
                    )?;
                    Some(
                        (*prepare_rollback_record_delta_v1(
                            child,
                            &token,
                            RollbackRecordRefV1::Authority(before),
                            RollbackRecordRefV1::Authority(after),
                            &mut read,
                            &mut output,
                        )?)
                        .clone(),
                    )
                }
                (Some(before), None, Some(child)) => {
                    qualify_rollback_record_v1(
                        child,
                        RollbackRecordRefV1::Authority(before),
                        &mut read,
                    )?;
                    None
                }
                _ => return Err(invalid().into()),
            };
            let after = if let Some(child) = next {
                let parsed = WorldFieldKeyV1::from_bytes(key.0.as_bytes())
                    .map_err(ValueTreeError::Invalid)?;
                let reference = RecordRootRefV1::new(
                    *blake3::hash(b"runtime.rollback.authority.v1").as_bytes(),
                    parsed.semantic_key.ok_or_else(invalid)?,
                    child,
                )
                .map_err(|_| invalid())?;
                let row = write_member_record_v1(
                    descriptor,
                    WorldFieldRecordKindV1::Entry,
                    id,
                    MemberPayloadRefV1::RecordRoot(&reference),
                    &mut output,
                )?;
                if row.field_key != key.0 {
                    return Err(invalid().into());
                }
                Some(row.value_root)
            } else {
                None
            };
            root = incremental_record_tree::edit(
                root,
                &key.0,
                (previous, after),
                read.values,
                &mut output,
                read.cancelled,
                RecordTreeWritePolicyV1 {
                    max_pending_node_bytes: read.work.max_pending_node_bytes,
                    labels: LABELS,
                },
            )?;
        }
        let reference = RecordRootRefV1::new(
            *blake3::hash(b"runtime.rollback.registry.v1").as_bytes(),
            semantic,
            root.clone(),
        )
        .map_err(|_| invalid())?;
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || (*cancel.borrow_mut())() || (*write_cancel.borrow_mut())(),
        };
        let row = write_member_record_v1(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut output,
        )?;
        if row.field_key != outer.0 {
            return Err(invalid().into());
        }
        if (output.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            outer.0.as_bytes(),
            Some(row.value_root),
        )?;
        let token = operation.retain(RollbackRegistryQualificationV1 {
            root,
            registry: change.after.clone(),
            class: MemoryClass::Ordinary,
        });
        if (output.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(token))
    })
}
