//! Concrete qualified cold source binder. The only caller is the fixed
//! complete typed-column initializer; no external seed/root setter exists.
use super::*;
use crate::world::incremental_normal_projection_commitment::{
    QualifiedColdProjectionSeedV1, QualifiedColdProjectionSourceFeesV1,
};
use crate::world::incremental_normal_sources::BoundNormalTickParentV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
};

/// Immutable original sources stay alive through the cache's final signed
/// projection-root comparison. Validation is performed before moving them in;
/// no later operation can replace one of these private fields.
pub(super) struct ColdBootstrapProjectionFeesV1 {
    _sources: QualifiedColdProjectionSourceFeesV1,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl ColdBootstrapProjectionFeesV1 {
    pub(super) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self._sources.validate_context(common)?;
        common.consume_steps(1).map_err(paid)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(invalid("cold projection retained actual sources"));
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn bind_qualified_cold_projection_source_v1<M: MerkleNodeReader>(
    source: QualifiedColdProjectionSeedV1,
    parent: &BoundNormalTickParentV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    side: PaidVerifiedProjectionSidecarV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<BoundNormalProjectionBootstrapSourceV1, BodyMutationError> {
    parent.validate_context(common).map_err(paid)?;
    if !tx.uses_budget(common.memory()) || tx.parent_fence() != parent.fence() {
        return Err(invalid("cold projection exact bootstrap TX"));
    }
    tx.validate_bootstrap_read_only_v1(live)?;
    source.validate_v1(parent, tx, common, live)?;
    let (columns, sources) = source.into_parts_v1();
    columns.validate_context(common)?;
    let refs = parent.refs();
    side.validate_context(refs.head, refs.core, common)
        .map_err(paid)?;
    // The private producer retained and checked every concrete ALL owner against
    // this accepted parent and the same read-only transaction before consumption.
    let json = side.retained_history_root_json();
    let graph = json
        .len()
        .checked_mul(128)
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(|| invalid("cold projection side graph"))?;
    common.charge(1, graph as u64).map_err(paid)?;
    let _graph = common
        .memory()
        .reserve(graph, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    // Original typed syntax, without granting any retained-History authority.
    let _: Option<oasis7_world_state::formal_retained_history::FormalRetainedHistoryRootV1> =
        serde_json::from_slice(json).map_err(paid)?;
    let mut selected = None;
    for field in columns.fields() {
        common.consume_steps(1).map_err(paid)?;
        if field.descriptor().owner
            == oasis7_world_state::incremental_projection::FieldOwner::WorldState
            && field.descriptor().name == "retained_history_root"
        {
            if selected.is_some() || field.rows().len() != 1 {
                return Err(invalid("cold projection side column uniqueness"));
            }
            selected = Some(field.rows()[0].original_json());
        }
    }
    let actual = selected.ok_or_else(|| invalid("cold projection actual side column"))?;
    common
        .charge(1, (actual.len() + json.len()) as u64)
        .map_err(paid)?;
    if actual != json {
        return Err(invalid("cold projection side differs from typed columns"));
    }
    let expected_projection_root = copy_text(&refs.head.record().projection_root, common)?;
    let n = std::mem::size_of::<ColdBootstrapProjectionFeesV1>()
        .checked_add(
            std::mem::size_of::<BootstrapProjectionSourceFeesV1>()
                + std::mem::size_of::<BoundNormalProjectionBootstrapSourceV1>(),
        )
        .ok_or_else(|| invalid("cold projection source layout"))?;
    common.charge(1, n as u64).map_err(paid)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let physical = ColdBootstrapProjectionFeesV1 {
        _sources: sources,
        core: common.work().identity(),
        memory: common.memory().clone(),
        _fee: common
            .memory()
            .reserve(
                std::mem::size_of::<ColdBootstrapProjectionFeesV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?,
    };
    tx.validate_bootstrap_read_only_v1(live)?;
    Ok(BoundNormalProjectionBootstrapSourceV1 {
        columns,
        record: refs.head.verified_record_id(),
        body: refs.core.catalog().entry.commit,
        roots: refs.core.body().roots,
        fees: BootstrapProjectionSourceFeesV1 {
            physical: BootstrapProjectionPhysicalSourceV1::Cold(physical),
            side,
            expected_projection_root,
            expected_genesis_raw_state_hash: None,
            core: common.work().identity(),
            memory: common.memory().clone(),
            _fee: fee,
        },
    })
}
