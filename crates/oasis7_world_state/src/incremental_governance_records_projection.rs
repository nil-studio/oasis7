//! Actual changed Identity/Proposal/Vote records through the original outer/body/cell
//! kernels. No record-root bytes are decoded as CBOR or treated as a DTO.
use super::*;
use crate::{
    incremental_agent_mutation::BodyMutationError,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateEntryV1, StateScanBudget},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{
    cell::RefCell,
    io::{self, Write},
    mem::size_of,
};
struct Change {
    kind: GovernanceRecordKindV1,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
#[derive(Clone, Copy)]
pub enum GovernanceProjectionPathRefV1<'a> {
    Column(GovernanceColumnV1),
    Count(GovernanceCollectionV1, u64),
    Option(u64),
    Snapshot(&'a str),
    Ballot(&'a str),
    Tally(&'a str),
}
enum Path {
    Column(GovernanceColumnV1),
    Count(GovernanceCollectionV1, u64),
    Option(u64),
    Snapshot(ReservedValue<String>),
    Ballot(ReservedValue<String>),
    Tally(ReservedValue<String>),
}
struct Patch {
    path: Path,
    json: Option<ReservedValue<Vec<u8>>>,
}
struct Patches {
    values: Vec<Patch>,
    _slots: MemoryReservation,
}
struct Entity {
    kind: GovernanceRecordKindV1,
    semantic: ReservedValue<Vec<u8>>,
    key: ReservedValue<Vec<u8>>,
    before: bool,
    after: bool,
    patches: Patches,
}
pub struct CurrentGovernanceProjectionDeltaV1 {
    rows: Vec<Entity>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current governance projection")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current governance projection")
}
fn oe(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => BodyMutationError::Execution(e),
        WorldFieldOverlayErrorV1::Value(e) => BodyMutationError::Value(e),
        e => BodyMutationError::Original(Box::new(e)),
    }
}
fn pay(
    work: &mut BusinessDiffProcessingWorkV1,
    n: usize,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), BodyMutationError> {
    if stop() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let n = u64::try_from(n).map_err(|_| limit())?;
    work.steps_remaining = work
        .steps_remaining
        .checked_sub(n.checked_add(1).ok_or_else(limit)?)
        .ok_or_else(limit)?;
    work.bytes_remaining = work.bytes_remaining.checked_sub(n).ok_or_else(limit)?;
    Ok(())
}
fn reserve(
    memory: &MemoryBudget,
    n: usize,
    work: &mut BusinessDiffProcessingWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<MemoryReservation, BodyMutationError> {
    pay(work, n, stop)?;
    memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}
impl CurrentGovernanceProjectionDeltaV1 {
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
    pub fn visit_entities_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(GovernanceRecordKindV1, &[u8], &[u8], bool, bool) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            emit(row.kind, &row.semantic, &row.key, row.before, row.after)?
        }
        Ok(())
    }
    pub fn entity_patch_count_with_work_v1<E>(
        &self,
        kind: GovernanceRecordKindV1,
        semantic: &[u8],
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
    ) -> Result<Option<usize>, E> {
        for row in &self.rows {
            work(1, (row.semantic.len() + semantic.len()) as u64)?;
            if row.kind == kind && row.semantic.as_slice() == semantic {
                return Ok(Some(row.patches.values.len()));
            }
        }
        Ok(None)
    }
    pub fn visit_entity_patches_with_work_v1<E>(
        &self,
        kind: GovernanceRecordKindV1,
        semantic: &[u8],
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(GovernanceProjectionPathRefV1<'_>, Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, (row.semantic.len() + semantic.len()) as u64)?;
            if row.kind != kind || row.semantic.as_slice() != semantic {
                continue;
            }
            for patch in &row.patches.values {
                work(1, 0)?;
                let path = match &patch.path {
                    Path::Column(v) => GovernanceProjectionPathRefV1::Column(*v),
                    Path::Count(g, n) => GovernanceProjectionPathRefV1::Count(*g, *n),
                    Path::Option(v) => GovernanceProjectionPathRefV1::Option(*v),
                    Path::Snapshot(v) => GovernanceProjectionPathRefV1::Snapshot(v),
                    Path::Ballot(v) => GovernanceProjectionPathRefV1::Ballot(v),
                    Path::Tally(v) => GovernanceProjectionPathRefV1::Tally(v),
                };
                emit(path, patch.json.as_deref().map(Vec::as_slice))?
            }
            break;
        }
        Ok(())
    }
    pub fn visit_patches_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(
            GovernanceRecordKindV1,
            &[u8],
            &[u8],
            GovernanceProjectionPathRefV1<'_>,
            Option<&[u8]>,
        ) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            for patch in &row.patches.values {
                work(1, 0)?;
                let path = match &patch.path {
                    Path::Column(v) => GovernanceProjectionPathRefV1::Column(*v),
                    Path::Count(g, n) => GovernanceProjectionPathRefV1::Count(*g, *n),
                    Path::Option(v) => GovernanceProjectionPathRefV1::Option(*v),
                    Path::Snapshot(v) => GovernanceProjectionPathRefV1::Snapshot(v),
                    Path::Ballot(v) => GovernanceProjectionPathRefV1::Ballot(v),
                    Path::Tally(v) => GovernanceProjectionPathRefV1::Tally(v),
                };
                emit(
                    row.kind,
                    &row.semantic,
                    &row.key,
                    path,
                    patch.json.as_deref().map(Vec::as_slice),
                )?
            }
        }
        Ok(())
    }
}
pub fn is_governance_projection_field_v1(name: &str) -> bool {
    GovernanceRecordKindV1::ALL
        .iter()
        .any(|kind| kind.field_name() == name)
}
fn identify(key: &[u8]) -> Result<Option<GovernanceRecordKindV1>, BodyMutationError> {
    if !WorldFieldKeyV1::is_field_key_v1(key) {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    let Some(kind) = GovernanceRecordKindV1::ALL
        .into_iter()
        .find(|kind| kind.field_name() == parsed.name)
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
fn spend(p: &mut BusinessDiffProcessingWorkV1, n: usize) -> Result<(), ValueTreeError> {
    let n = u64::try_from(n)
        .map_err(|_| ValueTreeError::ResourceLimited("governance projection processing"))?;
    p.steps_remaining =
        p.steps_remaining
            .checked_sub(n.checked_add(1).ok_or_else(|| {
                ValueTreeError::ResourceLimited("governance projection processing")
            })?)
            .ok_or(ValueTreeError::ResourceLimited(
                "governance projection steps",
            ))?;
    p.bytes_remaining = p
        .bytes_remaining
        .checked_sub(n)
        .ok_or(ValueTreeError::ResourceLimited(
            "governance projection bytes",
        ))?;
    Ok(())
}
fn text_copy(
    value: &str,
    memory: &MemoryBudget,
    p: &mut BusinessDiffProcessingWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<String>, ValueTreeError> {
    let n = value
        .len()
        .checked_add(size_of::<String>())
        .ok_or(ValueTreeError::ResourceLimited(
            "governance projection key layout",
        ))?;
    spend(p, n)?;
    let fee = memory.reserve(n, MemoryClass::Ordinary)?;
    let mut out = String::with_capacity(value.len());
    if out.capacity() != value.len() {
        return Err(ValueTreeError::ResourceLimited(
            "governance projection key capacity",
        ));
    }
    let mut start = 0;
    while start < value.len() {
        if stop() {
            return Err(ValueTreeError::Cancelled);
        }
        let mut end = (start + 8192).min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1
        }
        spend(p, end - start + 4)?;
        out.push_str(&value[start..end]);
        start = end;
    }
    Ok(fee.retain(out))
}
impl Patches {
    fn new(memory: &MemoryBudget) -> Result<Self, ValueTreeError> {
        Ok(Self {
            values: Vec::new(),
            _slots: memory.reserve(0, MemoryClass::Ordinary)?,
        })
    }
    fn push(
        &mut self,
        patch: Patch,
        memory: &MemoryBudget,
        p: &mut BusinessDiffProcessingWorkV1,
    ) -> Result<(), ValueTreeError> {
        spend(p, 0)?;
        if self.values.len() == self.values.capacity() {
            let cap = self.values.capacity().max(1).checked_mul(2).ok_or(
                ValueTreeError::ResourceLimited("governance projection slots"),
            )?;
            let n = cap
                .checked_mul(size_of::<Patch>())
                .ok_or(ValueTreeError::ResourceLimited(
                    "governance projection slots",
                ))?;
            spend(p, n)?;
            let fee = memory.reserve(n, MemoryClass::Ordinary)?;
            self.values.reserve_exact(cap - self.values.len());
            if self.values.capacity() != cap {
                return Err(ValueTreeError::ResourceLimited(
                    "governance projection slot capacity",
                ));
            }
            self._slots = fee;
        }
        self.values.push(patch);
        Ok(())
    }
}
fn observe<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    cell: GovernanceCellV1<'_>,
    new: Option<&StateEntryV1>,
    after: &oasis7_formal_protocol::incremental_state::StateRootV1,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
    output: &mut Patches,
) -> Result<(), ValueTreeError> {
    let memory = records.values.memory.clone();
    let path = match cell {
        GovernanceCellV1::Record => return Ok(()),
        GovernanceCellV1::Column(v) => Path::Column(v),
        GovernanceCellV1::Count(g) => {
            let entry = new.ok_or(ValueTreeError::Invalid("governance count deleted"))?;
            let key = read::cell_key(kind, cell, records)?;
            let GovernanceValueV1::Unsigned(n) =
                read::value(cell, &entry.value, &key, records, 8, &mut std::io::sink())?
            else {
                return Err(ValueTreeError::Invalid("governance count value"));
            };
            return output.push(
                Patch {
                    path: Path::Count(g, n),
                    json: None,
                },
                &memory,
                p,
            );
        }
        GovernanceCellV1::Option(i) => Path::Option(i),
        GovernanceCellV1::Snapshot(id, _) => {
            Path::Snapshot(text_copy(id, &memory, p, &mut *records.cancelled)?)
        }
        GovernanceCellV1::Ballot(id, _) => {
            Path::Ballot(text_copy(id, &memory, p, &mut *records.cancelled)?)
        }
        GovernanceCellV1::Tally(id) => {
            Path::Tally(text_copy(id, &memory, p, &mut *records.cancelled)?)
        }
    };
    let json = match (&path, new) {
        (_, None) => None,
        (Path::Snapshot(id), Some(_)) => Some(super::projection_cells::member_json(
            kind, true, id, after, records, p, max,
        )?),
        (Path::Ballot(id), Some(_)) => Some(super::projection_cells::member_json(
            kind, false, id, after, records, p, max,
        )?),
        (_, Some(entry)) => Some(super::projection_cells::scalar_json(
            kind, cell, entry, records, p, max,
        )?),
    };
    output.push(Patch { path, json }, &memory, p)
}
fn decode_projection_entity_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    change: Change,
    family: &ReservedValue<GovernanceFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    processing: &mut BusinessDiffProcessingWorkV1,
    max_text: u64,
    live: &ExecutionFence,
) -> Result<Option<Entity>, BodyMutationError> {
    let memory = ctx.values.memory.clone();
    pay(processing, change.key.len(), &mut ctx.cancelled)?;
    if ctx.transaction.read(change.kind.namespace(), &change.key)? != change.after {
        return Err(bad().into());
    }
    let value = change.after.or(change.before).ok_or_else(bad)?;
    let entry = StateEntryV1 {
        namespace: change.kind.namespace(),
        key: change.key,
        value,
    };
    let parent_root = roots
        .state
        .iter()
        .find(|root| root.namespace == change.kind.namespace())
        .ok_or_else(bad)?;
    let mut top = crate::incremental_field_read::WorldFieldReadContextV1 {
        root: parent_root,
        state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
    let _child_fee = reserve(
        &memory,
        2 * size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>() + 512,
        processing,
        &mut top.cancelled,
    )?;
    let _old_key_fee = reserve(&memory, entry.key.len(), processing, &mut top.cancelled)?;
    let selected = super::family::outer(&entry, &mut top)?;
    let before = if let Some(value) = change.before {
        super::family::outer(
            &StateEntryV1 {
                namespace: entry.namespace,
                key: entry.key.clone(),
                value,
            },
            &mut top,
        )?
    } else {
        None
    };
    drop(top);
    let Some((_kind, owner, child)) = selected else {
        if change.after.is_none() {
            return Err(bad().into());
        }
        return Ok(None);
    };
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let expected =
        WorldFieldKeyV1::new(change.kind.descriptor()?, parsed.kind, parsed.semantic_key)
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
    let n = usize::try_from(header.semantic_bytes).map_err(|_| limit())?;
    let semantic_fee = reserve(
        &memory,
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
        processing,
        &mut ctx.cancelled,
    )?;
    let mut semantic = Vec::with_capacity(n);
    if semantic.capacity() != n {
        return Err(limit().into());
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
    pay(processing, n, &mut ctx.cancelled)?;
    let semantic = semantic_fee.retain(semantic);
    let SemanticKeyRefV1::Text(id) =
        read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?
    else {
        return Err(bad().into());
    };
    let key_json =
        crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
            &id,
            &memory,
            processing,
            &mut ctx.cancelled,
        )?;
    let mut patches = Patches::new(&memory)?;
    if change.after.is_some() {
        let mut selected = crate::incremental_field_read::WorldFieldReadContextV1 {
            root: &child,
            state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
            let _proof = qualify_governance_body_v1(
                change.kind,
                owner,
                &mut selected,
                scan,
                cells,
                max_text,
            )?;
        }
        drop(selected);
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let shared = RefCell::new(&mut *processing);
        if let Some((_kind, _owner, before)) = &before {
            super::advance::child_with_work_and_observer(
                change.kind,
                before,
                &child,
                &mut records,
                cells,
                max_text,
                |cell, _old, new, records| {
                    observe(
                        change.kind,
                        cell,
                        new,
                        &child,
                        records,
                        &mut shared.borrow_mut(),
                        max_text,
                        &mut patches,
                    )
                },
                &mut |n| spend(&mut shared.borrow_mut(), n),
            )?;
        } else {
            let empty = oasis7_formal_protocol::incremental_state::StateRootV1::empty(
                change.kind.namespace(),
            )
            .map_err(|_| bad())?;
            crate::incremental_record_diff::visit_with_work(
                &empty,
                &child,
                &mut records,
                cells,
                crate::incremental_record_diff::RecordDiffLabelsV1 {
                    invalid: "governance initial diff",
                    cells: "governance initial paths",
                    stack: "governance initial stack",
                    tree: genesis::LABELS,
                },
                |_old, new, records| {
                    let entry = new.ok_or_else(bad)?;
                    let parsed =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    if parsed.kind == WorldFieldRecordKindV1::Metadata {
                        return Ok(());
                    }
                    key_read::with_cell(
                        change.kind,
                        entry,
                        records,
                        max_text,
                        false,
                        |cell, _key, records| {
                            observe(
                                change.kind,
                                cell,
                                new,
                                &child,
                                records,
                                &mut shared.borrow_mut(),
                                max_text,
                                &mut patches,
                            )
                        },
                    )
                },
                &mut |n| spend(&mut shared.borrow_mut(), n),
            )?;
        }
    }
    Ok(Some(Entity {
        kind: change.kind,
        semantic,
        key: key_json,
        before: change.before.is_some(),
        after: change.after.is_some(),
        patches,
    }))
}
pub fn read_current_governance_projection_delta_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<GovernanceFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    pending: &mut usize,
    processing: &mut BusinessDiffProcessingWorkV1,
    max_text: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentGovernanceProjectionDeltaV1, BodyMutationError> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(&memory)
            || !family.uses_budget(&memory)
            || family.allocation_class() != MemoryClass::Ordinary
        {
            return Err(bad().into());
        }
        family.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
        if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let mut count = 0usize;
        let mut bytes = 0usize;
        for namespace in [GovernanceRecordKindV1::Identity.namespace()] {
            let p = RefCell::new(&mut *processing);
            let stop = RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                namespace,
                pending,
                || pay(&mut p.borrow_mut(), 0, &mut **stop.borrow_mut()),
                |key, before, after| {
                    pay(&mut p.borrow_mut(), key.len(), &mut **stop.borrow_mut())?;
                    if before != after && identify(key)?.is_some() {
                        count = count.checked_add(1).ok_or_else(limit)?;
                        bytes = bytes.checked_add(key.len()).ok_or_else(limit)?
                    }
                    Ok::<_, BodyMutationError>(())
                },
            )?;
        }
        let _phase = reserve(
            &memory,
            count
                .checked_mul(size_of::<Change>())
                .and_then(|n| n.checked_add(bytes))
                .ok_or_else(limit)?,
            processing,
            &mut ctx.cancelled,
        )?;
        let mut changes = Vec::with_capacity(count);
        if changes.capacity() != count {
            return Err(limit().into());
        }
        for namespace in [GovernanceRecordKindV1::Identity.namespace()] {
            let p = RefCell::new(&mut *processing);
            let stop = RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                namespace,
                pending,
                || pay(&mut p.borrow_mut(), 0, &mut **stop.borrow_mut()),
                |key, before, after| {
                    pay(&mut p.borrow_mut(), key.len(), &mut **stop.borrow_mut())?;
                    if before == after {
                        return Ok(());
                    }
                    if let Some(kind) = identify(key)? {
                        if changes.len() == count {
                            return Err(limit().into());
                        }
                        let mut copy = Vec::with_capacity(key.len());
                        if copy.capacity() != key.len() {
                            return Err(limit().into());
                        }
                        copy.extend_from_slice(key);
                        changes.push(Change {
                            kind,
                            key: copy,
                            before,
                            after,
                        })
                    }
                    Ok::<_, BodyMutationError>(())
                },
            )?;
        }
        let fee = reserve(
            &memory,
            count
                .checked_mul(size_of::<Entity>())
                .and_then(|n| n.checked_add(size_of::<CurrentGovernanceProjectionDeltaV1>()))
                .ok_or_else(limit)?,
            processing,
            &mut ctx.cancelled,
        )?;
        let mut rows = Vec::with_capacity(count);
        if rows.capacity() != count {
            return Err(limit().into());
        }
        for change in changes {
            if let Some(row) = decode_projection_entity_v1(
                change, family, roots, state, ctx, scan, cells, processing, max_text, live,
            )? {
                rows.push(row)
            }
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        pay(processing, 0, &mut ctx.cancelled)?;
        let guard = ctx.transaction.guard();
        let output = CurrentGovernanceProjectionDeltaV1 {
            rows,
            guard,
            parent: ctx.transaction.parent_fence().parent_roots,
            memory,
            _fee: fee,
        };
        output.validate(ctx.transaction, ctx.values.memory, live)?;
        Ok(output)
    })();
    let result = match (ctx.take_state_error)() {
        Some(error) => Err(BodyMutationError::Value(error)),
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

#[path = "incremental_governance_records_projection_cold.rs"]
mod cold;
pub use cold::{
    QualifiedGovernanceProjectionColumnsV1, read_qualified_governance_projection_columns_v1,
};
