//! Actual Current single-target Meta records, not snapshot recovery or authority.
//! Child of native qualification; immutable before evidence survives preparation.
use super::*;
pub use crate::incremental_typed_record_codec::meta_progress_owned::MetaProgressMutationWorkV1;
use crate::incremental_typed_record_codec::meta_progress_owned::{
    PreparedMetaProgressValueV1, prepare_meta_progress_owned_v1,
};
use crate::incremental_world_native_gameplay::MetaProgressFieldV1;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
#[path = "incremental_meta_progress_write.rs"]
mod publication;
pub use publication::write_prepared_current_meta_progress_v1;
#[cfg(test)]
#[path = "incremental_meta_progress_tests.rs"]
mod tests;
/// Plain data owner; no mutable DTO, approval or accepted-event token escapes.
pub struct CurrentMetaProgressRecordV1 {
    record: Option<WorldNativeRecoveredRecordV1<crate::MetaProgressState>>,
    target: String,
    key: WorldFieldKeyV1,
    metadata: WorldFieldKeyV1,
    metadata_root: [u8; 32],
    before: Option<[u8; 32]>,
    parent: [u8; 32],
    parent_records: u64,
    _body: Option<ReservedValue<StateRootV1>>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl CurrentMetaProgressRecordV1 {
    pub fn value(&self) -> Option<&crate::MetaProgressState> {
        self.record.as_ref().map(|v| v.value().value())
    }
    pub fn target(&self) -> &str {
        &self.target
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.contribute(|tx| {
            tx.validate_guard(&self.guard, live)?;
            if !self.memory.same_pool(memory)
                || !tx.uses_budget(memory)
                || self.parent != tx.parent_fence().parent_roots
            {
                return Err(invalid().into());
            }
            Ok(())
        })
    }
    fn comparison_bytes(&self, other: &Self) -> Result<usize, ValueTreeError> {
        self.target
            .len()
            .max(other.target.len())
            .checked_add(self.key.as_bytes().len().max(other.key.as_bytes().len()))
            .and_then(|n| {
                n.checked_add(
                    self.metadata
                        .as_bytes()
                        .len()
                        .max(other.metadata.as_bytes().len()),
                )
            })
            .and_then(|n| n.checked_add(32 * 3 + 8))
            .ok_or(ValueTreeError::ResourceLimited("meta source comparisons"))
    }
    fn same_origin(&self, other: &Self) -> bool {
        self.target == other.target
            && self.key == other.key
            && self.metadata == other.metadata
            && self.metadata_root == other.metadata_root
            && self.before == other.before
            && self.parent == other.parent
            && self.parent_records == other.parent_records
            && self.memory.same_pool(&other.memory)
    }
}
/// Original decoder allocations plus independently admitted growth. The before
/// evidence is immutable even though its local decoded DTO has been updated.
pub struct PreparedCurrentMetaProgressV1 {
    origin: CurrentMetaProgressRecordV1,
    after: PreparedMetaProgressValueV1,
    _source_fee: MemoryReservation,
}
impl PreparedCurrentMetaProgressV1 {
    pub fn value(&self) -> &crate::MetaProgressState {
        self.after.value()
    }
    pub fn target(&self) -> &str {
        self.origin.target()
    }
}
#[allow(clippy::too_many_arguments)]
fn read_current<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    target: &str,
    parent: &ReservedValue<WorldNativeFieldQualificationV1<MetaProgressFieldV1>>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(CurrentMetaProgressRecordV1, MemoryReservation), WorldFieldOverlayErrorV1> {
    tx.validate_guard(guard, live)?;
    read::check(ctx)?;
    let _phase = ctx
        .values
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.allocation_class() != MemoryClass::Ordinary
        || !tx.uses_budget(ctx.values.memory)
    {
        return Err(invalid().into());
    }
    parent.validate_binding(roots)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    if digest != tx.parent_fence().parent_roots {
        return Err(invalid().into());
    }
    let field = MetaProgressFieldV1::descriptor()?;
    let profile = MetaProgressFieldV1::profile()?;
    let ns = profile.namespace()?;
    if ctx.root != roots.state_root(ns) {
        return Err(invalid().into());
    }
    let fee = ctx
        .values
        .memory
        .reserve(
            std::mem::size_of::<CurrentMetaProgressRecordV1>()
                .checked_add(target.len())
                .ok_or_else(invalid)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_root = tx.read(ns, metadata.as_bytes())?.ok_or_else(invalid)?;
    read::check(ctx)?;
    ctx.records().presence(&metadata_root, &metadata, 0)?;
    let key = ctx.key(field, WorldFieldRecordKindV1::Entry, target)?.0;
    let before = tx.read(ns, key.as_bytes())?;
    read::check(ctx)?;
    let (body, record) = if let Some(value) = before {
        let body =
            read::read_body_at_entry_v1::<MetaProgressFieldV1, _, _, _, _>(&key, &value, ctx)?;
        let record = recover_typed_record_body_v1(&body, &profile, ctx, scan, work)?;
        MetaProgressFieldV1::validate_record_binding(
            WorldNativeKeyRefV1::Text(target),
            &body,
            ctx,
            scan,
            work,
        )?;
        (Some(body), Some(record))
    } else {
        (None, None)
    };
    read::check(ctx)?;
    let tail = tx.guard();
    tx.validate_guard(&tail, live)?;
    let value = CurrentMetaProgressRecordV1 {
        record,
        target: target.to_owned(),
        key,
        metadata,
        metadata_root,
        before,
        parent: digest,
        parent_records: parent.records(),
        _body: body,
        guard: tail,
        memory: ctx.values.memory.clone(),
    };
    read::check(ctx)?;
    Ok((value, fee))
}
/// Missing is a real Current outer absence. Existing embedded agent_id is not
/// required to equal the outer target, matching the original World map grammar.
#[allow(clippy::too_many_arguments)]
pub fn recover_current_meta_progress_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    target: &str,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeSelectedReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentMetaProgressRecordV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        if !parent.uses_budget(ctx.values.memory) || parent.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(roots)?;
        let (value, fee) = read_current(
            target,
            &parent.meta_progress,
            roots,
            tx,
            ctx,
            scan,
            work,
            guard,
            live,
        )?;
        Ok(fee.retain(value))
    })
}
/// Re-decodes the actual same Current source before locally mutating its DTO.
/// No state is written. A later Accepted revision does not refresh this proof:
/// publication requires a new genuine Current opener and unchanged before root.
#[allow(clippy::too_many_arguments)]
fn prepare_field_delta<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentMetaProgressRecordV1>,
    track: &str,
    points: i64,
    achievement: Option<&str>,
    now: u64,
    parent: &ReservedValue<WorldNativeFieldQualificationV1<MetaProgressFieldV1>>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldNativeSelectedReadWorkV1,
    mutation: &mut MetaProgressMutationWorkV1,
    live: &ExecutionFence,
) -> Result<ReservedValue<PreparedCurrentMetaProgressV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        expected.validate(tx, ctx.values.memory, live)?;
        if !expected.uses_budget(ctx.values.memory) {
            return Err(invalid().into());
        }
        let carrier = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<PreparedCurrentMetaProgressV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let guard = tx.guard();
        let (mut origin, source_fee) = read_current(
            expected.target(),
            parent,
            roots,
            tx,
            ctx,
            scan,
            typed,
            &guard,
            live,
        )?;
        mutation.visit(expected.comparison_bytes(&origin)?, &mut ctx.cancelled)?;
        if !expected.same_origin(&origin) {
            return Err(invalid().into());
        }
        let record = origin.record.take();
        let after = prepare_meta_progress_owned_v1(
            record,
            origin.target(),
            track,
            points,
            achievement,
            now,
            ctx.values.memory,
            mutation,
            &mut ctx.cancelled,
        )?;
        read::check(ctx)?;
        Ok(carrier.retain(PreparedCurrentMetaProgressV1 {
            origin,
            after,
            _source_fee: source_fee,
        }))
    })
}

/// Prepare a paid local delta bound to the full genuine native parent carrier.
#[allow(clippy::too_many_arguments)]
pub fn prepare_current_meta_progress_delta_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentMetaProgressRecordV1>,
    track: &str,
    points: i64,
    achievement: Option<&str>,
    now: u64,
    parent: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldNativeSelectedReadWorkV1,
    mutation: &mut MetaProgressMutationWorkV1,
    live: &ExecutionFence,
) -> Result<ReservedValue<PreparedCurrentMetaProgressV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        if !parent.uses_budget(ctx.values.memory) || parent.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(roots)?;
        prepare_field_delta(
            expected,
            track,
            points,
            achievement,
            now,
            &parent.meta_progress,
            roots,
            tx,
            ctx,
            scan,
            typed,
            mutation,
            live,
        )
    })
}
