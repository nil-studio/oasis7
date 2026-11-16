//! Actual changed IntentLedger/AgentLocation/LocationAnchor records through the original outer/body/cell
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
    kind: AgentAuthorityRecordKindV1,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
struct Row {
    kind: AgentAuthorityRecordKindV1,
    semantic: ReservedValue<Vec<u8>>,
    key: ReservedValue<Vec<u8>>,
    json: Option<ReservedValue<Vec<u8>>>,
}
pub struct CurrentAgentAuthorityProjectionDeltaV1 {
    rows: Vec<Row>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current AgentAuthority projection")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current AgentAuthority projection")
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
impl CurrentAgentAuthorityProjectionDeltaV1 {
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
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            emit(
                row.kind.field_name(),
                &row.semantic,
                &row.key,
                row.json.as_deref().map(Vec::as_slice),
            )?
        }
        Ok(())
    }
}
pub fn is_agent_authority_projection_field_v1(name: &str) -> bool {
    AgentAuthorityRecordKindV1::ALL
        .iter()
        .any(|kind| kind.field_name() == name)
}
fn identify(key: &[u8]) -> Result<Option<AgentAuthorityRecordKindV1>, BodyMutationError> {
    if !WorldFieldKeyV1::is_field_key_v1(key) {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    let Some(kind) = AgentAuthorityRecordKindV1::ALL
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
fn cell<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: AgentAuthorityRecordKeyV1<'_>,
    body: &ReservedValue<AgentAuthorityRecordQualificationV1>,
    cell: AgentAuthorityCellV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    output: &mut impl Write,
    processing: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
) -> Result<AgentAuthorityValueV1, BodyMutationError> {
    pay(processing, 0, &mut ctx.cancelled)?;
    *cells = cells.checked_sub(1).ok_or_else(limit)?;
    let guard = ctx.transaction.guard();
    read_qualified_agent_authority_cell_overlay_v1(
        AgentAuthorityCellRequestV1 {
            key,
            cell,
            qualification: body,
        },
        ctx,
        max,
        output,
        &guard,
        live,
    )
    .map_err(oe)
}
fn text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: AgentAuthorityRecordKeyV1<'_>,
    body: &ReservedValue<AgentAuthorityRecordQualificationV1>,
    which: AgentAuthorityCellV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    processing: &mut BusinessDiffProcessingWorkV1,
    held: &mut MemoryReservation,
    live: &ExecutionFence,
) -> Result<String, BodyMutationError> {
    let value = cell(
        key,
        body,
        which,
        ctx,
        cells,
        max,
        &mut io::sink(),
        processing,
        live,
    )?;
    let AgentAuthorityValueV1::Text { utf8_bytes } = value else {
        return Err(bad().into());
    };
    let n = usize::try_from(utf8_bytes).map_err(|_| limit())?;
    let memory = ctx.values.memory.clone();
    let mut fee = reserve(
        &memory,
        n.checked_add(2 * size_of::<String>()).ok_or_else(limit)?,
        processing,
        &mut ctx.cancelled,
    )?;
    struct Out {
        bytes: Vec<u8>,
        bound: usize,
    }
    impl Write for Out {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self
                .bytes
                .len()
                .checked_add(b.len())
                .filter(|n| *n <= self.bound)
                .is_none()
            {
                return Err(io::Error::other("agent-authority projection text bound"));
            }
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = Out {
        bytes: Vec::with_capacity(n),
        bound: n,
    };
    if out.bytes.capacity() != n {
        return Err(limit().into());
    }
    let again = cell(
        key, body, which, ctx, cells, max, &mut out, processing, live,
    )?;
    if again != value || out.bytes.len() != n {
        return Err(bad().into());
    }
    pay(processing, n, &mut ctx.cancelled)?;
    let parsed = String::from_utf8(out.bytes);
    pay(processing, 0, &mut ctx.cancelled)?;
    let value = parsed.map_err(|_| bad())?;
    held.merge(&mut fee).map_err(ValueTreeError::Invalid)?;
    Ok(value)
}
fn unsigned<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: AgentAuthorityRecordKeyV1<'_>,
    body: &ReservedValue<AgentAuthorityRecordQualificationV1>,
    which: AgentAuthorityCellV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    p: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
) -> Result<u64, BodyMutationError> {
    match cell(key, body, which, ctx, cells, max, &mut io::sink(), p, live)? {
        AgentAuthorityValueV1::Unsigned(value) => Ok(value),
        _ => Err(bad().into()),
    }
}
fn boolean<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: AgentAuthorityRecordKeyV1<'_>,
    body: &ReservedValue<AgentAuthorityRecordQualificationV1>,
    which: AgentAuthorityCellV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    p: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
) -> Result<bool, BodyMutationError> {
    match cell(key, body, which, ctx, cells, max, &mut io::sink(), p, live)? {
        AgentAuthorityValueV1::Bool(value) => Ok(value),
        _ => Err(bad().into()),
    }
}
fn json<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: AgentAuthorityRecordKeyV1<'_>,
    body: &ReservedValue<AgentAuthorityRecordQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    p: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    use AgentAuthorityCellV1::Column;
    use AgentAuthorityColumnV1 as F;
    let memory = ctx.values.memory.clone();
    let header = size_of::<crate::AgentIntentV2>()
        .max(size_of::<crate::AgentLocationAuthorityV1>())
        .max(size_of::<crate::LocationAnchorV1>());
    let mut held = reserve(&memory, header, p, &mut ctx.cancelled)?;
    macro_rules! text {
        ($field:expr) => {
            text(
                key,
                body,
                Column($field),
                ctx,
                cells,
                max,
                p,
                &mut held,
                live,
            )?
        };
    }
    macro_rules! boolean {
        ($field:expr) => {
            boolean(key, body, Column($field), ctx, cells, max, p, live)?
        };
    }
    macro_rules! uint {
        ($field:expr) => {
            unsigned(key, body, Column($field), ctx, cells, max, p, live)?
        };
    }
    match key.kind {
        AgentAuthorityRecordKindV1::IntentLedger => {
            use crate::incremental_intent_fields::{
                OriginalIntentProjectionScalarV1 as V, assemble_original_intent_projection_v1,
            };
            let dto = assemble_original_intent_projection_v1(
                |field| {
                    let which = Column(F::Intent(field));
                    let value = cell(key, body, which, ctx, cells, max, &mut io::sink(), p, live)?;
                    Ok::<_, BodyMutationError>(match value {
                        AgentAuthorityValueV1::Text { .. } => {
                            V::Text(text(key, body, which, ctx, cells, max, p, &mut held, live)?)
                        }
                        AgentAuthorityValueV1::Unit => V::Unit,
                        AgentAuthorityValueV1::Unsigned(value) => {
                            if field == IntentFieldV1::SchemaVersion {
                                V::U32(u32::try_from(value).map_err(|_| bad())?)
                            } else {
                                V::U64(value)
                            }
                        }
                        _ => return Err(bad().into()),
                    })
                },
                "Current AgentAuthority projection",
            )?;
            crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
                &dto,
                &memory,
                p,
                &mut ctx.cancelled,
            )
            .map_err(Into::into)
        }
        AgentAuthorityRecordKindV1::AgentLocation => {
            let dto = crate::AgentLocationAuthorityV1 {
                agent_id: text!(F::AgentId),
                location_id: text!(F::LocationId),
                active: boolean!(F::Active),
                authority_revision: uint!(F::AuthorityRevision),
                effective_at: uint!(F::EffectiveAt),
            };
            crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
                &dto,
                &memory,
                p,
                &mut ctx.cancelled,
            )
            .map_err(Into::into)
        }
        AgentAuthorityRecordKindV1::LocationAnchor => {
            let dto = crate::LocationAnchorV1 {
                location_id: text!(F::LocationId),
                active: boolean!(F::Active),
                authority_revision: uint!(F::AuthorityRevision),
                effective_at: uint!(F::EffectiveAt),
            };
            crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
                &dto,
                &memory,
                p,
                &mut ctx.cancelled,
            )
            .map_err(Into::into)
        }
    }
}
fn decode_projection_row_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    change: Change,
    family: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    processing: &mut BusinessDiffProcessingWorkV1,
    max_text: u64,
    live: &ExecutionFence,
) -> Result<Option<Row>, BodyMutationError> {
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
    let selected = {
        let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        super::family::outer_parts_from_records_v1(
            entry.namespace,
            &entry.key,
            entry.value,
            &mut records,
        )?
    };
    if selected.is_none() {
        return Ok(None);
    }
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
    let key = AgentAuthorityRecordKeyV1 {
        kind: change.kind,
        id,
    };
    let key_json =
        crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
            &id,
            &memory,
            processing,
            &mut ctx.cancelled,
        )?;
    let data = if change.after.is_some() {
        let body =
            open_agent_authority_body_overlay_v1(key, family, state, ctx, scan, cells, max_text)
                .map_err(oe)?
                .ok_or_else(|| BodyMutationError::Value(bad()))?;
        Some(json(key, &body, ctx, cells, max_text, processing, live)?)
    } else {
        None
    };
    Ok(Some(Row {
        kind: change.kind,
        semantic,
        key: key_json,
        json: data,
    }))
}
pub fn read_current_agent_authority_projection_delta_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<AgentAuthorityFamilyQualificationV1>,
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
) -> Result<CurrentAgentAuthorityProjectionDeltaV1, BodyMutationError> {
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
        {
            let p = RefCell::new(&mut *processing);
            let stop = RefCell::new(&mut ctx.cancelled);
            for root in &roots.state {
                if !AgentAuthorityRecordKindV1::ALL
                    .iter()
                    .any(|kind| kind.namespace() == root.namespace)
                {
                    continue;
                }
                ctx.transaction.visit_pending_writes(
                    root.namespace,
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
        {
            let p = RefCell::new(&mut *processing);
            let stop = RefCell::new(&mut ctx.cancelled);
            for root in &roots.state {
                if !AgentAuthorityRecordKindV1::ALL
                    .iter()
                    .any(|kind| kind.namespace() == root.namespace)
                {
                    continue;
                }
                ctx.transaction.visit_pending_writes(
                    root.namespace,
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
        }
        let fee = reserve(
            &memory,
            count
                .checked_mul(size_of::<Row>())
                .and_then(|n| n.checked_add(size_of::<CurrentAgentAuthorityProjectionDeltaV1>()))
                .ok_or_else(limit)?,
            processing,
            &mut ctx.cancelled,
        )?;
        let mut rows = Vec::with_capacity(count);
        if rows.capacity() != count {
            return Err(limit().into());
        }
        for change in changes {
            if let Some(row) = decode_projection_row_v1(
                change, family, state, ctx, scan, cells, processing, max_text, live,
            )? {
                rows.push(row)
            }
        }

        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        pay(processing, 0, &mut ctx.cancelled)?;
        let guard = ctx.transaction.guard();
        let output = CurrentAgentAuthorityProjectionDeltaV1 {
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

#[path = "incremental_agent_authority_records_projection_cold.rs"]
mod cold;
pub use cold::{
    QualifiedAgentAuthorityProjectionColumnsV1,
    read_qualified_agent_authority_projection_columns_v1,
};
