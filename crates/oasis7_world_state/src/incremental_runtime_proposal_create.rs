//! Atomic qualified proposal creation contributes into the caller transaction.
use super::*;
use crate::{
    Proposal,
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::StatePageWriter,
};
pub fn create_runtime_proposal_overlay_v1<M, R, E, C, W, CW>(
    parent: &ReservedValue<RuntimeProposalsQualificationV1>,
    roots: &RootSetV1,
    original: u64,
    source: &Proposal,
    transaction: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    work: &mut RuntimeProposalWriteWorkV1,
) -> Result<ReservedValue<RuntimeProposalBodyQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
{
    transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        parent.validate_binding(roots)?;
        if parent.class != MemoryClass::Ordinary
            || parent.reserved_bytes() < 512
            || !parent.uses_budget(read.values.memory)
            || read.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(read.values.memory)
            || !tx.uses_budget(write.memory)
            || tx.parent_fence().parent_roots != parent.roots
        {
            return Err(invalid().into());
        }
        let retained = read
            .values
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _scratch = read
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let cancel = RefCell::new((&mut read.cancelled, &mut write.cancelled));
        let mut combined = || {
            let mut pair = cancel.borrow_mut();
            (pair.0)() || (pair.1)()
        };
        let key = {
            let mut records = InventoryValueReader {
                values: &mut read.values,
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                cancelled: &mut combined,
            };
            let metadata = WorldFieldKeyV1::new(
                runtime_proposal_descriptor_v1(),
                WorldFieldRecordKindV1::Metadata,
                None,
            )
            .map_err(ValueTreeError::Invalid)?;
            let meta = lookup(tx, &metadata, &mut records, &mut read.take_state_error)?
                .ok_or_else(invalid)?;
            records.presence(&meta, &metadata, 0)?;
            let (key, _) = records.key(
                runtime_proposal_descriptor_v1(),
                WorldFieldRecordKindV1::Entry,
                &original,
            )?;
            if lookup(tx, &key, &mut records, &mut read.take_state_error)?.is_some() {
                return Err(invalid().into());
            }
            key
        };
        let (root, row) = {
            let mut producer = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: &mut combined,
            };
            let root = prepare_runtime_proposal_body_v1(source, &mut producer, work)?;
            let owner = crate::incremental_semantic_key::hash_semantic_key_v1(
                &original,
                producer.memory,
                MemoryClass::Ordinary,
                producer.key_work,
                &mut producer.cancelled,
            )
            .map_err(MemberRecordError::Key)?;
            let reference = RecordRootRefV1::new(schema(), owner.digest, root.clone())
                .map_err(|_| invalid())?;
            let row = write_member_record_v1(
                runtime_proposal_descriptor_v1(),
                WorldFieldRecordKindV1::Entry,
                &original,
                MemberPayloadRefV1::RecordRoot(&reference),
                &mut producer,
            )?;
            (root, row)
        };
        if row.field_key != key {
            return Err(invalid().into());
        }
        if combined() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let ready = tx.guard();
        tx.validate_guard(&ready, live)?;
        tx.write(
            StateNamespaceV1::Authorization,
            key.as_bytes(),
            Some(row.value_root),
        )?;
        if combined() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(retained.retain(RuntimeProposalBodyQualificationV1 {
            parent: parent.roots,
            original,
            root,
            class: MemoryClass::Ordinary,
        }))
    })
}
