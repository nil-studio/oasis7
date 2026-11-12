//! NEW unhooked Current count over complete Contract metadata plus real deltas.
//! The eventual family hook stores `contracts: ContractCreatorIndexV1` privately;
//! this observer accepts that genuine qualification, never a caller row list.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};
use std::cell::RefCell;
struct Change {
    key: WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
pub struct CurrentContractCreatorCountV1 {
    count: usize,
    identity: PreparedGuard,
    memory: MemoryBudget,
    _rows: ContractCreatorIndexV1,
}
impl CurrentContractCreatorCountV1 {
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.identity, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(closure::invalid().into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn observe_current_creator_contract_count_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    creator: &str,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut DiplomacyIndexWorkV1,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentContractCreatorCountV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        tx.validate_guard(guard, live)?;
        read.check()?;
        if !parent.uses_budget(read.values.memory)
            || !parent.permits_class(MemoryClass::Ordinary)
            || read.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(read.values.memory)
        {
            return Err(closure::invalid().into());
        }
        let phase = read
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = roots.digest().map_err(|_| closure::invalid())?;
        parent.validate_binding(&digest)?;
        if digest != tx.parent_fence().parent_roots
            || read.root != roots.state_root(StateNamespaceV1::Business)
            || u64::try_from(parent.contracts.len()).map_err(|_| bounded())? != parent.counts[3]
        {
            return Err(closure::invalid().into());
        }
        let layout = times(work.max_pending_rows, size_of::<Change>())?;
        let fee = read
            .values
            .memory
            .reserve(
                plus(layout, size_of::<Vec<Change>>())?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut changed = Vec::with_capacity(work.max_pending_rows);
        if changed.capacity() > work.max_pending_rows {
            return Err(bounded().into());
        }
        let cancelled = RefCell::new(&mut read.cancelled);
        let pending = &mut work.pending_entries_remaining;
        let visits = &mut work.cells_remaining;
        let max_pending = work.max_pending_rows;
        tx.visit_pending_writes(
            StateNamespaceV1::Business,
            pending,
            || {
                if (**cancelled.borrow_mut())() {
                    Err(WorldFieldOverlayErrorV1::from(ValueTreeError::Cancelled))
                } else {
                    Ok(())
                }
            },
            |raw, before, after| {
                *visits = visits
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "Current contract pending visits",
                    ))?;
                if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                    return Ok(());
                }
                let parsed = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
                if parsed.name != DiplomacyFieldV1::EconomicContracts.name() {
                    return Ok(());
                }
                let key = WorldFieldKeyV1::new(
                    DiplomacyFieldV1::EconomicContracts.descriptor()?,
                    parsed.kind,
                    parsed.semantic_key,
                )
                .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != raw {
                    return Err(closure::invalid().into());
                }
                if parsed.kind == WorldFieldRecordKindV1::Metadata {
                    if parsed.semantic_key.is_some() {
                        return Err(closure::invalid().into());
                    }
                    if let Some(value) = after {
                        let mut cancellation = cancelled.borrow_mut();
                        let mut records =
                            crate::incremental_inventory_value_read::InventoryValueReader {
                                values: &mut read.values,
                                key_work: &mut *read.key_work,
                                semantic_bytes: read.semantic_bytes,
                                record_bytes: read.record_bytes,
                                cancelled: &mut **cancellation,
                            };
                        records.presence(&value, &key, 0)?;
                    } else {
                        return Err(closure::invalid().into());
                    }
                    return Ok(());
                }
                if parsed.kind != WorldFieldRecordKindV1::Entry {
                    return Err(closure::invalid().into());
                }
                if changed.len() == max_pending {
                    return Err(
                        ValueTreeError::ResourceLimited("Current contract changed rows").into(),
                    );
                }
                changed.push(Change { key, before, after });
                Ok(())
            },
        )?;
        drop(cancelled);
        let mut next = if changed.is_empty() {
            None
        } else {
            Some(parent.contracts.copy_for_changes(
                read.values.memory,
                MemoryClass::Ordinary,
                work,
                &mut read.cancelled,
            )?)
        };
        for change in changed {
            cancel(work, &mut read.cancelled)?;
            let Some(value) = change.after.or(change.before) else {
                return Err(closure::invalid().into());
            };
            let entry = StateEntryV1 {
                namespace: StateNamespaceV1::Business,
                key: change.key.as_bytes().to_vec(),
                value,
            };
            let outer = outer_text(&entry, read, work)?;
            if let Some(after) = change.after {
                let guard = tx.guard();
                let proof =
                    crate::incremental_diplomacy::open_current_diplomacy_contract_record_v1(
                        &outer,
                        parent,
                        roots,
                        tx,
                        read,
                        inner_scan,
                        cells,
                        max_payload,
                        &guard,
                        live,
                    )?;
                let proof = proof.ok_or_else(closure::invalid)?;
                let entry = StateEntryV1 {
                    namespace: StateNamespaceV1::Business,
                    key: change.key.as_bytes().to_vec(),
                    value: after,
                };
                let row = project_contract_row(&entry, proof.root(), read, work)?;
                next.as_mut()
                    .ok_or_else(closure::invalid)?
                    .put(row, work, &mut read.cancelled)?;
            } else {
                next.as_mut().ok_or_else(closure::invalid)?.remove(
                    &change.key,
                    work,
                    &mut read.cancelled,
                )?;
            }
        }
        drop(fee);
        let index = match next {
            Some(next) => next.finish()?,
            None => parent
                .contracts
                .share(read.values.memory, MemoryClass::Ordinary)?,
        };
        let count = index.active_for_creator(
            creator,
            read.values.memory,
            MemoryClass::Ordinary,
            work,
            &mut read.cancelled,
        )?;
        Ok(phase.retain(CurrentContractCreatorCountV1 {
            count,
            identity: tx.guard(),
            memory: read.values.memory.clone(),
            _rows: index,
        }))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> = tx
            .contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
