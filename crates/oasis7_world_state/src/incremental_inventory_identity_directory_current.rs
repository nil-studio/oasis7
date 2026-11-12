//! Current identity folding reads only real pending inventory records. The
//! selected ledger then point-reads every original material, including zero
//! and negative quantities; an absent ledger remains distinct from empty.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, PreparedGuard,
};
use std::collections::BTreeMap;
pub(super) fn fold<M, R, E, C, P, V, ER, CR>(
    directory: &ReservedValue<InventoryIdentityDirectoryV1>,
    family: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    family_work: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
) -> Result<Option<tree::NodeRef>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    if parent.values.class != MemoryClass::Ordinary
        || !parent.values.memory.same_pool(overlay.values.memory)
    {
        return Err(invalid().into());
    }
    fold_with_state(
        directory,
        family,
        roots,
        overlay,
        parent.state,
        &mut parent.cancelled,
        family_work,
        work,
    )
}
pub(super) fn fold_with_state<M, R, E, C, P, PC>(
    directory: &ReservedValue<InventoryIdentityDirectoryV1>,
    family: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    state: &P,
    parent_cancelled: &mut PC,
    family_work: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
) -> Result<Option<tree::NodeRef>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    PC: FnMut() -> bool,
{
    directory.validate_binding(roots)?;
    family.validate_binding(roots)?;
    family.check(overlay.values.memory, MemoryClass::Ordinary)?;
    if overlay.values.class != MemoryClass::Ordinary
        || state.allocation_class() != Some(MemoryClass::Ordinary)
        || state
            .allocation_budget()
            .is_none_or(|m| !m.same_pool(overlay.values.memory))
        || !directory.uses_budget(overlay.values.memory)
        || !family.uses_budget(overlay.values.memory)
        || !overlay.transaction.uses_budget(overlay.values.memory)
        || overlay.transaction.parent_fence().parent_roots
            != roots.digest().map_err(|_| invalid())?
    {
        return Err(invalid().into());
    }
    let memory = overlay.values.memory;
    let mut tree = directory.tree.clone();
    let stops = std::cell::RefCell::new((&mut overlay.cancelled, parent_cancelled));
    let stop = std::cell::RefCell::new(|| {
        let mut s = stops.borrow_mut();
        (s.0)() || (s.1)()
    });
    let mut local = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: overlay.values.source,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut *overlay.values.io,
        },
        key_work: &mut *overlay.key_work,
        semantic_bytes: overlay.semantic_bytes,
        record_bytes: overlay.record_bytes,
        take_state_error: &mut overlay.take_state_error,
        cancelled: || (stop.borrow_mut())(),
    };
    // Cancellation aliases are shared only through this cell, never a second
    // budget or independently reset source work counter.
    let mut remaining = work.pending_entries_remaining;
    let result = overlay
        .transaction
        .visit_pending_writes::<WorldFieldOverlayErrorV1>(
            StateNamespaceV1::Business,
            &mut remaining,
            || {
                if (stop.borrow_mut())() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |physical, before, after| {
                let Some(key) = key(physical)? else {
                    return Ok(());
                };
                let value = after.or(before).ok_or_else(invalid)?;
                let mut update = None;
                let row = row_with_semantic(
                    &key,
                    &value,
                    &mut local,
                    family_work,
                    |parsed, semantic| {
                        if let Some(id) = semantic_identity(parsed, semantic)? {
                            if after.is_some() {
                                let id = retain_identity(id, memory, work, &mut || {
                                    (stop.borrow_mut())()
                                })?;
                                update = Some(
                                    tree::put(&tree, id, memory, work, &mut || {
                                        (stop.borrow_mut())()
                                    })?
                                    .0,
                                );
                            } else {
                                let (next, removed) =
                                    tree::remove(&tree, id, memory, work, &mut || {
                                        (stop.borrow_mut())()
                                    })?;
                                if !removed {
                                    return Err(invalid());
                                }
                                tree = next;
                            }
                        }
                        Ok(())
                    },
                )?;
                if matches!(row, Row::Meta(_)) && after.is_none() {
                    return Err(invalid().into());
                }
                if let Some(next) = update {
                    tree = Some(next);
                }
                Ok(())
            },
        );
    work.pending_entries_remaining = remaining;
    result?;
    local.check()?;
    Ok(tree)
}
// Current supported Rust BTree nodes contain <=11 keys/values and <=12 child
// edges (B=6). One full internal-node layout per output entry plus two scratch
// nodes bounds both leaf/internal allocations and transient root splitting.
// Header/padding pays eight machine words in addition to all key/value/edges.
fn output_node_heap() -> Result<usize, ValueTreeError> {
    11usize
        .checked_mul(size_of::<String>() + size_of::<i64>() + 2 * align_of::<String>())
        .and_then(|n| n.checked_add(20 * size_of::<usize>()))
        .ok_or_else(invalid)
}
#[allow(clippy::too_many_arguments)]
pub fn recover_current_inventory_ledger_v1<M, R, E, C, P, V, ER, CR>(
    ledger: &MaterialLedgerId,
    directory: &ReservedValue<InventoryIdentityDirectoryV1>,
    family: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    family_work: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<BTreeMap<String, i64>>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let result = (|| {
        overlay.transaction.validate_guard(guard, live)?;
        let tree = fold(directory, family, roots, overlay, parent, family_work, work)?;
        let memory = overlay.values.memory;
        let mut count = 0usize;
        let mut payload = 0usize;
        let mut present = false;
        tree::ledger(
            &tree,
            Some(ledger_ref(ledger)),
            work,
            &mut || (overlay.cancelled)() || (parent.cancelled)(),
            &mut |id, work, _| {
                if let Some(material) = &id.material {
                    count = count.checked_add(1).ok_or_else(invalid)?;
                    if count > work.max_output_entries {
                        return Err(ValueTreeError::ResourceLimited(
                            "inventory selected ledger entries",
                        ));
                    }
                    payload = payload.checked_add(material.len()).ok_or_else(invalid)?;
                } else {
                    if present {
                        return Err(invalid());
                    }
                    present = true;
                }
                Ok(())
            },
        )?;
        if !present && count != 0 {
            return Err(invalid().into());
        }
        let bytes = output_node_heap()?
            .checked_mul(count.checked_add(2).ok_or_else(invalid)?)
            .and_then(|n| n.checked_add(payload))
            .and_then(|n| n.checked_add(size_of::<Option<BTreeMap<String, i64>>>()))
            .ok_or_else(invalid)?;
        let fee = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // Even an empty/absent ledger authenticates actual Current group presence
        // through the existing full inventory point decoder.
        let actual_present = !matches!(
            overlay.ledger(ledger, "")?,
            crate::incremental_inventory_read::LedgerQuantityV1::LedgerAbsent
        );
        if present != actual_present {
            return Err(invalid().into());
        }
        let mut output = BTreeMap::new();
        let stop = std::cell::RefCell::new((&mut overlay.cancelled, &mut parent.cancelled));
        // Use one records view and the original resolve_point kernel so the
        // selected loop shares its actual transaction and cancellation source.
        let mut records_cancelled = || {
            let mut s = stop.borrow_mut();
            (s.0)() || (s.1)()
        };
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut overlay.values,
            key_work: &mut *overlay.key_work,
            semantic_bytes: overlay.semantic_bytes,
            record_bytes: overlay.record_bytes,
            cancelled: &mut records_cancelled,
        };
        let mut source_error = None;
        let recovered = tree::ledger(
            &tree,
            Some(ledger_ref(ledger)),
            work,
            &mut || {
                let mut s = stop.borrow_mut();
                (s.0)() || (s.1)()
            },
            &mut |id, work, _| {
                let Some(material) = &id.material else {
                    return Ok(());
                };
                let comparisons = 12usize
                    .checked_mul(usize::BITS as usize)
                    .ok_or_else(invalid)?;
                work.visits_remaining = work
                    .visits_remaining
                    .checked_sub(comparisons as u64)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "inventory output map comparisons",
                    ))?;
                work.comparison_bytes_remaining = work
                    .comparison_bytes_remaining
                    .checked_sub(
                        material
                            .len()
                            .checked_mul(comparisons)
                            .ok_or_else(invalid)? as u64,
                    )
                    .ok_or(ValueTreeError::ResourceLimited(
                        "inventory output map bytes",
                    ))?;
                let point = crate::incremental_inventory_overlay_read::resolve_point(
                    overlay.transaction,
                    &mut records,
                    &mut overlay.take_state_error,
                    "material_ledgers",
                    &(ledger, material.as_str()),
                    Some(ledger),
                )
                .map_err(|error| {
                    source_error = Some(error);
                    ValueTreeError::Invalid("inventory selected execution error")
                })?;
                let crate::incremental_inventory_read::LedgerQuantityV1::Value(quantity) =
                    point.quantity()
                else {
                    return Err(invalid());
                };
                let key = material.clone();
                if key.capacity() > material.len() {
                    return Err(invalid());
                }
                if output.insert(key, quantity).is_some() {
                    return Err(invalid());
                }
                Ok(())
            },
        );
        if let Some(error) = source_error {
            return Err(error);
        }
        recovered?;
        if output.len() != count {
            return Err(invalid().into());
        }
        let guard = overlay.transaction.guard();
        overlay.transaction.validate_guard(&guard, live)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(if present { Some(output) } else { None }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = overlay
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
