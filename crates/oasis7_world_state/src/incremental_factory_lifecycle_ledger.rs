//! Empty output-ledger creation bound to a genuine Current factory and the
//! complete inventory parent plus actual typed pending deltas. Inventory child.
use super::*;
use crate::{
    MaterialLedgerId,
    incremental_factory_records::*,
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_field_presence::{PresenceRecordWriteContext, write_field_group_v1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_ledger_value::LedgerValueKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
use std::cell::RefCell;
pub struct FactoryOutputLedgerGroupWorkV1 {
    pub pending_entries_remaining: usize,
    pub max_ledger_bytes: usize,
}
#[allow(clippy::too_many_arguments)]
pub fn ensure_current_factory_output_ledger_group_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    factory_id: &str,
    factory: &ReservedValue<FactoryFamilyQualificationV1>,
    inventory: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    factory_work: &mut FactoryWorkBudgetV1,
    inventory_work: &mut InventoryFamilyWorkV1,
    work: &mut FactoryOutputLedgerGroupWorkV1,
    max_payload_bytes: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        ctx.check()?;
        let memory = ctx.values.memory;
        let _phase = memory
            .reserve(
                work.max_ledger_bytes
                    .checked_add(64 << 10)
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !factory.uses_budget(memory)
            || !inventory.uses_budget(memory)
            || !tx.uses_budget(memory)
            || !memory.same_pool(write.memory)
            || factory.allocation_class() != MemoryClass::Ordinary
            || inventory.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let digest = roots.digest().map_err(|_| invalid())?;
        if digest != tx.parent_fence().parent_roots
            || ctx.root != roots.state_root(StateNamespaceV1::Business)
        {
            return Err(invalid().into());
        }
        factory.validate_binding(&digest)?;
        inventory.validate_binding(roots)?;
        inventory.check(memory, MemoryClass::Ordinary)?;
        // The existing Current factory owner validates the entire typed child.
        let proof = prepare_current_factory_delete_v1(
            factory_id,
            factory,
            roots,
            tx,
            ctx,
            factory_work,
            max_payload_bytes,
            guard,
            live,
        )?;
        if proof.factory_id() != factory_id {
            return Err(invalid().into());
        }
        let mut bytes = Vec::with_capacity(work.max_ledger_bytes);
        let value = {
            let mut overlay = crate::incremental_field_overlay::WorldFieldOverlayContextV1 {
                transaction: tx,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            read_factory_cell_overlay_v1(
                FactoryRecordKeyV1::Factory(factory_id),
                FactoryCellV1::Column(FactoryColumnV1::FactoryOutputLedger),
                &mut overlay,
                work.max_ledger_bytes as u64,
                &mut bytes,
            )?
        };
        let FactoryCellReadV1::Value(FactoryValueV1::Ledger(info)) = value else {
            return Err(invalid().into());
        };
        let id = String::from_utf8(bytes).map_err(|_| invalid())?;
        if usize::try_from(info.id_bytes).ok() != Some(id.len()) {
            return Err(invalid().into());
        }
        let ledger = match info.kind {
            LedgerValueKindV1::World => MaterialLedgerId::World,
            LedgerValueKindV1::Agent => MaterialLedgerId::Agent(id),
            LedgerValueKindV1::Site => MaterialLedgerId::Site(id),
            LedgerValueKindV1::Factory => MaterialLedgerId::Factory(id),
        };
        let field = descriptor(
            "material_ledgers",
            crate::incremental_projection::FieldShape::NestedMap,
        )
        .map_err(|_| invalid())?;
        let group = ctx.key(field, WorldFieldRecordKindV1::Group, &ledger)?;
        let hash = WorldFieldKeyV1::from_bytes(group.0.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let mut index_work = InventoryIndexWorkV1 {
            steps_remaining: std::mem::take(&mut inventory_work.index_steps_remaining),
        };
        let found = inventory
            .groups
            .get(hash, &mut index_work, &mut ctx.cancelled);
        inventory_work.index_steps_remaining = index_work.steps_remaining;
        let before = found?.unwrap_or(InventoryGroupMetaV1 {
            key: hash,
            present: false,
            members: 0,
        });
        let (mut present, mut members) = (before.present, i128::from(before.members));
        let mut remaining = std::mem::take(&mut work.pending_entries_remaining);
        let root_context = ctx.root;
        let state_context = ctx.state;
        let semantic_limit = ctx.semantic_bytes;
        let record_limit = ctx.record_bytes;
        let cancellation = RefCell::new(&mut ctx.cancelled);
        let values = RefCell::new(&mut ctx.values);
        let keys = RefCell::new(&mut ctx.key_work);
        let error = RefCell::new(&mut ctx.take_state_error);
        let result = tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
            StateNamespaceV1::Business,
            &mut remaining,
            || {
                if (cancellation.borrow_mut())() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |raw, old, new| {
                let Some(key) = key(raw)? else { return Ok(()) };
                let parsed =
                    WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
                if parsed.name != "material_ledgers" {
                    return Ok(());
                }
                if parsed.kind == WorldFieldRecordKindV1::Metadata {
                    if old != new {
                        return Err(invalid().into());
                    }
                    return Ok(());
                }
                let mut decode =
                    |root: Option<[u8; 32]>| -> Result<Option<Row>, WorldFieldOverlayErrorV1> {
                        let Some(root) = root else { return Ok(None) };
                        let mut values = values.borrow_mut();
                        let mut keys = keys.borrow_mut();
                        let mut error = error.borrow_mut();
                        let mut local = WorldFieldReadContextV1 {
                            root: root_context,
                            state: state_context,
                            values: ValueReadContext {
                                source: values.source,
                                memory,
                                class: MemoryClass::Ordinary,
                                io: &mut *values.io,
                            },
                            key_work: &mut **keys,
                            semantic_bytes: semantic_limit,
                            record_bytes: record_limit,
                            take_state_error: &mut **error,
                            cancelled: || (cancellation.borrow_mut())(),
                        };
                        Ok(Some(row(&key, &root, &mut local, inventory_work)?))
                    };
                let old = decode(old)?;
                let new = decode(new)?;
                if matches!(old,Some(Row::Group(h))if h==hash) {
                    if !present {
                        return Err(invalid().into());
                    }
                    present = false;
                }
                if matches!(new,Some(Row::Group(h))if h==hash) {
                    if present {
                        return Err(invalid().into());
                    }
                    present = true;
                }
                if matches!(old,Some(Row::Member(h))if h==hash) {
                    members = members.checked_sub(1).ok_or_else(invalid)?;
                }
                if matches!(new,Some(Row::Member(h))if h==hash) {
                    members = members.checked_add(1).ok_or_else(invalid)?;
                }
                Ok(())
            },
        );
        work.pending_entries_remaining = remaining;
        result?;
        // End all visitor adapters before borrowing the original IO/context.
        let _ = (cancellation, values, keys, error);
        if members < 0 || (!present && members != 0) {
            return Err(invalid().into());
        }
        let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let actual = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            let meta = lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?
                .ok_or_else(invalid)?;
            records.presence(&meta, &metadata, 0)?;
            let actual = lookup(tx, &group.0, &mut records, &mut ctx.take_state_error)?;
            if let Some(root) = actual {
                records.presence(&root, &group.0, group.1)?;
            }
            actual
        };
        if actual.is_some() != present {
            return Err(invalid().into());
        }
        if present {
            ctx.check()?;
            if (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        inventory_work.step(&mut ctx.cancelled)?;
        let record = write_field_group_v1(
            field,
            &ledger,
            &mut PresenceRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: write.limits.semantic_bytes,
                    record_bytes: write.limits.record_bytes,
                },
                cancelled: || (ctx.cancelled)() || (write.cancelled)(),
            },
        )
        .map_err(WorldFieldOverlayErrorV1::Member)?;
        if record.field_key != group.0 {
            return Err(invalid().into());
        }
        ctx.check()?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Business,
            group.0.as_bytes(),
            Some(record.value_root),
        )?;
        ctx.check()?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
