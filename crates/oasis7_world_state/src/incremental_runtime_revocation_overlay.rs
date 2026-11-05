//! Provisional exact replacement of an already qualified revocation body.
use super::*;
use crate::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup,
};
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_proto::memory_budget::ReservedValue;

/// Authenticate the unchanged current body before using its qualified empty
/// safe-alternative count. A changed body needs the full current dispatcher.
pub fn require_current_safe_alternatives_empty_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(context.values.memory)
            || qualification.reserved_bytes() < 512
            || !transaction.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        qualification.validate_digest(&transaction.parent_fence().parent_roots)?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (context.take_state_error)() {
            return Err(error.into());
        }
        let _phase = context
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let field = family::outer();
        let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let metadata = lookup(
            transaction,
            &meta,
            &mut records,
            &mut context.take_state_error,
        )?
        .ok_or_else(invalid)?;
        records.presence(&metadata, &meta, 0)?;
        let (key, _) = records.key(field, WorldFieldRecordKindV1::Entry, &())?;
        let value = lookup(
            transaction,
            &key,
            &mut records,
            &mut context.take_state_error,
        )?
        .ok_or_else(invalid)?;
        let current = family::child_records(&key, &value, &mut records)?;
        if current != *qualification.body() || qualification.safe_alternatives_len() != 0 {
            return Err(ValueTreeError::Invalid("unsupported current safe alternatives").into());
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (context.take_state_error)() {
            return Err(error.into());
        }
        Ok(())
    })
}

