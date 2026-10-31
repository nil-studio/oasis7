use super::*;
use crate::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::StatePageWriter,
};
pub struct DiplomacyEmptyCreateV1<'a> {
    pub id: &'a str,
    pub source: DiplomacySourceV1<'a>,
    pub family: &'a ReservedValue<DiplomacyFamilyQualificationV1>,
    pub max_pending_node_bytes: usize,
}
pub fn create_qualified_diplomacy_empty_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: DiplomacyEmptyCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    cells: &mut u64,
) -> Result<ReservedValue<DiplomacyRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let field = change.source.field();
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        change
            .family
            .validate_binding(&tx.parent_fence().parent_roots)?;
        if change.source.length() != 0 {
            return Err(ValueTreeError::Invalid("diplomacy empty creation source vectors").into());
        }
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !change.family.uses_budget(write.memory)
        {
            return Err(closure::invalid().into());
        }
        let outer = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            access::metadata(field, tx, &mut records, error)?;
            let key = records.key(
                field.descriptor()?,
                WorldFieldRecordKindV1::Entry,
                change.id,
            )?;
            if lookup(tx, &key.0, &mut records, error)?.is_some() {
                return Err(closure::invalid().into());
            }
            key.0
        };
        let (row, root) = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let root = genesis::body(
                change.source,
                &mut combined,
                cells,
                change.max_pending_node_bytes,
            )?;
            let row = write_diplomacy_root_reference_v1(field, change.id, &root, &mut combined)?;
            (row, root)
        };
        if row.field_key != outer {
            return Err(closure::invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Business,
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let proof = access::mint(
            field,
            access::digest(&outer)?,
            &root,
            write.memory,
            MemoryClass::Ordinary,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(proof)
    })
}
