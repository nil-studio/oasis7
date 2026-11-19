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
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
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

/// Original module-request projection of all actual Current ledgers. Empty
/// ledgers survive; only strictly positive quantities become MaterialStacks.
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn recover_current_inventory_stacks_by_ledger_v1<M, R, E, C, P, V, ER, CR>(
    directory: &ReservedValue<InventoryIdentityDirectoryV1>,
    family: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    family_work: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<BTreeMap<String, Vec<oasis7_wasm_abi::MaterialStack>>>,
    WorldFieldOverlayErrorV1,
>
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
        let mut groups = 0usize;
        let mut entries = 0usize;
        let mut text = 0usize;
        tree::all(
            &tree,
            work,
            &mut || (overlay.cancelled)() || (parent.cancelled)(),
            &mut |id| {
                if let Some(ledger) = &id.ledger {
                    if let Some(material) = &id.material {
                        entries = entries.checked_add(1).ok_or_else(invalid)?;
                        text = text.checked_add(material.len()).ok_or_else(invalid)?;
                    } else {
                        groups = groups.checked_add(1).ok_or_else(invalid)?;
                        let (_, name) = ledger_parts(Some(ledger_ref(ledger)));
                        text = name
                            .len()
                            .checked_add(32)
                            .and_then(|n| n.checked_mul(2))
                            .and_then(|n| text.checked_add(n))
                            .ok_or_else(invalid)?;
                    }
                }
                Ok(())
            },
        )?;
        if groups.checked_add(entries).ok_or_else(invalid)? > work.max_output_entries {
            return Err(ValueTreeError::ResourceLimited("inventory module input entries").into());
        }
        let memory = overlay.values.memory;
        let group_fee = memory
            .reserve(
                groups
                    .checked_mul(size_of::<Arc<ReservedValue<Identity>>>())
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut ledgers = Vec::with_capacity(groups);
        if ledgers.capacity() > groups {
            return Err(invalid().into());
        }
        tree::all(
            &tree,
            work,
            &mut || (overlay.cancelled)() || (parent.cancelled)(),
            &mut |id| {
                if id.ledger.is_some() && id.material.is_none() {
                    ledgers.push(Arc::clone(id));
                }
                Ok(())
            },
        )?;
        // One BTree node per entry is deliberately conservative, including
        // header/edge padding; all Strings and complete vector capacity are paid.
        let bytes = groups
            .checked_mul(
                16 * (size_of::<String>()
                    + size_of::<Vec<oasis7_wasm_abi::MaterialStack>>()
                    + 4 * size_of::<usize>()),
            )
            .and_then(|n| {
                entries
                    .checked_mul(size_of::<oasis7_wasm_abi::MaterialStack>())
                    .and_then(|v| n.checked_add(v))
            })
            .and_then(|n| n.checked_add(text))
            .and_then(|n| {
                n.checked_add(size_of::<
                    BTreeMap<String, Vec<oasis7_wasm_abi::MaterialStack>>,
                >())
            })
            .ok_or_else(invalid)?;
        let fee = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut output = BTreeMap::new();
        for id in &ledgers {
            let ledger = id.ledger.as_ref().ok_or_else(invalid)?;
            let guard = overlay.transaction.guard();
            let values = recover_current_inventory_ledger_v1(
                ledger,
                directory,
                family,
                roots,
                overlay,
                parent,
                family_work,
                work,
                &guard,
                live,
            )?;
            let values = values.as_ref().ok_or_else(invalid)?;
            let mut stacks = Vec::with_capacity(values.len());
            if stacks.capacity() > values.len() {
                return Err(invalid().into());
            }
            for (kind, quantity) in values {
                work.step(kind.len(), &mut || {
                    (overlay.cancelled)() || (parent.cancelled)()
                })?;
                if *quantity > 0 {
                    let owned = kind.clone();
                    if owned.capacity() > kind.len() {
                        return Err(invalid().into());
                    }
                    stacks.push(oasis7_wasm_abi::MaterialStack::new(owned, *quantity));
                }
            }
            let (_, name) = ledger_parts(Some(ledger_ref(ledger)));
            let comparisons = 12usize * usize::BITS as usize;
            work.visits_remaining = work
                .visits_remaining
                .checked_sub(comparisons as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "inventory module map comparisons",
                ))?;
            work.comparison_bytes_remaining = work
                .comparison_bytes_remaining
                .checked_sub(
                    (name
                        .len()
                        .checked_add(32)
                        .and_then(|n| n.checked_mul(comparisons))
                        .ok_or_else(invalid)?) as u64,
                )
                .ok_or(ValueTreeError::ResourceLimited(
                    "inventory module map comparison bytes",
                ))?;
            let key = ledger.to_string();
            if key.capacity()
                > name
                    .len()
                    .checked_add(32)
                    .and_then(|n| n.checked_mul(2))
                    .ok_or_else(invalid)?
            {
                return Err(invalid().into());
            }
            // Matches original BTree<MaterialLedgerId>::iter().collect(): a
            // display-key collision replaces the earlier ledger's projection.
            output.insert(key, stacks);
        }
        drop(ledgers);
        drop(group_fee);
        let guard = overlay.transaction.guard();
        overlay.transaction.validate_guard(&guard, live)?;
        Ok(fee.retain(output))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = overlay
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
