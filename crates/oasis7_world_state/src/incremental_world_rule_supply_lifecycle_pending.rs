//! Actual Current Supply lifecycle projection. Parent rows are only a seed;
//! genuine net outer writes are joined and closed before selection can use it.
//! No accepted family, transition witness, lease or business authority is minted.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::{cell::RefCell, mem::size_of, sync::Arc};
pub struct CurrentSupplyLifecycleDirectoryV1 {
    rows: lifecycle::SupplyLifecycleRowsV1,
    parent: [u8; 32],
    guard: PreparedGuard,
    memory: MemoryBudget,
    present: bool,
}
impl CurrentSupplyLifecycleDirectoryV1 {
    pub fn authorizations(
        &self,
    ) -> &[Arc<
        ReservedValue<crate::incremental_world_rules_family::LifecycleSupplyAuthorizationV1>,
    >] {
        &self.rows.authorizations
    }
    pub fn batches(
        &self,
    ) -> &[Arc<ReservedValue<crate::incremental_world_rules_family::LifecycleSupplyBatchV1>>] {
        &self.rows.batches
    }
    pub fn rules_present(&self) -> bool {
        self.present
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
        {
            return Err(bad().into());
        }
        Ok(())
    }
    /// Copy only the actual selected Current proof under fresh Ordinary
    /// admission. All original row/proof fees remain retained by this owner.
    pub fn retain_ordinary_record_v1(
        &self,
        owner: SupplyRootKeyV1<'_>,
        memory: &MemoryBudget,
        cells: &mut u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Option<ReservedValue<SupplyRecordQualificationV1>>, ValueTreeError> {
        if !self.memory.same_pool(memory) {
            return Err(bad());
        }
        tick(cells, 0, &mut cancelled)?;
        let id = match owner {
            SupplyRootKeyV1::Authorization(id) | SupplyRootKeyV1::Batch(id) => id,
        };
        tick(
            cells,
            id.len()
                .checked_mul(usize::BITS as usize + 2)
                .ok_or_else(limit)?,
            &mut cancelled,
        )?;
        let Some(proof) = self.rows.record(owner) else {
            return Ok(None);
        };
        if !proof.uses_budget(memory) {
            return Err(bad());
        }
        let fee = memory.reserve(512, MemoryClass::Ordinary)?;
        let next = fee.retain(SupplyRecordQualificationV1 {
            schema: proof.schema,
            owner: proof.owner,
            root: proof.root.clone(),
            class: MemoryClass::Ordinary,
        });
        tick(cells, 0, &mut cancelled)?;
        Ok(Some(next))
    }
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Supply lifecycle pending closure")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current Supply lifecycle pending work")
}
fn tick(
    cells: &mut u64,
    bytes: usize,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    *cells = cells
        .checked_sub(
            u64::try_from(bytes)
                .map_err(|_| limit())?
                .checked_add(1)
                .ok_or_else(limit)?,
        )
        .ok_or_else(limit)?;
    Ok(())
}
struct Pending {
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
fn recognized(key: &[u8]) -> Result<bool, ValueTreeError> {
    if !key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(false);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    if parsed.name != "world_rules" {
        return Ok(false);
    }
    let expected = WorldFieldKeyV1::new(
        crate::incremental_world_rule_parent::descriptor()?,
        parsed.kind,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if expected.as_bytes() != key || parsed.namespace != StateNamespaceV1::Authorization {
        return Err(bad());
    }
    match parsed.kind {
        WorldFieldRecordKindV1::Metadata => Ok(false),
        WorldFieldRecordKindV1::Entry => Ok(true),
        _ => Err(bad()),
    }
}
/// Uses the original mutable transaction, genuine parent family and explicit
/// continuous allowances. It never fabricates an after RootSet or transition.
#[allow(clippy::too_many_arguments)]
pub fn open_current_supply_lifecycle_directory_v1<M, S, R, E, C>(
    parent: &ReservedValue<SupplyFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    pending_entries: &mut usize,
    max_payload: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentSupplyLifecycleDirectoryV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || !parent.uses_budget(ctx.values.memory)
            || parent.class != MemoryClass::Ordinary
        {
            return Err(bad().into());
        }
        // Account the original RootSet digest's actual bounded string copies
        // before validating/encoding it; no current root is guessed here.
        let root_bytes = roots
            .state
            .iter()
            .try_fold(roots.world_id.len() + 256, |n, r| {
                n.checked_add(r.node_root.len() + 128)
            })
            .ok_or_else(limit)?;
        tick(
            cells,
            root_bytes.checked_mul(4).ok_or_else(limit)?,
            &mut ctx.cancelled,
        )?;
        let _root_fee = ctx
            .values
            .memory
            .reserve(
                root_bytes
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(limit)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let digest = roots.digest().map_err(|_| bad())?;
        parent.validate_binding(&digest)?;
        if digest != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                size_of::<CurrentSupplyLifecycleDirectoryV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let present = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            crate::incremental_world_rule_parent::read_parent_overlay(
                ctx.transaction,
                &mut records,
                &mut ctx.take_state_error,
            )?
            .1
        };
        let mut rows = parent.share_lifecycle(
            ctx.values.memory,
            MemoryClass::Ordinary,
            cells,
            &mut ctx.cancelled,
        )?;
        // Only mutation metadata is buffered, exactly admitted from measured
        // matching keys. Both passes consume the caller's pending allowance.
        let stop = RefCell::new(&mut ctx.cancelled);
        let meter = RefCell::new(&mut *cells);
        let mut count = 0usize;
        let mut bytes = 0usize;
        ctx.transaction.visit_pending_writes(
            StateNamespaceV1::Authorization,
            pending_entries,
            || {
                tick(&mut meter.borrow_mut(), 0, &mut || (stop.borrow_mut())())
                    .map_err(WorldFieldOverlayErrorV1::from)
            },
            |key, _, _| {
                tick(&mut meter.borrow_mut(), key.len(), &mut || {
                    (stop.borrow_mut())()
                })?;
                if recognized(key)? {
                    count = count.checked_add(1).ok_or_else(limit)?;
                    bytes = bytes.checked_add(key.len()).ok_or_else(limit)?;
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?;
        let allocation = count
            .checked_mul(size_of::<Pending>())
            .and_then(|n| n.checked_add(bytes))
            .ok_or_else(limit)?;
        let _pending_fee = ctx
            .values
            .memory
            .reserve(allocation, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut pending = Vec::new();
        pending.try_reserve_exact(count).map_err(|_| limit())?;
        if pending.capacity() > count {
            return Err(limit().into());
        }
        ctx.transaction.visit_pending_writes(
            StateNamespaceV1::Authorization,
            pending_entries,
            || {
                tick(&mut meter.borrow_mut(), 0, &mut || (stop.borrow_mut())())
                    .map_err(WorldFieldOverlayErrorV1::from)
            },
            |key, before, after| {
                tick(&mut meter.borrow_mut(), key.len(), &mut || {
                    (stop.borrow_mut())()
                })?;
                if recognized(key)? {
                    if pending.len() == count {
                        return Err(bad().into());
                    }
                    pending.push(Pending {
                        key: key.to_vec(),
                        before,
                        after,
                    });
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?;
        if pending.len() != count {
            return Err(bad().into());
        }
        drop(stop);
        drop(meter);
        let mut counts = parent.record_counts();
        for mutation in pending {
            tick(cells, mutation.key.len(), &mut ctx.cancelled)?;
            let key =
                WorldFieldKeyV1::from_bytes(&mutation.key).map_err(ValueTreeError::Invalid)?;
            let key = WorldFieldKeyV1::new(
                crate::incremental_world_rule_parent::descriptor()?,
                key.kind,
                key.semantic_key,
            )
            .map_err(ValueTreeError::Invalid)?;
            if ctx
                .transaction
                .read(StateNamespaceV1::Authorization, key.as_bytes())?
                != mutation.after
            {
                return Err(bad().into());
            }
            let mut read = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Authorization),
                state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let old = super::super::receiver::edge(&key, mutation.before, &mut read)?;
            let new = super::super::receiver::edge(&key, mutation.after, &mut read)?;
            if let Some((kind, root, id)) = &old {
                let owner = match kind {
                    SupplyRootKeyV1::Authorization(_) => SupplyRootKeyV1::Authorization(id),
                    SupplyRootKeyV1::Batch(_) => SupplyRootKeyV1::Batch(id),
                };
                parent.validate_lifecycle_record(owner, root, cells)?;
            }
            match (old, new) {
                (None, None) => {}
                (Some(_), None) => {
                    return Err(ValueTreeError::Invalid(
                        "supply whole record deletion lacks relationship proof",
                    )
                    .into());
                }
                (old, Some((kind, after, id))) => {
                    let index = match kind {
                        SupplyRootKeyV1::Authorization(_) => 0,
                        SupplyRootKeyV1::Batch(_) => 1,
                    };
                    if old.as_ref().is_some_and(|(o, _, _)| {
                        matches!(o, SupplyRootKeyV1::Authorization(_)) != (index == 0)
                    }) || old.is_none() && mutation.before.is_some()
                    {
                        return Err(bad().into());
                    }
                    if old.is_none() {
                        counts[index] = counts[index].checked_add(1).ok_or_else(bad)?;
                    }
                    // This is precisely the receiver's authenticated changed
                    // child closure, including insertion's entire new body.
                    super::super::receiver::child(
                        kind,
                        old.as_ref().map(|(_, r, _)| &**r),
                        &after,
                        &mut read,
                        scan,
                        cells,
                        max_payload,
                    )?;
                    let q_fee = read
                        .values
                        .memory
                        .reserve(512, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    let q = q_fee.retain(SupplyRecordQualificationV1 {
                        schema: kind.schema(),
                        owner: WorldFieldKeyV1::from_bytes(key.as_bytes())
                            .map_err(ValueTreeError::Invalid)?
                            .semantic_key
                            .ok_or_else(bad)?,
                        root: (*after).clone(),
                        class: MemoryClass::Ordinary,
                    });
                    tick(cells, id.len(), &mut read.cancelled)?;
                    let _copy = read
                        .values
                        .memory
                        .reserve(id.len() + size_of::<String>(), MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    let mut selected = WorldFieldReadContextV1 {
                        root: &after,
                        state: read.state,
                        values: ValueReadContext {
                            source: read.values.source,
                            memory: read.values.memory,
                            class: MemoryClass::Ordinary,
                            io: &mut *read.values.io,
                        },
                        key_work: &mut *read.key_work,
                        semantic_bytes: read.semantic_bytes,
                        record_bytes: read.record_bytes,
                        take_state_error: &mut read.take_state_error,
                        cancelled: &mut read.cancelled,
                    };
                    rows.insert(
                        (*id).clone(),
                        index,
                        q,
                        &mut selected,
                        scan,
                        cells,
                        max_payload,
                    )?;
                    selected.check()?;
                }
            }
            read.check()?;
        }
        if u64::try_from(rows.authorizations.len()).map_err(|_| limit())? != counts[0]
            || u64::try_from(rows.batches.len()).map_err(|_| limit())? != counts[1]
            || !present && counts != [0, 0]
        {
            return Err(bad().into());
        }
        tick(cells, 0, &mut ctx.cancelled)?;
        let guard = ctx.transaction.guard();
        ctx.transaction.validate_guard(&guard, live)?;
        Ok(fee.retain(CurrentSupplyLifecycleDirectoryV1 {
            rows,
            parent: digest,
            guard,
            memory: ctx.values.memory.clone(),
            present,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
