//! Exact current reward globals/records. Pool maps emit only changed members;
//! mint payloads have seven fixed original fields and retain their Vec ordinal.
use super::*;
use crate::{
    incremental_agent_mutation::BodyMutationError,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateEntryV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, mem::size_of};
struct Change {
    kind: RewardRecordKindV1,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
#[derive(Clone, Copy)]
pub enum RewardPoolProjectionPathRefV1<'a> {
    Scalar(RewardColumnV1, u64),
    Member(RewardCollectionV1, &'a str, Option<u64>),
}
enum Patch {
    Scalar(RewardColumnV1, u64),
    Member(RewardCollectionV1, ReservedValue<String>, Option<u64>),
}
struct Patches {
    values: Vec<Patch>,
    _fee: MemoryReservation,
}
struct Entity {
    index: u64,
    kind: RewardRecordKindV1,
    semantic: ReservedValue<Vec<u8>>,
    key: ReservedValue<Vec<u8>>,
    before: bool,
    after: bool,
    mint: Option<ReservedValue<Vec<u8>>>,
    patches: Patches,
}
pub struct CurrentRewardProjectionDeltaV1 {
    entities: Vec<Entity>,
    before_length: u64,
    after_length: u64,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
fn oe(error: crate::incremental_field_overlay::WorldFieldOverlayErrorV1) -> BodyMutationError {
    use crate::incremental_field_overlay::WorldFieldOverlayErrorV1 as E;
    match error {
        E::Value(e) => e.into(),
        E::Execution(e) => e.into(),
        e => BodyMutationError::Original(Box::new(e)),
    }
}
fn bad() -> ValueTreeError {
    read::invalid()
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current reward projection")
}
fn spend(p: &mut BusinessDiffProcessingWorkV1, n: usize) -> Result<(), ValueTreeError> {
    let n = u64::try_from(n).map_err(|_| limited())?;
    p.steps_remaining = p
        .steps_remaining
        .checked_sub(n.checked_add(1).ok_or_else(limited)?)
        .ok_or_else(limited)?;
    p.bytes_remaining = p.bytes_remaining.checked_sub(n).ok_or_else(limited)?;
    Ok(())
}
fn pay(
    p: &mut BusinessDiffProcessingWorkV1,
    n: usize,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), BodyMutationError> {
    if stop() {
        return Err(ValueTreeError::Cancelled.into());
    }
    spend(p, n)?;
    Ok(())
}
fn reserve(
    memory: &MemoryBudget,
    n: usize,
    p: &mut BusinessDiffProcessingWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<MemoryReservation, BodyMutationError> {
    pay(p, n, stop)?;
    Ok(memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?)
}
impl Patches {
    fn new(memory: &MemoryBudget) -> Result<Self, ValueTreeError> {
        Ok(Self {
            values: Vec::new(),
            _fee: memory.reserve(0, MemoryClass::Ordinary)?,
        })
    }
    fn push(
        &mut self,
        value: Patch,
        memory: &MemoryBudget,
        p: &mut BusinessDiffProcessingWorkV1,
    ) -> Result<(), ValueTreeError> {
        spend(p, 0)?;
        if self.values.len() == self.values.capacity() {
            let cap = self
                .values
                .capacity()
                .max(1)
                .checked_mul(2)
                .ok_or_else(limited)?;
            let n = cap.checked_mul(size_of::<Patch>()).ok_or_else(limited)?;
            spend(p, n)?;
            let fee = memory.reserve(n, MemoryClass::Ordinary)?;
            self.values.reserve_exact(cap - self.values.len());
            if self.values.capacity() != cap {
                return Err(limited());
            }
            self._fee = fee;
        }
        self.values.push(value);
        Ok(())
    }
}
impl CurrentRewardProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn before_mint_length(&self) -> u64 {
        self.before_length
    }
    pub fn after_mint_length(&self) -> u64 {
        self.after_length
    }
    pub fn visit_entities_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(
            RewardRecordKindV1,
            &[u8],
            &[u8],
            bool,
            bool,
            Option<&[u8]>,
        ) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.entities {
            work(1, 0)?;
            emit(
                row.kind,
                &row.semantic,
                &row.key,
                row.before,
                row.after,
                row.mint.as_deref().map(Vec::as_slice),
            )?
        }
        Ok(())
    }
    pub fn visit_pool_patches_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&[u8], &[u8], RewardPoolProjectionPathRefV1<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.entities {
            work(1, 0)?;
            for patch in &row.patches.values {
                work(1, 0)?;
                emit(
                    &row.semantic,
                    &row.key,
                    match patch {
                        Patch::Scalar(c, v) => RewardPoolProjectionPathRefV1::Scalar(*c, *v),
                        Patch::Member(c, key, value) => {
                            RewardPoolProjectionPathRefV1::Member(*c, key, *value)
                        }
                    },
                )?
            }
        }
        Ok(())
    }
}
pub fn is_reward_projection_field_v1(name: &str) -> bool {
    RewardRecordKindV1::ALL
        .into_iter()
        .any(|k| k.name() == name)
}
fn identify(key: &[u8]) -> Result<Option<RewardRecordKindV1>, BodyMutationError> {
    if !WorldFieldKeyV1::is_field_key_v1(key) {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    let Some(kind) = RewardRecordKindV1::ALL
        .into_iter()
        .find(|k| k.name() == parsed.name)
    else {
        return Ok(None);
    };
    let expected = WorldFieldKeyV1::new(kind.descriptor()?, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if expected.as_bytes() != key || parsed.namespace != kind.namespace() {
        return Err(bad().into());
    }
    Ok(Some(kind))
}
fn observe<R: ValueNodeReader, C: FnMut() -> bool>(
    old: Option<&StateEntryV1>,
    new: Option<&StateEntryV1>,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
    out: &mut Patches,
) -> Result<(), ValueTreeError> {
    let entry = new.or(old).ok_or_else(bad)?;
    let kind = RewardRecordKindV1::Pool;
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(());
    }
    let memory = records.values.memory.clone();
    match closure::cell(kind, entry, records, max)? {
        key_read::Parsed::Record | key_read::Parsed::Collection(_) => Ok(()),
        key_read::Parsed::Column(column) => {
            let entry = new.ok_or_else(bad)?;
            let value = super::projection_cells::scalar_value(
                kind,
                RewardCellV1::Column(column),
                entry,
                records,
                p,
            )?;
            out.push(Patch::Scalar(column, value), &memory, p)
        }
        key_read::Parsed::Member(collection) => {
            let id = super::projection_cells::member_id(collection, entry, records, p)?;
            let value = new
                .map(|entry| {
                    super::projection_cells::scalar_value(
                        kind,
                        RewardCellV1::Member(collection, &id),
                        entry,
                        records,
                        p,
                    )
                })
                .transpose()?;
            out.push(Patch::Member(collection, id, value), &memory, p)
        }
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
fn decode_projection_entity_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    change: Change,
    _family: &ReservedValue<RewardFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
) -> Result<Option<Entity>, BodyMutationError> {
    let memory = ctx.values.memory.clone();
    pay(p, change.key.len(), &mut ctx.cancelled)?;
    if ctx.transaction.read(change.kind.namespace(), &change.key)? != change.after {
        return Err(bad().into());
    }
    let value = change.after.or(change.before).ok_or_else(bad)?;
    let _rootfee = reserve(
        &memory,
        2 * size_of::<StateRootV1>() + 512 + change.key.len(),
        p,
        &mut ctx.cancelled,
    )?;
    let entry = StateEntryV1 {
        namespace: change.kind.namespace(),
        key: change.key,
        value,
    };
    let top = roots
        .state
        .iter()
        .find(|r| r.namespace == entry.namespace)
        .ok_or_else(bad)?;
    let mut read = WorldFieldReadContextV1 {
        root: top,
        state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let selected = family::outer(&entry, &mut read)?;
    let before = change
        .before
        .map(|value| {
            family::outer(
                &StateEntryV1 {
                    namespace: entry.namespace,
                    key: entry.key.clone(),
                    value,
                },
                &mut read,
            )
        })
        .transpose()?
        .flatten();
    let (key, child) = match selected {
        Some(family::Outer::Record(key, root)) => (key, root),
        Some(family::Outer::Metadata(_) | family::Outer::Format(_) | family::Outer::Length(_)) => {
            if change.after.is_none() {
                return Err(bad().into());
            }
            return Ok(None);
        }
        None => return Err(bad().into()),
    };
    let before = match before {
        Some(family::Outer::Record(old_key, root)) => {
            if old_key != key {
                return Err(bad().into());
            }
            Some(root)
        }
        None => None,
        _ => return Err(bad().into()),
    };
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let expected = WorldFieldKeyV1::new(key.kind.descriptor()?, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: ctx.semantic_bytes,
        payload_bytes: oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES as u64,
        record_bytes: ctx.record_bytes,
    };
    let header = read_member_record_header_v1(
        &entry.value,
        &expected,
        &mut ctx.values,
        limits,
        &mut ctx.cancelled,
    )?;
    let n = usize::try_from(header.semantic_bytes).map_err(|_| limited())?;
    let keyfee = reserve(
        &memory,
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limited)?,
        p,
        &mut ctx.cancelled,
    )?;
    let mut semantic = Vec::with_capacity(n);
    if semantic.capacity() != n {
        return Err(limited().into());
    }
    read_member_record_key_v1(
        &entry.value,
        &expected,
        &mut ctx.values,
        limits,
        &mut semantic,
        &mut ctx.cancelled,
    )?;
    if semantic.len() != n {
        return Err(bad().into());
    }
    pay(p, n, &mut ctx.cancelled)?;
    let semantic = keyfee.retain(semantic);
    if read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?
        != SemanticKeyRefV1::Unsigned(key.index)
    {
        return Err(bad().into());
    }
    let json_key =
        crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
            &key.index,
            &memory,
            p,
            &mut ctx.cancelled,
        )?;
    let mut patches = Patches::new(&memory)?;
    let mut mint = None;
    if change.after.is_some() {
        let mut selected = WorldFieldReadContextV1 {
            root: &child,
            state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        if before.is_none() {
            let _proof = qualify_reward_body_v1(key, &mut selected, scan, cells, max)?;
        }
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let shared = RefCell::new(&mut *p);
        if let Some(before) = &before {
            super::receiver::child_with_work_and_observer(
                key.kind,
                before,
                &child,
                &mut records,
                cells,
                max,
                |old, new, records| {
                    if key.kind == RewardRecordKindV1::Pool {
                        observe(
                            old,
                            new,
                            records,
                            &mut shared.borrow_mut(),
                            max,
                            &mut patches,
                        )
                    } else {
                        Ok(())
                    }
                },
                &mut |n| spend(&mut shared.borrow_mut(), n),
            )?;
        } else if key.kind == RewardRecordKindV1::Pool {
            let empty = StateRootV1::empty(key.kind.namespace()).map_err(|_| bad())?;
            crate::incremental_record_diff::visit_with_work(
                &empty,
                &child,
                &mut records,
                cells,
                crate::incremental_record_diff::RecordDiffLabelsV1 {
                    invalid: "reward initial projection",
                    cells: "reward initial paths",
                    stack: "reward initial stack",
                    tree: genesis::LABELS,
                },
                |old, new, records| {
                    observe(
                        old,
                        new,
                        records,
                        &mut shared.borrow_mut(),
                        max,
                        &mut patches,
                    )
                },
                &mut |n| spend(&mut shared.borrow_mut(), n),
            )?;
        }
        if key.kind == RewardRecordKindV1::Mint {
            mint = Some(super::projection_cells::mint_json(
                &child,
                &mut records,
                &mut shared.borrow_mut(),
                max,
            )?);
        }
    }
    Ok(Some(Entity {
        index: key.index,
        kind: key.kind,
        semantic,
        key: json_key,
        before: change.before.is_some(),
        after: change.after.is_some(),
        mint,
        patches,
    }))
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn read_current_reward_projection_delta_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<RewardFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    pending: &mut usize,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentRewardProjectionDeltaV1, BodyMutationError> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !family.uses_budget(&memory)
            || family.allocation_class() != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(&memory)
        {
            return Err(bad().into());
        }
        family.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
        if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let mut count = 0usize;
        let mut keybytes = 0usize;
        for ns in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
            let p = RefCell::new(&mut *p);
            let stop = RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                ns,
                pending,
                || pay(&mut p.borrow_mut(), 0, &mut **stop.borrow_mut()),
                |key, before, after| {
                    pay(&mut p.borrow_mut(), key.len(), &mut **stop.borrow_mut())?;
                    if before != after
                        && let Some(kind) = identify(key)?
                    {
                        if kind.namespace() != ns {
                            return Err(bad().into());
                        }
                        count = count.checked_add(1).ok_or_else(limited)?;
                        keybytes = keybytes.checked_add(key.len()).ok_or_else(limited)?
                    }
                    Ok::<_, BodyMutationError>(())
                },
            )?;
        }
        let _phase = reserve(
            &memory,
            count
                .checked_mul(size_of::<Change>())
                .and_then(|n| n.checked_add(keybytes))
                .ok_or_else(limited)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut changes = Vec::with_capacity(count);
        if changes.capacity() != count {
            return Err(limited().into());
        }
        for ns in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
            let p = RefCell::new(&mut *p);
            let stop = RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                ns,
                pending,
                || pay(&mut p.borrow_mut(), 0, &mut **stop.borrow_mut()),
                |key, before, after| {
                    pay(&mut p.borrow_mut(), key.len(), &mut **stop.borrow_mut())?;
                    if before != after
                        && let Some(kind) = identify(key)?
                    {
                        if changes.len() == count {
                            return Err(limited().into());
                        }
                        let mut copy = Vec::with_capacity(key.len());
                        if copy.capacity() != key.len() {
                            return Err(limited().into());
                        }
                        copy.extend_from_slice(key);
                        changes.push(Change {
                            kind,
                            key: copy,
                            before,
                            after,
                        });
                    }
                    Ok::<_, BodyMutationError>(())
                },
            )?;
        }
        let fee = reserve(
            &memory,
            count
                .checked_mul(size_of::<Entity>())
                .and_then(|n| n.checked_add(size_of::<CurrentRewardProjectionDeltaV1>()))
                .ok_or_else(limited)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut entities = Vec::with_capacity(count);
        if entities.capacity() != count {
            return Err(limited().into());
        }
        let before_length = family.mint_length();
        let after_length = if count == 0 {
            before_length
        } else {
            let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            point::metadata(
                RewardRecordKindV1::Mint,
                ctx.transaction,
                &mut records,
                &mut ctx.take_state_error,
            )
            .map_err(oe)?;
            point::length(ctx.transaction, &mut records, &mut ctx.take_state_error)
                .map_err(oe)?
                .value
        };
        let mut ordered = super::receiver::OrderChanges::default();
        for change in changes {
            if let Some(row) =
                decode_projection_entity_v1(change, family, roots, state, ctx, scan, cells, p, max)?
            {
                if row.kind == RewardRecordKindV1::Mint {
                    ordered.observe(row.index, row.before, row.after)?;
                }
                entities.push(row)
            }
        }

        ordered.check(before_length, after_length)?;
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        pay(p, 0, &mut ctx.cancelled)?;
        let out = CurrentRewardProjectionDeltaV1 {
            entities,
            before_length,
            after_length,
            guard: ctx.transaction.guard(),
            parent: ctx.transaction.parent_fence().parent_roots,
            memory,
            _fee: fee,
        };
        out.validate(ctx.transaction, ctx.values.memory, live)?;
        Ok(out)
    })();
    let result = match (ctx.take_state_error)() {
        Some(e) => Err(e.into()),
        None => result,
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

#[path = "incremental_reward_budget_records_projection_cold.rs"]
mod cold;
pub use cold::{QualifiedRewardProjectionColumnsV1, read_qualified_reward_projection_columns_v1};
