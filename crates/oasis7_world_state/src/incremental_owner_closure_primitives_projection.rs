//! Actual primitive-family net changes to original typed JSON fragments.
//! Field ownership, key grammar and payload profiles remain the original
//! primitive closure kernels; metadata never becomes a formatting value.
use super::*;
use crate::{
    incremental_field_member::{read_member_record_header_v1, read_member_record_key_v1},
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use std::mem::size_of;
struct Change {
    namespace: StateNamespaceV1,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
struct Row {
    field: &'static str,
    semantic: ReservedValue<Vec<u8>>,
    key_json: ReservedValue<Vec<u8>>,
    value_json: Option<ReservedValue<Vec<u8>>>,
}
pub struct CurrentPrimitiveProjectionDeltaV1 {
    rows: Vec<Row>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current primitive projection delta")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current primitive projection work")
}
fn pay(
    work: &mut BusinessDiffProcessingWorkV1,
    bytes: usize,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    let bytes = u64::try_from(bytes).map_err(|_| limit())?;
    let steps = bytes.checked_add(1).ok_or_else(limit)?;
    let before = (work.steps_remaining, work.bytes_remaining);
    work.steps_remaining = work.steps_remaining.saturating_sub(steps);
    work.bytes_remaining = work.bytes_remaining.saturating_sub(bytes);
    if before.0 < steps || before.1 < bytes {
        return Err(limit());
    }
    Ok(())
}
fn json<T: serde::Serialize>(
    value: &T,
    memory: &MemoryBudget,
    work: &mut BusinessDiffProcessingWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
        value, memory, work, stop,
    )
}
pub fn is_primitive_projection_field_v1(field: FieldDescriptor) -> bool {
    supports(field)
}
impl CurrentPrimitiveProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self._fee.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit<E: From<ValueTreeError>>(
        &self,
        processing: &mut BusinessDiffProcessingWorkV1,
        mut stop: impl FnMut() -> bool,
        visit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        self.visit_with_work_v1(|_, _| pay(processing, 0, &mut stop).map_err(E::from), visit)
    }
    /// The same original DATA emission with continuing caller work. Neither
    /// callback can replace a field, source guard or decode predicate.
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut visit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            visit(
                row.field,
                &row.semantic,
                &row.key_json,
                row.value_json.as_deref().map(Vec::as_slice),
            )?;
        }
        Ok(())
    }
}
fn owner_error(error: WorldStateOwnerError) -> WorldFieldOverlayErrorV1 {
    match error {
        WorldStateOwnerError::Value(error) => error.into(),
        WorldStateOwnerError::Memory(error) => ValueTreeError::Memory(error).into(),
        WorldStateOwnerError::RootSet(error) => error.into(),
        WorldStateOwnerError::Invalid(note) => ValueTreeError::Invalid(note).into(),
        WorldStateOwnerError::ResourceLimited(note) => ValueTreeError::ResourceLimited(note).into(),
        _ => bad().into(),
    }
}
fn copy_capacity(
    memory: &MemoryBudget,
    n: usize,
) -> Result<(Vec<u8>, MemoryReservation), ValueTreeError> {
    let fee = memory.reserve(
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let out = Vec::with_capacity(n);
    if out.capacity() != n {
        return Err(limit());
    }
    Ok((out, fee))
}
struct PrimitiveReadContextV1<'a, 'v, R, C> {
    values: &'a mut oasis7_formal_protocol::incremental_value::ValueReadContext<'v, R>,
    key_work: &'a mut crate::incremental_semantic_key::SemanticKeyBudget,
    semantic_bytes: u64,
    record_bytes: u64,
    cancelled: &'a mut C,
}
enum PrimitiveSelection<'a> {
    Current(&'a Change),
    Qualified(&'a oasis7_formal_protocol::incremental_state::StateEntryV1),
}
impl PrimitiveSelection<'_> {
    fn namespace(&self) -> StateNamespaceV1 {
        match self {
            Self::Current(c) => c.namespace,
            Self::Qualified(e) => e.namespace,
        }
    }
    fn key(&self) -> &[u8] {
        match self {
            Self::Current(c) => &c.key,
            Self::Qualified(e) => &e.key,
        }
    }
    fn root(&self) -> Result<[u8; 32], ValueTreeError> {
        match self {
            Self::Current(c) => c.after.or(c.before).ok_or_else(bad),
            Self::Qualified(e) => Ok(e.value),
        }
    }
    fn present_root(&self) -> Option<[u8; 32]> {
        match self {
            Self::Current(c) => c.after,
            Self::Qualified(e) => Some(e.value),
        }
    }
}
fn one<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    change: &Change,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    family: &mut WorldPrimitiveFamilyWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<Option<Row>, WorldFieldOverlayErrorV1> {
    let mut read = PrimitiveReadContextV1 {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    one_selection(
        PrimitiveSelection::Current(change),
        roots,
        &mut read,
        family,
        processing,
    )
}
fn one_selection<R: ValueNodeReader, C: FnMut() -> bool>(
    change: PrimitiveSelection<'_>,
    roots: &RootSetV1,
    ctx: &mut PrimitiveReadContextV1<'_, '_, R, C>,
    family: &mut WorldPrimitiveFamilyWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<Option<Row>, WorldFieldOverlayErrorV1> {
    let Some((_, field, key)) = identify(change.namespace(), change.key()).map_err(owner_error)?
    else {
        return Err(bad().into());
    };
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    spend(family).map_err(owner_error)?;
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        match change {
            PrimitiveSelection::Current(change) => {
                if change.before.is_none() || change.before != change.after {
                    return Err(bad().into());
                }
            }
            PrimitiveSelection::Qualified(entry) => {
                metadata(&key, &entry.value, ctx.values, &mut ctx.cancelled).map_err(owner_error)?
            }
        }
        return Ok(None);
    }
    let root = change.root()?;
    if let Some(after) = change.present_root() {
        member(
            field,
            &key,
            &after,
            roots.state_root(change.namespace()),
            ctx.values,
            family,
            &mut ctx.cancelled,
        )
        .map_err(owner_error)?
    }
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: ctx.semantic_bytes,
        payload_bytes: ctx.record_bytes,
        record_bytes: ctx.record_bytes,
    };
    let header = read_member_record_header_v1(&root, &key, ctx.values, limits, &mut ctx.cancelled)?;
    let n = usize::try_from(header.semantic_bytes).map_err(|_| limit())?;
    pay(
        processing,
        n.checked_mul(16).ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    let (mut semantic, semantic_fee) = copy_capacity(ctx.values.memory, n)?;
    read_member_record_key_v1(
        &root,
        &key,
        ctx.values,
        limits,
        &mut semantic,
        &mut ctx.cancelled,
    )?;
    if semantic.len() != n {
        return Err(bad().into());
    }
    let semantic = semantic_fee.retain(semantic);
    let original = read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?;
    let key_json = match original {
        SemanticKeyRefV1::Unit => json(&(), ctx.values.memory, processing, &mut ctx.cancelled)?,
        SemanticKeyRefV1::Text(text) => {
            json(&text, ctx.values.memory, processing, &mut ctx.cancelled)?
        }
        SemanticKeyRefV1::Unsigned(number) => {
            json(&number, ctx.values.memory, processing, &mut ctx.cancelled)?
        }
        _ => return Err(bad().into()),
    };
    let value_json = if change.present_root().is_none() {
        if u64_field(field).is_some() {
            return Err(bad().into());
        }
        None
    } else if id_set_field(field).is_some() {
        Some(match original {
            SemanticKeyRefV1::Text(text) => {
                json(&text, ctx.values.memory, processing, &mut ctx.cancelled)?
            }
            SemanticKeyRefV1::Unsigned(number) => {
                json(&number, ctx.values.memory, processing, &mut ctx.cancelled)?
            }
            _ => return Err(bad().into()),
        })
    } else {
        let mut reader = InventoryValueReader {
            values: ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let result = match header.codec {
            MemberPayloadCodecV1::U64 => {
                let value = reader.unsigned(&root, &key, header.semantic_bytes)?;
                json(&value, reader.values.memory, processing, reader.cancelled)?
            }
            MemberPayloadCodecV1::I64 => {
                let value = reader.quantity(&root, &key, header.semantic_bytes)?;
                json(&value, reader.values.memory, processing, reader.cancelled)?
            }
            MemberPayloadCodecV1::Unit => {
                let _: [u8; 0] = reader.fixed(
                    &root,
                    &key,
                    header.semantic_bytes,
                    MemberPayloadCodecV1::Unit,
                )?;
                json(
                    &Option::<&str>::None,
                    reader.values.memory,
                    processing,
                    reader.cancelled,
                )?
            }
            MemberPayloadCodecV1::Text => {
                let n = usize::try_from(header.payload_bytes).map_err(|_| limit())?;
                pay(
                    processing,
                    n.checked_mul(2).ok_or_else(limit)?,
                    reader.cancelled,
                )?;
                let (mut bytes, _fee) = copy_capacity(reader.values.memory, n)?;
                read_member_record_profile_v1(
                    &root,
                    &key,
                    reader.values,
                    MemberReadProfileV1 {
                        codec: MemberPayloadCodecV1::Text,
                        semantic_bytes: header.semantic_bytes,
                        limits,
                    },
                    &mut io::sink(),
                    &mut bytes,
                    &mut *reader.cancelled,
                )?;
                let value = std::str::from_utf8(&bytes).map_err(|_| bad())?;
                json(&value, reader.values.memory, processing, reader.cancelled)?
            }
            MemberPayloadCodecV1::IdentityU64Triple => {
                let n = usize::try_from(header.payload_bytes).map_err(|_| limit())?;
                pay(
                    processing,
                    n.checked_mul(2).ok_or_else(limit)?,
                    reader.cancelled,
                )?;
                let (mut bytes, _fee) = copy_capacity(reader.values.memory, n)?;
                let (_, _, amounts) =
                    crate::incremental_identity_amounts::read_identity_amounts_record_v1(
                        &root,
                        &key,
                        reader.values,
                        limits,
                        &mut io::sink(),
                        &mut bytes,
                        reader.cancelled,
                    )?;
                let identity = String::from_utf8(bytes).map_err(|_| bad())?;
                if field.name == "main_token_balances" {
                    json(
                        &crate::MainTokenAccountBalance {
                            account_id: identity,
                            liquid_balance: amounts[0],
                            vested_balance: amounts[1],
                            restricted_starter_claim_balance: amounts[2],
                        },
                        reader.values.memory,
                        processing,
                        reader.cancelled,
                    )?
                } else if field.name == "node_asset_balances" {
                    json(
                        &crate::NodeAssetBalance {
                            node_id: identity,
                            power_credit_balance: amounts[0],
                            total_minted_credits: amounts[1],
                            total_burned_credits: amounts[2],
                        },
                        reader.values.memory,
                        processing,
                        reader.cancelled,
                    )?
                } else {
                    return Err(bad().into());
                }
            }
            _ => return Err(bad().into()),
        };
        Some(result)
    };
    Ok(Some(Row {
        field: field.name,
        semantic,
        key_json,
        value_json,
    }))
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_primitive_projection_delta_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldPrimitiveQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    family: &mut WorldPrimitiveFamilyWorkV1,
    pending: &mut usize,
    processing: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentPrimitiveProjectionDeltaV1, WorldFieldOverlayErrorV1> {
    ctx.transaction.validate_guard(guard, live)?;
    let memory = ctx.values.memory.clone();
    if ctx.values.class != MemoryClass::Ordinary
        || !ctx.transaction.uses_budget(&memory)
        || !parent.uses_budget(&memory)
    {
        return Err(bad().into());
    }
    pay(
        processing,
        roots
            .state
            .len()
            .checked_mul(512)
            .and_then(|n| n.checked_add(roots.world_id.len()))
            .ok_or_else(limit)?,
        &mut ctx.cancelled,
    )?;
    for root in &roots.state {
        pay(processing, root.node_root.len(), &mut ctx.cancelled)?
    }
    parent.validate_binding(roots).map_err(owner_error)?;
    if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
        return Err(bad().into());
    }
    let mut count = 0usize;
    let mut key_bytes = 0usize;
    for root in &roots.state {
        let stop = RefCell::new(&mut ctx.cancelled);
        let bank = RefCell::new(&mut *processing);
        ctx.transaction.visit_pending_writes(
            root.namespace,
            pending,
            || {
                pay(&mut bank.borrow_mut(), 0, &mut || (stop.borrow_mut())())
                    .map_err(WorldFieldOverlayErrorV1::from)
            },
            |key, before, after| {
                pay(&mut bank.borrow_mut(), key.len(), &mut || {
                    (stop.borrow_mut())()
                })?;
                if before != after
                    && identify(root.namespace, key)
                        .map_err(owner_error)?
                        .is_some()
                {
                    count = count.checked_add(1).ok_or_else(limit)?;
                    key_bytes = key_bytes.checked_add(key.len()).ok_or_else(limit)?
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?
    }
    let phase_bytes = count
        .checked_mul(size_of::<Change>())
        .and_then(|n| n.checked_add(key_bytes))
        .ok_or_else(limit)?;
    pay(processing, phase_bytes, &mut ctx.cancelled)?;
    let _phase = memory
        .reserve(phase_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut changes = Vec::with_capacity(count);
    if changes.capacity() != count {
        return Err(limit().into());
    }
    for root in &roots.state {
        let stop = RefCell::new(&mut ctx.cancelled);
        let bank = RefCell::new(&mut *processing);
        ctx.transaction.visit_pending_writes(
            root.namespace,
            pending,
            || {
                pay(&mut bank.borrow_mut(), 0, &mut || (stop.borrow_mut())())
                    .map_err(WorldFieldOverlayErrorV1::from)
            },
            |key, before, after| {
                pay(&mut bank.borrow_mut(), key.len(), &mut || {
                    (stop.borrow_mut())()
                })?;
                if before != after
                    && identify(root.namespace, key)
                        .map_err(owner_error)?
                        .is_some()
                {
                    if changes.len() == count {
                        return Err(limit().into());
                    }
                    let mut bytes = Vec::with_capacity(key.len());
                    if bytes.capacity() != key.len() {
                        return Err(limit().into());
                    }
                    bytes.extend_from_slice(key);
                    changes.push(Change {
                        namespace: root.namespace,
                        key: bytes,
                        before,
                        after,
                    })
                }
                Ok::<_, WorldFieldOverlayErrorV1>(())
            },
        )?
    }
    if changes.len() != count {
        return Err(bad().into());
    }
    let bytes = count
        .checked_mul(size_of::<Row>())
        .and_then(|n| n.checked_add(size_of::<CurrentPrimitiveProjectionDeltaV1>()))
        .ok_or_else(limit)?;
    pay(processing, bytes, &mut ctx.cancelled)?;
    let fee = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut rows = Vec::with_capacity(count);
    if rows.capacity() != count {
        return Err(limit().into());
    }
    for change in &changes {
        pay(processing, change.key.len(), &mut ctx.cancelled)?;
        if ctx.transaction.read(change.namespace, &change.key)? != change.after {
            return Err(bad().into());
        }
        if let Some(row) = one(change, roots, ctx, family, processing)? {
            rows.push(row)
        }
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    let guard = ctx.transaction.guard();
    ctx.transaction.validate_guard(&guard, live)?;
    Ok(CurrentPrimitiveProjectionDeltaV1 {
        rows,
        guard,
        parent: ctx.transaction.parent_fence().parent_roots,
        memory,
        _fee: fee,
    })
}

#[path = "incremental_owner_closure_primitives_projection_cold.rs"]
mod cold;
pub use cold::{
    QualifiedPrimitiveProjectionColumnsV1, read_qualified_primitive_projection_columns_v1,
};
