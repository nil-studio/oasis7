//! Readonly projection patches from original marketplace member codecs and
//! original graph validators. No execution or acceptance authority is minted.
use super::*;
use crate::incremental_module_marketplace::{
    ModuleMarketplaceFamilyQualificationV1, ModuleMarketplaceFieldV1 as Kind,
};
use crate::{
    incremental_agent_mutation::BodyMutationError,
    incremental_field_member::{
        MemberReadProfileV1, read_member_record_header_v1, read_member_record_profile_v1,
    },
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::{StateScanBudget, StateScanContext},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::mem::size_of;
struct Change {
    kind: Kind,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
enum Patch {
    Scalar(&'static str, Option<ReservedValue<Vec<u8>>>),
    Bid(u64, &'static str, Option<ReservedValue<Vec<u8>>>),
    Node(u64, Option<[u64; 7]>),
}
struct Entity {
    kind: Kind,
    semantic: ReservedValue<Vec<u8>>,
    key: ReservedValue<Vec<u8>>,
    before: bool,
    after: bool,
    meta: Option<[u64; 3]>,
    patches: Vec<Patch>,
    _patches: MemoryReservation,
}
pub enum MarketplaceProjectionPatchRefV1<'a> {
    Scalar(&'static str, Option<&'a [u8]>),
    Bid(u64, &'static str, Option<&'a [u8]>),
    Node(u64, Option<[u64; 7]>),
}
pub struct CurrentMarketplaceProjectionDeltaV1 {
    entities: Vec<Entity>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("actual marketplace projection")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("actual marketplace projection")
}
fn pay(p: &mut BusinessDiffProcessingWorkV1, n: usize) -> Result<(), ValueTreeError> {
    let n = u64::try_from(n).map_err(|_| limited())?;
    p.steps_remaining = p
        .steps_remaining
        .checked_sub(n.checked_add(1).ok_or_else(limited)?)
        .ok_or_else(limited)?;
    p.bytes_remaining = p.bytes_remaining.checked_sub(n).ok_or_else(limited)?;
    Ok(())
}
fn reserve(
    memory: &MemoryBudget,
    p: &mut BusinessDiffProcessingWorkV1,
    n: usize,
) -> Result<MemoryReservation, ValueTreeError> {
    pay(p, n)?;
    Ok(memory.reserve(n, MemoryClass::Ordinary)?)
}
fn owner(error: WorldStateOwnerError) -> BodyMutationError {
    match error {
        WorldStateOwnerError::Value(e) => e.into(),
        WorldStateOwnerError::Memory(e) => ValueTreeError::Memory(e).into(),
        WorldStateOwnerError::RootSet(e) => e.into(),
        WorldStateOwnerError::Invalid(e) => ValueTreeError::Invalid(e).into(),
        WorldStateOwnerError::ResourceLimited(e) => ValueTreeError::ResourceLimited(e).into(),
        e => BodyMutationError::Original(Box::new(e)),
    }
}
fn json<T: serde::Serialize + ?Sized, R: ValueNodeReader, C: FnMut() -> bool>(
    value: &T,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let memory = records.values.memory.clone();
    crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
        &value,
        &memory,
        p,
        &mut *records.cancelled,
    )
}
fn text<R: ValueNodeReader, C: FnMut() -> bool>(
    entry: &StateEntryV1,
    field: FieldDescriptor,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
) -> Result<ReservedValue<String>, ValueTreeError> {
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: max,
        record_bytes: records.record_bytes,
    };
    pay(p, 0)?;
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut *records.cancelled,
    )?;
    let n = usize::try_from(info.payload_bytes).map_err(|_| limited())?;
    let fee = reserve(
        records.values.memory,
        p,
        n.checked_add(size_of::<String>()).ok_or_else(limited)?,
    )?;
    let mut out = Vec::with_capacity(n);
    if out.capacity() != n {
        return Err(limited());
    }
    struct Out<'a> {
        value: &'a mut Vec<u8>,
        max: usize,
    }
    impl Write for Out<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self
                .value
                .len()
                .checked_add(b.len())
                .filter(|n| *n <= self.max)
                .is_none()
            {
                return Err(io::Error::other("marketplace text output bound"));
            }
            self.value.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    read_member_record_profile_v1(
        &entry.value,
        &key,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            semantic_bytes: info.semantic_bytes,
            limits,
        },
        &mut io::sink(),
        &mut Out {
            value: &mut out,
            max: n,
        },
        &mut *records.cancelled,
    )?;
    if out.len() != n {
        return Err(invalid());
    }
    pay(p, n)?;
    let result = String::from_utf8(out);
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(fee.retain(result.map_err(|_| invalid())?))
}
fn push(
    out: &mut Vec<Patch>,
    fee: &mut MemoryReservation,
    value: Patch,
    records: &mut InventoryValueReader<'_, '_, impl ValueNodeReader, impl FnMut() -> bool>,
    p: &mut BusinessDiffProcessingWorkV1,
) -> Result<(), ValueTreeError> {
    pay(p, 0)?;
    if out.len() == out.capacity() {
        let cap = out.capacity().max(1).checked_mul(2).ok_or_else(limited)?;
        let next = reserve(
            records.values.memory,
            p,
            cap.checked_mul(size_of::<Patch>()).ok_or_else(limited)?,
        )?;
        out.reserve_exact(cap - out.len());
        if out.capacity() != cap {
            return Err(limited());
        }
        *fee = next;
    }
    out.push(value);
    Ok(())
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
fn observe<R: ValueNodeReader, C: FnMut() -> bool>(
    old: Option<&StateEntryV1>,
    new: Option<&StateEntryV1>,
    field: FieldDescriptor,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
    out: &mut Vec<Patch>,
    fee: &mut MemoryReservation,
) -> Result<(), BodyMutationError> {
    let entry = new.or(old).ok_or_else(invalid)?;
    pay(p, entry.key.len())?;
    let (semantic, before_value) =
        read_child_member(entry, field, records.values, max, records.cancelled).map_err(owner)?;
    let value = if new.is_some() {
        Some(match before_value {
            ChildValue::Unit => json(&true, records, p)?,
            ChildValue::Bool(v) => json(&v, records, p)?,
            ChildValue::U64(v) => json(&v, records, p)?,
            ChildValue::I64(v) => json(&v, records, p)?,
            ChildValue::Text => {
                let value = text(entry, field, records, p, max)?;
                json(&*value, records, p)?
            }
            ChildValue::BidIndexNode(_)
            | ChildValue::BidGroupRoot(_)
            | ChildValue::TextPairDigest(_) => {
                return match semantic {
                    ChildSemantic::Bid {
                        bid_id,
                        field: "index_node",
                    } => {
                        let ChildValue::BidIndexNode(v) = before_value else {
                            return Err(invalid().into());
                        };
                        push(out, fee, Patch::Node(bid_id, Some(v.fields())), records, p)
                            .map_err(Into::into)
                    }
                    ChildSemantic::BidGroup(_) => Ok(()),
                    _ => Err(invalid().into()),
                };
            }
        })
    } else {
        None
    };
    match semantic {
        ChildSemantic::Field(name) => {
            if !matches!(name, "length" | "sequence_root_id" | "next_bid_id") {
                push(out, fee, Patch::Scalar(name, value), records, p)?;
            }
        }
        ChildSemantic::Bid {
            bid_id,
            field: name,
        } => {
            if name == "index_node" {
                push(out, fee, Patch::Node(bid_id, None), records, p)?;
            } else {
                push(out, fee, Patch::Bid(bid_id, name, value), records, p)?;
            }
        }
        ChildSemantic::BidGroup(_) => (),
    };
    Ok(())
}
impl CurrentMarketplaceProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
            || !tx.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn visit_entities_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(Kind, &[u8], &[u8], bool, bool, Option<[u64; 3]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.entities {
            work(1, 0)?;
            emit(
                row.kind,
                &row.semantic,
                &row.key,
                row.before,
                row.after,
                row.meta,
            )?
        }
        Ok(())
    }
    pub fn visit_patches_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(Kind, &[u8], &[u8], MarketplaceProjectionPatchRefV1<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.entities {
            work(1, 0)?;
            for patch in &row.patches {
                work(1, 0)?;
                emit(
                    row.kind,
                    &row.semantic,
                    &row.key,
                    match patch {
                        Patch::Scalar(n, v) => MarketplaceProjectionPatchRefV1::Scalar(
                            n,
                            v.as_deref().map(Vec::as_slice),
                        ),
                        Patch::Bid(id, n, v) => MarketplaceProjectionPatchRefV1::Bid(
                            *id,
                            n,
                            v.as_deref().map(Vec::as_slice),
                        ),
                        Patch::Node(id, v) => MarketplaceProjectionPatchRefV1::Node(*id, *v),
                    },
                )?;
            }
        }
        Ok(())
    }
}
fn identify(raw: &[u8]) -> Result<Option<Kind>, BodyMutationError> {
    if !WorldFieldKeyV1::is_field_key_v1(raw) {
        return Ok(None);
    }
    let p = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    if p.owner != FieldOwner::WorldState {
        return Ok(None);
    }
    let Some(kind) = Kind::from_name(p.name) else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(
        kind.descriptor().map_err(|_| invalid())?,
        p.kind,
        p.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != raw || p.namespace != StateNamespaceV1::Business {
        return Err(invalid().into());
    }
    Ok(Some(kind))
}
fn scan_error(
    error: oasis7_formal_protocol::incremental_state::StateScanError<BodyMutationError>,
) -> BodyMutationError {
    use oasis7_formal_protocol::incremental_state::StateScanError as E;
    match error {
        E::Visitor(e) => e,
        E::Memory(e) => ValueTreeError::Memory(e).into(),
        E::ResourceLimited => limited().into(),
        E::Cancelled => ValueTreeError::Cancelled.into(),
        E::Invalid(e) => ValueTreeError::Invalid(e).into(),
        E::Tree(e) => BodyMutationError::Original(Box::new(io::Error::other(e))),
    }
}

fn decode_projection_entity_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    change: Change,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
) -> Result<Entity, BodyMutationError> {
    let memory = ctx.values.memory.clone();
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    pay(p, change.key.len())?;
    if ctx
        .transaction
        .read(StateNamespaceV1::Business, &change.key)?
        != change.after
    {
        return Err(invalid().into());
    }
    let field = change.kind.descriptor().map_err(|_| invalid())?;
    let parsed = WorldFieldKeyV1::from_bytes(&change.key).map_err(ValueTreeError::Invalid)?;
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        return Err(invalid().into());
    }
    if parsed.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid().into());
    }
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let _roots = reserve(
        &memory,
        p,
        2 * size_of::<StateRootV1>() + 512 + change.key.len(),
    )?;
    let value = change.after.or(change.before).ok_or_else(invalid)?;
    let entry = StateEntryV1 {
        namespace: StateNamespaceV1::Business,
        key: change.key,
        value,
    };
    let selected = bridge::child_root(
        field,
        &entry,
        &mut ctx.values,
        ctx.semantic_bytes,
        max,
        &mut ctx.cancelled,
    )
    .map_err(owner)?;
    let before = if let Some(value) = change.before {
        let old = StateEntryV1 {
            namespace: entry.namespace,
            key: entry.key.clone(),
            value,
        };
        Some(
            bridge::child_root(
                field,
                &old,
                &mut ctx.values,
                ctx.semantic_bytes,
                max,
                &mut ctx.cancelled,
            )
            .map_err(owner)?,
        )
    } else {
        None
    };
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: ctx.semantic_bytes,
        payload_bytes: RECORD_ROOT_BYTES as u64,
        record_bytes: ctx.record_bytes,
    };
    pay(p, 0)?;
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        limits,
        &mut ctx.cancelled,
    )?;
    let n = usize::try_from(info.semantic_bytes).map_err(|_| limited())?;
    let sf = reserve(
        &memory,
        p,
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limited)?,
    )?;
    let mut semantic = Vec::with_capacity(n);
    if semantic.capacity() != n {
        return Err(limited().into());
    }
    crate::incremental_field_member::read_member_record_key_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        limits,
        &mut semantic,
        &mut ctx.cancelled,
    )?;
    if semantic.len() != n {
        return Err(invalid().into());
    }
    pay(p, n)?;
    let semantic = sf.retain(semantic);
    let SemanticKeyRefV1::Text(module) =
        read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?
    else {
        return Err(invalid().into());
    };
    let json_key =
        crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
            &module,
            &memory,
            p,
            &mut ctx.cancelled,
        )?;
    let mut patches = Vec::new();
    let mut patchfee = memory
        .reserve(0, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut meta = None;
    if change.after.is_some() {
        let stop = RefCell::new(&mut ctx.cancelled);
        let shared = RefCell::new(&mut *p);
        StateScanContext {
            source: state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: scan,
        }
        .with_reader(
            || (stop.borrow_mut())(),
            |reader| {
                let mut cancelled = || (stop.borrow_mut())();
                let mut records = InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut cancelled,
                };
                if let Some(before) = &before {
                    match change.kind {
                        Kind::ArtifactListings => bridge::listing_delta(
                            field,
                            before,
                            &selected,
                            &mut records,
                            cells,
                            max,
                        ),
                        Kind::ArtifactBids => bridge::bids_delta(
                            field,
                            before,
                            &selected,
                            reader,
                            &mut records,
                            cells,
                            &mut shared.borrow_mut().steps_remaining,
                            max,
                        ),
                        Kind::VisualEntities => bridge::visual_delta(
                            field,
                            before,
                            &selected,
                            reader,
                            &mut records,
                            cells,
                            max,
                        ),
                    }
                    .map_err(owner)?;
                } else {
                    bridge::full_child(
                        field,
                        &selected,
                        reader,
                        records.values,
                        records.key_work,
                        max,
                        records.cancelled,
                    )
                    .map_err(owner)?;
                }
                if change.kind == Kind::ArtifactBids {
                    use crate::incremental_module_marketplace::bid_index;
                    let mut scalar = |name: &str| -> Result<u64, BodyMutationError> {
                        pay(&mut shared.borrow_mut(), 0)?;
                        Ok(bid_index::read_u64_member(
                            &selected,
                            field,
                            name,
                            records.values,
                            records.key_work,
                            records.semantic_bytes,
                            max,
                            records.cancelled,
                        )?
                        .0)
                    };
                    meta = Some([
                        scalar("length")?,
                        scalar("sequence_root_id")?,
                        scalar("next_bid_id")?,
                    ]);
                }
                let empty =
                    StateRootV1::empty(StateNamespaceV1::Business).map_err(|_| invalid())?;
                let old = before.as_deref().unwrap_or(&empty);
                let mut original = None;
                let result = incremental_record_diff::visit_with_work(
                    old,
                    &selected,
                    &mut records,
                    cells,
                    bridge::LABELS,
                    |old, new, records| {
                        if let Err(error) = observe(
                            old,
                            new,
                            field,
                            records,
                            &mut shared.borrow_mut(),
                            max,
                            &mut patches,
                            &mut patchfee,
                        ) {
                            original = Some(error);
                            return Err(invalid());
                        }
                        Ok(())
                    },
                    &mut |n| pay(&mut shared.borrow_mut(), n),
                );
                if let Some(error) = original {
                    return Err(error);
                }
                result.map_err(BodyMutationError::Value)?;
                Ok::<_, BodyMutationError>(())
            },
        )
        .map_err(scan_error)?;
    }
    Ok(Entity {
        kind: change.kind,
        semantic,
        key: json_key,
        before: change.before.is_some(),
        after: change.after.is_some(),
        meta,
        patches,
        _patches: patchfee,
    })
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn read_current_marketplace_projection_delta_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
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
) -> Result<CurrentMarketplaceProjectionDeltaV1, BodyMutationError> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !family.uses_budget(&memory)
            || family.allocation_class() != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(&memory)
        {
            return Err(invalid().into());
        }
        family.validate_binding(roots).map_err(owner)?;
        if roots.digest()? != ctx.transaction.parent_fence().parent_roots {
            return Err(invalid().into());
        }
        let mut count = 0usize;
        let mut bytes = 0usize;
        {
            let p = RefCell::new(&mut *p);
            let stop = RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                StateNamespaceV1::Business,
                pending,
                || {
                    if (stop.borrow_mut())() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    pay(&mut p.borrow_mut(), 0).map_err(Into::into)
                },
                |key, before, after| {
                    if (stop.borrow_mut())() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    pay(&mut p.borrow_mut(), key.len())?;
                    if before != after && identify(key)?.is_some() {
                        count = count.checked_add(1).ok_or_else(limited)?;
                        bytes = bytes.checked_add(key.len()).ok_or_else(limited)?;
                    }
                    Ok::<_, BodyMutationError>(())
                },
            )?;
        }
        let _changes = reserve(
            &memory,
            p,
            count
                .checked_mul(size_of::<Change>())
                .and_then(|n| n.checked_add(bytes))
                .ok_or_else(limited)?,
        )?;
        let mut changes = Vec::with_capacity(count);
        if changes.capacity() != count {
            return Err(limited().into());
        }
        {
            let p = RefCell::new(&mut *p);
            let stop = RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                StateNamespaceV1::Business,
                pending,
                || {
                    if (stop.borrow_mut())() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    pay(&mut p.borrow_mut(), 0).map_err(Into::into)
                },
                |key, before, after| {
                    if (stop.borrow_mut())() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    pay(&mut p.borrow_mut(), key.len())?;
                    if before != after
                        && let Some(kind) = identify(key)?
                    {
                        if changes.len() == count {
                            return Err(limited().into());
                        }
                        let mut owned = Vec::with_capacity(key.len());
                        if owned.capacity() != key.len() {
                            return Err(limited().into());
                        }
                        owned.extend_from_slice(key);
                        changes.push(Change {
                            kind,
                            key: owned,
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
            p,
            count
                .checked_mul(size_of::<Entity>())
                .and_then(|n| n.checked_add(size_of::<CurrentMarketplaceProjectionDeltaV1>()))
                .ok_or_else(limited)?,
        )?;
        let mut entities = Vec::with_capacity(count);
        if entities.capacity() != count {
            return Err(limited().into());
        }
        for change in changes {
            entities.push(decode_projection_entity_v1(
                change, state, ctx, scan, cells, p, max,
            )?);
        }

        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        pay(p, 0)?;
        Ok(CurrentMarketplaceProjectionDeltaV1 {
            entities,
            guard: ctx.transaction.guard(),
            parent: ctx.transaction.parent_fence().parent_roots,
            memory,
            _fee: fee,
        })
    })();
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    result
}
impl CurrentMarketplaceProjectionDeltaV1 {
    pub fn visit_existing_visual_patches_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&[u8], &[u8], &'static str, Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.entities {
            work(1, 0)?;
            if row.kind != Kind::VisualEntities || !row.before || !row.after {
                continue;
            }
            for patch in &row.patches {
                work(1, 0)?;
                if let Patch::Scalar(name, value) = patch {
                    emit(
                        &row.semantic,
                        &row.key,
                        name,
                        value.as_deref().map(Vec::as_slice),
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[path = "incremental_owner_marketplace_projection_cold.rs"]
mod cold;
pub use cold::{
    QualifiedMarketplaceProjectionColumnsV1, read_qualified_marketplace_projection_columns_v1,
};