/// Replaces the exact qualified parent body with a closed original typed root.
/// Subsequent ordinary edits require the private current token below. This
/// adapter grants no revocation/business authorization or head installation.
#[allow(clippy::too_many_arguments)]
pub fn change_runtime_revocation_overlay_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    ER: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CR: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    expected: &StateRootV1,
    after: &StateRootV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    change_runtime_revocation_inner_v1(
        qualification,
        None,
        expected,
        after,
        context,
        read,
        scan,
        work,
        write,
        guard,
        live,
    )
}
#[allow(clippy::too_many_arguments)]
fn change_runtime_revocation_inner_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    ER: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CR: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current_token: Option<&RuntimeRevocationCurrentV1>,
    expected: &StateRootV1,
    after: &StateRootV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let stop = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(context.values.memory)
            || qualification.reserved_bytes() < 512
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(read.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        read.check()?;
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        qualification.validate_digest(&transaction.parent_fence().parent_roots)?;
        if let Some(current) = current_token {
            current.check(transaction, expected, context.values.memory)?;
        } else if expected != qualification.body() {
            return Err(
                ValueTreeError::Invalid("revocation before lacks current qualification").into(),
            );
        }
        let field = family::outer();
        let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let (key, current) = {
            let mut combined = || stop() || (write.cancelled)() || (read.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let metadata = lookup(transaction, &meta, &mut records, error)?.ok_or_else(invalid)?;
            records.presence(&metadata, &meta, 0)?;
            let (key, _) = records.key(field, WorldFieldRecordKindV1::Entry, &())?;
            let value = lookup(transaction, &key, &mut records, error)?.ok_or_else(invalid)?;
            (key, value)
        };
        let current = {
            let mut combined_read = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || stop() || (write.cancelled)() || (read.cancelled)(),
            };
            family::child(&key, &current, &mut combined_read)?
        };
        if current != *expected {
            return Err(ValueTreeError::Invalid("revocation exact before").into());
        }
        if stop() || (write.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if expected == after {
            read.check()?;
            return Ok(false);
        }
        {
            let mut combined_read = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || stop() || (write.cancelled)() || (read.cancelled)(),
            };
            let trees_remaining = work.max_retained_trees;
            let profile = profile();
            crate::incremental_typed_record_diff::accept_body::<
                crate::CapabilityRevocationState,
                _,
                _,
                _,
                _,
            >(
                &profile,
                Some(expected),
                Some(after),
                &mut crate::incremental_typed_record_diff::ChangeDecoder {
                    ctx: &mut combined_read,
                    scan,
                    work,
                    profile,
                    trees_remaining,
                    tree_depth: 0,
                },
            )?;
        }
        if stop() || (write.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let _reference_charge = write
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let reference =
            RecordRootRefV1::new(schema(), owner, after.clone()).map_err(|_| invalid())?;
        let row = write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || stop() || (write.cancelled)() || (read.cancelled)(),
            },
        )?;
        if row.field_key != key {
            return Err(invalid().into());
        }
        transaction.validate_guard(&transaction.guard(), live)?;
        transaction.write(
            StateNamespaceV1::Authorization,
            key.as_bytes(),
            Some(row.value_root),
        )?;
        if stop() || (write.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

/// Authenticated current body for one in-process shared transaction. Its fields
/// cannot be fabricated from an arbitrary provisional root or another transaction.
pub struct RuntimeRevocationCurrentV1 {
    parent: [u8; 32],
    guard: PreparedGuard,
    body: ReservedValue<StateRootV1>,
}
impl RuntimeRevocationCurrentV1 {
    pub fn body(&self) -> &StateRootV1 {
        &self.body
    }
    fn check<M: MerkleNodeReader>(
        &self,
        transaction: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        expected: &StateRootV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        transaction.validate_transaction_identity(&self.guard)?;
        if self.parent != transaction.parent_fence().parent_roots
            || &*self.body != expected
            || !self.body.uses_budget(memory)
            || self.body.reserved_bytes() < 512
        {
            return Err(invalid().into());
        }
        Ok(())
    }
}
/// Point-authenticate the parent's complete revocation body before the first
/// ordinary edit. Later edits reuse the private current token across revisions.
#[allow(clippy::too_many_arguments)]
pub fn begin_runtime_revocation_current_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    ER: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CR: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<RuntimeRevocationCurrentV1, WorldFieldOverlayErrorV1> {
    change_runtime_revocation_overlay_v1(
        qualification,
        qualification.body(),
        qualification.body(),
        context,
        read,
        scan,
        work,
        write,
        guard,
        live,
    )?;
    context
        .transaction
        .contribute(|transaction| -> Result<_, WorldFieldOverlayErrorV1> {
            read.check()?;
            if (context.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let body =
                TypedTreeBuilderV1::from_root(qualification.body(), write.memory)?.into_reserved();
            let parent = transaction.parent_fence().parent_roots;
            read.check()?;
            if (context.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            transaction.validate_guard(&transaction.guard(), live)?;
            Ok(RuntimeRevocationCurrentV1 {
                parent,
                guard: transaction.guard(),
                body,
            })
        })
}
/// One exact leaf edit in the owner's closed typed record graph. Paths contain
/// canonical semantic keys; map members use Pair("entry", original map key).
/// Container count/presence changes must close in the same batch.
pub struct RuntimeRevocationPathEditV1<'a> {
    pub path: &'a [&'a [u8]],
    pub expected: Option<[u8; 32]>,
    pub after: Option<MemberPayloadRefV1<'a>>,
}
/// Applies a finite batch without reconstructing unchanged original histories.
/// This validates storage structure; business revocation authority remains with
/// the caller. Any failing stage poisons the same shared execution transaction.
#[allow(clippy::too_many_arguments)]
pub fn patch_runtime_revocation_overlay_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    ER: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CR: FnMut() -> bool,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &mut RuntimeRevocationCurrentV1,
    edits: &[RuntimeRevocationPathEditV1<'_>],
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    write_work: &mut RevocationWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let stop = &mut context.cancelled;
    let after = context.transaction.contribute(
        |transaction| -> Result<ReservedValue<StateRootV1>, WorldFieldOverlayErrorV1> {
            transaction.validate_guard(guard, live)?;
            read.check()?;
            if context.values.class != MemoryClass::Ordinary
                || read.values.class != MemoryClass::Ordinary
                || qualification.allocation_class() != MemoryClass::Ordinary
                || qualification.reserved_bytes() < 512
                || !qualification.uses_budget(context.values.memory)
                || !transaction.uses_budget(context.values.memory)
                || !transaction.uses_budget(read.values.memory)
                || !transaction.uses_budget(write.memory)
            {
                return Err(invalid().into());
            }
            qualification.validate_digest(&transaction.parent_fence().parent_roots)?;
            current.check(transaction, &current.body, write.memory)?;
            let mut body = TypedTreeBuilderV1::from_root(&current.body, write.memory)?;
            for edit in edits {
                if stop() || (read.cancelled)() || (write.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                let mut combined_read = WorldFieldReadContextV1 {
                    root: read.root,
                    state: read.state,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: read.values.source,
                        memory: read.values.memory,
                        class: read.values.class,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: || stop() || (read.cancelled)(),
                };
                crate::incremental_typed_record_patch::patch_typed_path_v1(
                    &profile(),
                    &mut body,
                    edit.path,
                    edit.expected,
                    edit.after.map(Encoded::Scalar),
                    &mut combined_read,
                    write,
                    write_work,
                )?;
            }
            read.check()?;
            if stop() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(body.into_reserved())
        },
    )?;
    // No parent mutation occurred during the charged child edits. Revalidate the
    // current outer body and live fence, then close the entire typed batch before
    // the single publication. This also validates empty/no-op batches.
    let changed = change_runtime_revocation_inner_v1(
        qualification,
        Some(current),
        &current.body,
        &after,
        context,
        read,
        scan,
        work,
        write,
        guard,
        live,
    )?;
    current.body = after;
    Ok(changed)
}
