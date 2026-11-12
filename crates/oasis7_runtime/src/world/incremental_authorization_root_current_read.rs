//! Exclusive Common bridge for fixed actual source readers. Private callbacks
//! select only a concrete reader; callers cannot override a source predicate.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_json_records::JsonReadWorkV1,
    incremental_runtime_capability_records::RuntimeCapabilityRecordsWorkV1,
    incremental_runtime_revocation::RevocationReadWorkV1,
};
#[derive(Debug)]
pub(in crate::world) enum SourceError {
    Cache(AuthorizationRootCacheErrorV1),
    Overlay(oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1),
    Execution(oasis7_formal_protocol::incremental_execution::ExecutionError),
    Semantic(oasis7_world_state::incremental_semantic_key::SemanticKeyError),
}
impl From<AuthorizationRootCacheErrorV1> for SourceError {
    fn from(e: AuthorizationRootCacheErrorV1) -> Self {
        Self::Cache(e)
    }
}
impl From<oasis7_formal_protocol::incremental_execution::ExecutionError> for SourceError {
    fn from(e: oasis7_formal_protocol::incremental_execution::ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<MemoryLimitExceeded> for SourceError {
    fn from(e: MemoryLimitExceeded) -> Self {
        Self::Cache(e.into())
    }
}
impl From<ValueTreeError> for SourceError {
    fn from(e: ValueTreeError) -> Self {
        Self::Cache(e.into())
    }
}
impl From<FormalError> for SourceError {
    fn from(e: FormalError) -> Self {
        Self::Cache(e.into())
    }
}
impl From<oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1> for SourceError {
    fn from(e: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
pub(in crate::world) struct AuthorizationRootSourceReadWorkV1 {
    pub json: JsonReadWorkV1,
    pub typed: RevocationReadWorkV1,
    pub scan: StateScanBudget,
    pub capabilities: RuntimeCapabilityRecordsWorkV1,
    pub pending_entries: usize,
    pub max_selected_heap: usize,
    pub max_text_bytes: u64,
}
pub(super) fn metadata(
    roots: &RootSetV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), SourceError> {
    if roots.world_id.len() > 256 {
        return Err(invalid("authorization cache world bound").into());
    }
    let wire = b"oasis7.iws.root-set.v1\0".len()
        + 4
        + 2
        + roots.world_id.len()
        + 48
        + 6 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES;
    let validation = roots.world_id.len()
        + 6 * (64 + oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES)
        + 32;
    // Eight fixed RootSet validation/copy/digest paths cover every single
    // selected source adapter; payload/typed traversal has its own lanes below.
    common.charge((validation * 8) as u64, ((wire + validation) * 8) as u64)?;
    roots.validate().map_err(SourceError::Execution)?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn paid<M, R, E, C, P, V, ER, CR, T>(
    roots: &RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut AuthorizationRootSourceReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    operation: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
        &mut WorldFieldReadContextV1<'_, P, V, &mut ER, &mut dyn FnMut() -> bool>,
        &mut AuthorizationRootSourceReadWorkV1,
    ) -> Result<T, SourceError>,
) -> Result<T, SourceError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    metadata(roots, common)?;
    let (_, meter, stop) = common.parts();
    let lease = meter.begin_child()?;
    let visits = lease.remaining_steps() / 9;
    let bytes = lease.remaining_read_bytes() / 5;
    overlay.values.io.nodes_remaining = overlay.values.io.nodes_remaining.min(visits);
    read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(visits);
    work.scan.nodes_remaining = work.scan.nodes_remaining.min(visits);
    work.scan.leaves_remaining = work.scan.leaves_remaining.min(visits);
    work.json.cells_remaining = work.json.cells_remaining.min(visits);
    work.json.json_nodes_remaining = work.json.json_nodes_remaining.min(visits);
    work.typed.json.cells_remaining = work.typed.json.cells_remaining.min(visits);
    work.typed.json.json_nodes_remaining = work.typed.json.json_nodes_remaining.min(visits);
    work.capabilities.cells_remaining = work.capabilities.cells_remaining.min(visits);
    overlay.values.io.read_bytes_remaining = overlay.values.io.read_bytes_remaining.min(bytes);
    read.values.io.read_bytes_remaining = read.values.io.read_bytes_remaining.min(bytes);
    work.scan.read_bytes_remaining = work.scan.read_bytes_remaining.min(bytes);
    overlay.key_work.bytes_remaining = overlay.key_work.bytes_remaining.min(bytes);
    read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(bytes);
    let before_steps = [
        overlay.values.io.nodes_remaining,
        read.values.io.nodes_remaining,
        work.scan.nodes_remaining,
        work.scan.leaves_remaining,
        work.json.cells_remaining,
        work.json.json_nodes_remaining,
        work.typed.json.cells_remaining,
        work.typed.json.json_nodes_remaining,
        work.capabilities.cells_remaining,
    ];
    let before_bytes = [
        overlay.values.io.read_bytes_remaining,
        read.values.io.read_bytes_remaining,
        work.scan.read_bytes_remaining,
        overlay.key_work.bytes_remaining,
        read.key_work.bytes_remaining,
    ];
    let result = {
        let stop = std::cell::RefCell::new(stop);
        let mut current_stop = || (overlay.cancelled)() || (stop.borrow_mut())();
        let mut parent_stop = || (read.cancelled)() || (stop.borrow_mut())();
        let mut current = WorldFieldOverlayContextV1 {
            transaction: &mut *overlay.transaction,
            values: ValueReadContext {
                source: overlay.values.source,
                memory: overlay.values.memory,
                class: overlay.values.class,
                io: &mut *overlay.values.io,
            },
            key_work: &mut *overlay.key_work,
            semantic_bytes: overlay.semantic_bytes,
            record_bytes: overlay.record_bytes,
            take_state_error: &mut overlay.take_state_error,
            cancelled: &mut current_stop as &mut dyn FnMut() -> bool,
        };
        let mut parent = WorldFieldReadContextV1 {
            root: read.root,
            state: read.state,
            values: ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut parent_stop as &mut dyn FnMut() -> bool,
        };
        operation(&mut current, &mut parent, work)
    };
    let after_steps = [
        overlay.values.io.nodes_remaining,
        read.values.io.nodes_remaining,
        work.scan.nodes_remaining,
        work.scan.leaves_remaining,
        work.json.cells_remaining,
        work.json.json_nodes_remaining,
        work.typed.json.cells_remaining,
        work.typed.json.json_nodes_remaining,
        work.capabilities.cells_remaining,
    ];
    let after_bytes = [
        overlay.values.io.read_bytes_remaining,
        read.values.io.read_bytes_remaining,
        work.scan.read_bytes_remaining,
        overlay.key_work.bytes_remaining,
        read.key_work.bytes_remaining,
    ];
    let debit = |before: &[u64], after: &[u64]| {
        before
            .iter()
            .zip(after)
            .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
    };
    let Some(steps) = debit(&before_steps, &after_steps) else {
        return result.and(Err(invalid("authorization source visits accounting").into()));
    };
    let Some(bytes) = debit(&before_bytes, &after_bytes) else {
        return result.and(Err(invalid("authorization source bytes accounting").into()));
    };
    let paid = lease.consume(steps, bytes);
    lease.finish();
    let value = result?;
    paid?;
    common.consume_steps(0)?;
    if (overlay.cancelled)() || (read.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(value)
}
