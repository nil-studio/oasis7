//! Actual changed Profile/Transfer records through the original outer/body/cell
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
    kind: MaterialRecordKindV1,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
struct Row {
    kind: MaterialRecordKindV1,
    semantic: ReservedValue<Vec<u8>>,
    key: ReservedValue<Vec<u8>>,
    json: Option<ReservedValue<Vec<u8>>>,
}
pub struct CurrentMaterialProjectionDeltaV1 {
    rows: Vec<Row>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Material projection")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current Material projection")
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
impl CurrentMaterialProjectionDeltaV1 {
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
pub fn is_material_projection_field_v1(name: &str) -> bool {
    MaterialRecordKindV1::ALL
        .iter()
        .any(|kind| kind.field_name() == name)
}
fn identify(key: &[u8]) -> Result<Option<MaterialRecordKindV1>, BodyMutationError> {
    if !WorldFieldKeyV1::is_field_key_v1(key) {
        return Ok(None);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key).map_err(ValueTreeError::Invalid)?;
    let Some(kind) = MaterialRecordKindV1::ALL
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
    kind: MaterialRecordKindV1,
    which: MaterialCellV1,
    ctx: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    output: &mut impl Write,
    p: &mut BusinessDiffProcessingWorkV1,
) -> Result<MaterialValueV1, BodyMutationError> {
    pay(p, 0, &mut ctx.cancelled)?;
    *cells = cells.checked_sub(1).ok_or_else(limit)?;
    let key = read::cell_key(kind, which, &mut ctx.records())?;
    let root = ctx.lookup(&key.0)?.ok_or_else(bad)?;
    Ok(read::value(
        which,
        &root,
        &key,
        &mut ctx.records(),
        max,
        output,
    )?)
}
fn bytes<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: MaterialRecordKindV1,
    which: MaterialCellV1,
    ctx: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    p: &mut BusinessDiffProcessingWorkV1,
    held: &mut MemoryReservation,
) -> Result<(MaterialValueV1, String), BodyMutationError> {
    let value = cell(kind, which, ctx, cells, max, &mut io::sink(), p)?;
    let length = match value {
        MaterialValueV1::Text { utf8_bytes } => utf8_bytes,
        MaterialValueV1::Ledger(info) => info.id_bytes,
        _ => return Err(bad().into()),
    };
    let n = usize::try_from(length).map_err(|_| limit())?;
    let memory = ctx.values.memory.clone();
    let mut fee = reserve(
        &memory,
        n.checked_add(2 * size_of::<String>()).ok_or_else(limit)?,
        p,
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
                return Err(io::Error::other("material projection text bound"));
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
    let again = cell(kind, which, ctx, cells, max, &mut out, p)?;
    if again != value || out.bytes.len() != n {
        return Err(bad().into());
    }
    pay(p, n, &mut ctx.cancelled)?;
    let result = String::from_utf8(out.bytes);
    pay(p, 0, &mut ctx.cancelled)?;
    let text = result.map_err(|_| bad())?;
    held.merge(&mut fee).map_err(ValueTreeError::Invalid)?;
    Ok((value, text))
}
fn json<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: MaterialRecordKindV1,
    ctx: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    p: &mut BusinessDiffProcessingWorkV1,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    use MaterialCellV1 as F;
    let memory = ctx.values.memory.clone();
    let mut held = reserve(
        &memory,
        size_of::<MaterialProfileV1>().max(size_of::<MaterialTransferReceiptV1>()),
        p,
        &mut ctx.cancelled,
    )?;
    macro_rules! text {
        ($field:expr) => {
            bytes(kind, $field, ctx, cells, max, p, &mut held)?.1
        };
    }
    macro_rules! uint {
        ($field:expr) => {
            match cell(kind, $field, ctx, cells, max, &mut io::sink(), p)? {
                MaterialValueV1::Unsigned(v) => v,
                _ => return Err(bad().into()),
            }
        };
    }
    macro_rules! sint {
        ($field:expr) => {
            match cell(kind, $field, ctx, cells, max, &mut io::sink(), p)? {
                MaterialValueV1::Signed(v) => v,
                _ => return Err(bad().into()),
            }
        };
    }
    macro_rules! boolean {
        ($field:expr) => {
            match cell(kind, $field, ctx, cells, max, &mut io::sink(), p)? {
                MaterialValueV1::Bool(v) => v,
                _ => return Err(bad().into()),
            }
        };
    }
    macro_rules! ledger {
        ($field:expr) => {{
            let (value, text) = bytes(kind, $field, ctx, cells, max, p, &mut held)?;
            let MaterialValueV1::Ledger(info) = value else {
                return Err(bad().into());
            };
            use crate::incremental_ledger_value::LedgerValueKindV1 as K;
            match info.kind {
                K::World => {
                    if !text.is_empty() {
                        return Err(bad().into());
                    }
                    crate::MaterialLedgerId::World
                }
                K::Agent => crate::MaterialLedgerId::Agent(text),
                K::Site => crate::MaterialLedgerId::Site(text),
                K::Factory => crate::MaterialLedgerId::Factory(text),
            }
        }};
    }
    match kind {
        MaterialRecordKindV1::Profile => {
            let dto = MaterialProfileV1 {
                kind: text!(F::Kind),
                tier: u8::try_from(uint!(F::Tier)).map_err(|_| bad())?,
                category: text!(F::Category),
                stack_limit: sint!(F::StackLimit),
                transport_loss_class: match uint!(F::TransportLossClass) {
                    0 => MaterialTransportLossClass::Low,
                    1 => MaterialTransportLossClass::Medium,
                    2 => MaterialTransportLossClass::High,
                    _ => return Err(bad().into()),
                },
                decay_bps_per_tick: sint!(F::DecayBpsPerTick),
                default_priority: match uint!(F::DefaultPriority) {
                    0 => MaterialDefaultPriority::Urgent,
                    1 => MaterialDefaultPriority::Standard,
                    _ => return Err(bad().into()),
                },
            };
            crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
                &dto,
                &memory,
                p,
                &mut ctx.cancelled,
            )
            .map_err(Into::into)
        }
        MaterialRecordKindV1::Transfer => {
            let dto = MaterialTransferReceiptV1 {
                transfer_id: uint!(F::TransferId),
                requester_agent_id: text!(F::RequesterAgentId),
                from_ledger: ledger!(F::FromLedger),
                to_ledger: ledger!(F::ToLedger),
                kind: text!(F::Kind),
                amount: sint!(F::Amount),
                distance_km: sint!(F::DistanceKm),
                priority: match uint!(F::Priority) {
                    0 => MaterialTransitPriority::Urgent,
                    1 => MaterialTransitPriority::Standard,
                    _ => return Err(bad().into()),
                },
                route_id: if boolean!(F::RoutePresent) {
                    Some(text!(F::RouteId))
                } else {
                    None
                },
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
    family: &ReservedValue<MaterialFamilyQualificationV1>,
    roots: &RootSetV1,
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
        size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>() + 256,
        processing,
        &mut top.cancelled,
    )?;
    let selected = super::family::outer(&entry, &mut top)?;
    drop(top);
    let Some((_kind, owner, child)) = selected else {
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
    let key_json = match (
        change.kind,
        read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?,
    ) {
        (MaterialRecordKindV1::Profile, SemanticKeyRefV1::Text(id)) => {
            crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
                &id,
                &memory,
                processing,
                &mut ctx.cancelled,
            )?
        }
        (MaterialRecordKindV1::Transfer, SemanticKeyRefV1::Unsigned(id)) => {
            crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
                &id,
                &memory,
                processing,
                &mut ctx.cancelled,
            )?
        }
        _ => return Err(bad().into()),
    };
    let data = if change.after.is_some() {
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
        let _proof =
            qualify_material_body_v1(change.kind, owner, &mut selected, scan, cells, max_text)?;
        Some(json(
            change.kind,
            &mut selected,
            cells,
            max_text,
            processing,
        )?)
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
pub fn read_current_material_projection_delta_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<MaterialFamilyQualificationV1>,
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
) -> Result<CurrentMaterialProjectionDeltaV1, BodyMutationError> {
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
        for namespace in [
            MaterialRecordKindV1::Profile.namespace(),
            MaterialRecordKindV1::Transfer.namespace(),
        ] {
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
        for namespace in [
            MaterialRecordKindV1::Profile.namespace(),
            MaterialRecordKindV1::Transfer.namespace(),
        ] {
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
                .checked_mul(size_of::<Row>())
                .and_then(|n| n.checked_add(size_of::<CurrentMaterialProjectionDeltaV1>()))
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
        let output = CurrentMaterialProjectionDeltaV1 {
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

#[path = "incremental_material_records_projection_cold.rs"]
mod cold;
pub use cold::{
    QualifiedMaterialProjectionColumnsV1, read_qualified_material_projection_columns_v1,
};
