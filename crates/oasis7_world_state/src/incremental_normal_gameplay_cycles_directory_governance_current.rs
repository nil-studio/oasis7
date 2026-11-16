//! Actual selected Current Proposal/Vote, not a World or execution capability.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[allow(clippy::too_many_arguments)]
pub fn open_current_gameplay_governance_record_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: GovernanceRecordKeyV1<'_>,
    parent: &ReservedValue<GovernanceFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<GovernanceRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        ctx.check()?;
        if !matches!(
            key.kind,
            GovernanceRecordKindV1::Proposal | GovernanceRecordKindV1::Vote
        ) || ctx.values.class != MemoryClass::Ordinary
            || parent.allocation_class() != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(ValueTreeError::Invalid("gameplay Current governance source").into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = roots
            .digest()
            .map_err(|_| ValueTreeError::Invalid("gameplay governance parent roots"))?;
        parent.validate_binding(&digest)?;
        if digest != tx.parent_fence().parent_roots
            || ctx.root != roots.state_root(key.kind.namespace())
        {
            return Err(ValueTreeError::Invalid("gameplay governance parent fence").into());
        }
        let (outer, child) = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            point::metadata(key, tx, &mut records, &mut ctx.take_state_error)?;
            let outer = read::outer_key(key, &mut records)?;
            let Some(value) = lookup(tx, &outer.0, &mut records, &mut ctx.take_state_error)? else {
                ctx.check()?;
                tx.validate_guard(&tx.guard(), live)?;
                return Ok(None);
            };
            let child = read::outer(key.kind, &outer, &value, &mut records)?;
            (outer.0, child)
        };
        let owner = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or(ValueTreeError::Invalid("original governance owner key"))?;
        let mut body = WorldFieldReadContextV1 {
            root: &child,
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        let result = qualify_governance_body_v1(key.kind, owner, &mut body, scan, cells, max)?;
        body.check()?;
        ctx.check()?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(Some(result))
    })
}

pub struct CurrentGameplayTallyV1 {
    option: ReservedValue<String>,
    weight: u64,
}
pub struct CurrentGameplayTalliesV1 {
    rows: ReservedValue<Vec<CurrentGameplayTallyV1>>,
    guard: PreparedGuard,
    memory: oasis7_proto::memory_budget::MemoryBudget,
}
impl CurrentGameplayTalliesV1 {
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, u64)> {
        self.rows
            .iter()
            .map(|row| (row.option.as_str(), row.weight))
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self.rows.uses_budget(memory)
            || !tx.uses_budget(memory)
        {
            return Err(ValueTreeError::Invalid("Current gameplay tallies scope").into());
        }
        Ok(())
    }
}
/// Read the complete original Tallies map of one real Current Vote. Snapshot,
/// ballots and unrelated fields are authenticated by the same closed proof but
/// are never restored into a World or cloned into a second vote DTO.
#[allow(clippy::too_many_arguments)]
pub fn recover_current_gameplay_tallies_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    proof: &ReservedValue<GovernanceRecordQualificationV1>,
    state: &M,
    ctx: &mut crate::incremental_field_overlay::WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut crate::incremental_normal_gameplay_cycles_directory::GameplayDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentGameplayTalliesV1>, WorldFieldOverlayErrorV1> {
    tx_tallies(id, proof, state, ctx, scan, work, guard, live)
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
fn tx_tallies<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    proof: &ReservedValue<GovernanceRecordQualificationV1>,
    state: &M,
    ctx: &mut crate::incremental_field_overlay::WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut crate::incremental_normal_gameplay_cycles_directory::GameplayDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentGameplayTalliesV1>, WorldFieldOverlayErrorV1> {
    use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
    use std::{cell::RefCell, mem::size_of};
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let key = GovernanceRecordKeyV1 {
            kind: GovernanceRecordKindV1::Vote,
            id,
        };
        let count = match read_qualified_governance_cell_overlay_v1(
            GovernanceCellRequestV1 {
                key,
                cell: GovernanceCellV1::Count(GovernanceCollectionV1::Tallies),
                qualification: proof,
            },
            ctx,
            8,
            &mut std::io::sink(),
            guard,
            live,
        )? {
            Some(GovernanceValueV1::Unsigned(n)) => usize::try_from(n)
                .map_err(|_| ValueTreeError::ResourceLimited("gameplay tally count"))?,
            _ => return Err(ValueTreeError::Invalid("original gameplay tally count").into()),
        };
        let bytes = count
            .checked_mul(size_of::<CurrentGameplayTallyV1>())
            .and_then(|n| n.checked_add(size_of::<CurrentGameplayTalliesV1>()))
            .ok_or(ValueTreeError::ResourceLimited("gameplay tally layout"))?;
        work.step(bytes, &mut ctx.cancelled)?;
        let carrier = ctx
            .values
            .memory
            .reserve(size_of::<CurrentGameplayTalliesV1>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let fee = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(count)
            .map_err(|_| ValueTreeError::ResourceLimited("gameplay tally slots"))?;
        if rows.capacity() > count {
            return Err(ValueTreeError::ResourceLimited("gameplay tally capacity").into());
        }
        let stop = RefCell::new(&mut ctx.cancelled);
        let memory = ctx.values.memory;
        let mut stopped = || (stop.borrow_mut())();
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut stopped,
        };
        let result = StateScanContext {
            source: state,
            memory,
            class: MemoryClass::Ordinary,
            io: scan,
        }
        .visit_entries(
            proof.root(),
            |entry| {
                work.step(entry.key.len(), &mut || (stop.borrow_mut())())?;
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if parsed.kind == WorldFieldRecordKindV1::Metadata {
                    return Ok(());
                }
                key_read::with_cell(
                    GovernanceRecordKindV1::Vote,
                    entry,
                    &mut records,
                    work.max_payload_bytes,
                    false,
                    |cell, original, records| {
                        if let GovernanceCellV1::Tally(option) = cell {
                            if rows.len() == count {
                                return Err(ValueTreeError::Invalid(
                                    "original tally count closure",
                                ));
                            }
                            let GovernanceValueV1::Unsigned(weight) = read::value(
                                cell,
                                &entry.value,
                                original,
                                records,
                                8,
                                &mut std::io::sink(),
                            )?
                            else {
                                return Err(ValueTreeError::Invalid("original tally value shape"));
                            };
                            work.step(option.len(), &mut || (stop.borrow_mut())())?;
                            let paid = memory.reserve(
                                option.len().checked_add(size_of::<String>()).ok_or(
                                    ValueTreeError::ResourceLimited("gameplay tally text"),
                                )?,
                                MemoryClass::Ordinary,
                            )?;
                            rows.push(CurrentGameplayTallyV1 {
                                option: paid.retain(option.to_owned()),
                                weight,
                            });
                        }
                        Ok(())
                    },
                )
            },
            || (stop.borrow_mut())(),
        );
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(e)) => return Err(e.into()),
            Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("gameplay tally scan").into());
            }
            Err(_) => return Err(ValueTreeError::Invalid("gameplay tally tree").into()),
        }
        if rows.len() != count {
            return Err(ValueTreeError::Invalid("complete gameplay tallies count").into());
        }
        let guard = ctx.transaction.guard();
        ctx.transaction.validate_guard(&guard, live)?;
        Ok(carrier.retain(CurrentGameplayTalliesV1 {
            rows: fee.retain(rows),
            guard,
            memory: memory.clone(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
