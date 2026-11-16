//! Private same-Core bridge for concrete industrial Current readers.
//! Scalar lanes are clamped in place, never replaced/refunded. The exclusive
//! Common lease prevents reentrant aliases from spending during child IO.
use super::*;
use current::EvaluationErrorV1;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn paid<M, R, E, C, P, V, PE, PC, T>(
    roots: &RootSetV1,
    current: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    operation: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
        &mut WorldFieldReadContextV1<'_, P, V, &mut PE, &mut dyn FnMut() -> bool>,
        &mut FactoryScheduleWorkV1,
    ) -> Result<T, EvaluationErrorV1>,
) -> Result<T, EvaluationErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    paid_inner(
        roots,
        current,
        parent,
        work,
        common,
        None,
        |ctx, parent, work, _| operation(ctx, parent, work),
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn paid_with_module<M, R, E, C, P, V, PE, PC, T>(
    roots: &RootSetV1,
    current: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    module: &mut crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleWorkV1<
        '_,
    >,
    operation: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
        &mut WorldFieldReadContextV1<'_, P, V, &mut PE, &mut dyn FnMut() -> bool>,
        &mut FactoryScheduleWorkV1,
        &mut crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleWorkV1<'_>,
    ) -> Result<T, EvaluationErrorV1>,
) -> Result<T, EvaluationErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    paid_inner(
        roots,
        current,
        parent,
        work,
        common,
        Some(module),
        |ctx, parent, work, module| {
            let module = module.ok_or_else(invalid)?;
            operation(ctx, parent, work, module)
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn paid_inner<M, R, E, C, P, V, PE, PC, T>(
    roots: &RootSetV1,
    current: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    mut module: Option<
        &mut crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleWorkV1<'_>,
    >,
    operation: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
        &mut WorldFieldReadContextV1<'_, P, V, &mut PE, &mut dyn FnMut() -> bool>,
        &mut FactoryScheduleWorkV1,
        Option<
            &mut crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleWorkV1<
                '_,
            >,
        >,
    ) -> Result<T, EvaluationErrorV1>,
) -> Result<T, EvaluationErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    if !current.values.memory.same_pool(common.memory())
        || !parent.values.memory.same_pool(common.memory())
        || current.values.class != MemoryClass::Ordinary
        || parent.values.class != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    // Eight fixed RootSet validate/digest/copy paths dominate each concrete
    // selected reader (inventory directory needs three). Root wire <=1024;
    // actual world/root strings are charged before the original validation.
    let payload = roots
        .world_id
        .len()
        .checked_add(6 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES)
        .ok_or_else(limited)?;
    charge(
        common,
        payload
            .checked_add(1024)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(limited)?,
    )?;
    let _metadata = common
        .memory()
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    roots.validate().map_err(BodyMutationError::from)?;
    let (_, meter, stop) = common.parts();
    let lease = meter
        .begin_child()
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let module_lanes = if module.is_some() { 25 } else { 0 };
    let visits = lease.remaining_steps() / (41 + module_lanes);
    let bytes = lease.remaining_read_bytes() / (23 + if module.is_some() { 4 } else { 0 });
    let module_original = module.as_deref().map(|m| m.io_snapshot_v1());
    if let Some(module) = module.as_deref_mut() {
        module.clamp_io_v1(visits, bytes);
    }
    let module_before = module.as_deref().map(|m| m.io_snapshot_v1());
    let original_steps = [
        current.values.io.nodes_remaining,
        parent.values.io.nodes_remaining,
        work.lifecycle.steps_remaining,
        work.lifecycle.factory.cells_remaining,
        work.lifecycle.factory.scan.nodes_remaining,
        work.lifecycle.factory.scan.leaves_remaining,
        work.lifecycle.industry.cells_remaining,
        work.lifecycle.industry.scan.nodes_remaining,
        work.lifecycle.industry.scan.leaves_remaining,
        work.lifecycle.lifecycle.cells_remaining,
        work.lifecycle.native_scan.nodes_remaining,
        work.lifecycle.native_scan.leaves_remaining,
        work.lifecycle.native_read.json.cells_remaining,
        work.lifecycle.native_read.json.json_nodes_remaining,
        work.builds.cells_remaining,
        work.receipts.cells_remaining,
        work.authority.cells_remaining,
        work.authority_cells,
        work.authority_scan.nodes_remaining,
        work.authority_scan.leaves_remaining,
        work.factory.cells_remaining,
        work.logistics.cells_remaining,
        work.inventory_directory.visits_remaining,
        work.inventory_family.records_remaining,
        work.inventory_family.index_steps_remaining,
        work.material_profiles.scan.nodes_remaining,
        work.material_profiles.scan.leaves_remaining,
        work.material_profiles.cells_remaining,
        work.invocation.instance_scan.nodes_remaining,
        work.invocation.instance_scan.leaves_remaining,
        work.invocation.registry_scan.nodes_remaining,
        work.invocation.registry_scan.leaves_remaining,
        work.config.cells_remaining,
        work.config.json.cells_remaining,
        work.config.json.json_nodes_remaining,
        work.config_scan.nodes_remaining,
        work.config_scan.leaves_remaining,
        work.lifecycle.pending_entries_remaining as u64,
        work.lifecycle.lifecycle.pending_entries_remaining as u64,
        work.inventory_directory.pending_entries_remaining as u64,
        work.invocation.pending_entries as u64,
    ];
    let original_bytes = [
        current.values.io.read_bytes_remaining,
        current.values.io.write_bytes_remaining,
        parent.values.io.read_bytes_remaining,
        parent.values.io.write_bytes_remaining,
        current.key_work.bytes_remaining,
        parent.key_work.bytes_remaining,
        work.lifecycle.factory.scan.read_bytes_remaining,
        work.lifecycle.industry.scan.read_bytes_remaining,
        work.lifecycle.native_scan.read_bytes_remaining,
        work.authority_scan.read_bytes_remaining,
        work.lifecycle.bytes_remaining,
        work.lifecycle.lifecycle.comparison_bytes_remaining,
        work.builds.comparison_bytes_remaining,
        work.receipts.bytes_remaining,
        work.authority.bytes_remaining,
        work.factory.bytes_remaining,
        work.logistics.bytes_remaining,
        work.inventory_directory.comparison_bytes_remaining,
        work.material_profiles.scan.read_bytes_remaining,
        work.invocation.instance_scan.read_bytes_remaining,
        work.invocation.registry_scan.read_bytes_remaining,
        work.invocation.comparison_bytes_remaining,
        work.config_scan.read_bytes_remaining,
    ];
    current.values.io.nodes_remaining = current.values.io.nodes_remaining.min(visits);
    parent.values.io.nodes_remaining = parent.values.io.nodes_remaining.min(visits);
    work.lifecycle.steps_remaining = work.lifecycle.steps_remaining.min(visits);
    work.lifecycle.factory.cells_remaining = work.lifecycle.factory.cells_remaining.min(visits);
    work.lifecycle.factory.scan.nodes_remaining =
        work.lifecycle.factory.scan.nodes_remaining.min(visits);
    work.lifecycle.factory.scan.leaves_remaining =
        work.lifecycle.factory.scan.leaves_remaining.min(visits);
    work.lifecycle.industry.cells_remaining = work.lifecycle.industry.cells_remaining.min(visits);
    work.lifecycle.industry.scan.nodes_remaining =
        work.lifecycle.industry.scan.nodes_remaining.min(visits);
    work.lifecycle.industry.scan.leaves_remaining =
        work.lifecycle.industry.scan.leaves_remaining.min(visits);
    work.lifecycle.lifecycle.cells_remaining = work.lifecycle.lifecycle.cells_remaining.min(visits);
    work.lifecycle.native_scan.nodes_remaining =
        work.lifecycle.native_scan.nodes_remaining.min(visits);
    work.lifecycle.native_scan.leaves_remaining =
        work.lifecycle.native_scan.leaves_remaining.min(visits);
    work.lifecycle.native_read.json.cells_remaining =
        work.lifecycle.native_read.json.cells_remaining.min(visits);
    work.lifecycle.native_read.json.json_nodes_remaining = work
        .lifecycle
        .native_read
        .json
        .json_nodes_remaining
        .min(visits);
    work.builds.cells_remaining = work.builds.cells_remaining.min(visits);
    work.receipts.cells_remaining = work.receipts.cells_remaining.min(visits);
    work.authority.cells_remaining = work.authority.cells_remaining.min(visits);
    work.authority_cells = work.authority_cells.min(visits);
    work.authority_scan.nodes_remaining = work.authority_scan.nodes_remaining.min(visits);
    work.authority_scan.leaves_remaining = work.authority_scan.leaves_remaining.min(visits);
    work.factory.cells_remaining = work.factory.cells_remaining.min(visits);
    work.logistics.cells_remaining = work.logistics.cells_remaining.min(visits);
    work.inventory_directory.visits_remaining =
        work.inventory_directory.visits_remaining.min(visits);
    work.inventory_family.records_remaining = work.inventory_family.records_remaining.min(visits);
    work.inventory_family.index_steps_remaining =
        work.inventory_family.index_steps_remaining.min(visits);
    work.material_profiles.scan.nodes_remaining =
        work.material_profiles.scan.nodes_remaining.min(visits);
    work.material_profiles.scan.leaves_remaining =
        work.material_profiles.scan.leaves_remaining.min(visits);
    work.material_profiles.cells_remaining = work.material_profiles.cells_remaining.min(visits);
    work.invocation.instance_scan.nodes_remaining =
        work.invocation.instance_scan.nodes_remaining.min(visits);
    work.invocation.instance_scan.leaves_remaining =
        work.invocation.instance_scan.leaves_remaining.min(visits);
    work.invocation.registry_scan.nodes_remaining =
        work.invocation.registry_scan.nodes_remaining.min(visits);
    work.invocation.registry_scan.leaves_remaining =
        work.invocation.registry_scan.leaves_remaining.min(visits);
    work.config.cells_remaining = work.config.cells_remaining.min(visits);
    work.config.json.cells_remaining = work.config.json.cells_remaining.min(visits);
    work.config.json.json_nodes_remaining = work.config.json.json_nodes_remaining.min(visits);
    work.config_scan.nodes_remaining = work.config_scan.nodes_remaining.min(visits);
    work.config_scan.leaves_remaining = work.config_scan.leaves_remaining.min(visits);
    work.lifecycle.pending_entries_remaining = work
        .lifecycle
        .pending_entries_remaining
        .min(usize::try_from(visits).unwrap_or(usize::MAX));
    work.lifecycle.lifecycle.pending_entries_remaining = work
        .lifecycle
        .lifecycle
        .pending_entries_remaining
        .min(usize::try_from(visits).unwrap_or(usize::MAX));
    work.inventory_directory.pending_entries_remaining = work
        .inventory_directory
        .pending_entries_remaining
        .min(usize::try_from(visits).unwrap_or(usize::MAX));
    work.invocation.pending_entries = work
        .invocation
        .pending_entries
        .min(usize::try_from(visits).unwrap_or(usize::MAX));
    current.values.io.read_bytes_remaining = current.values.io.read_bytes_remaining.min(bytes);
    current.values.io.write_bytes_remaining = current.values.io.write_bytes_remaining.min(bytes);
    parent.values.io.read_bytes_remaining = parent.values.io.read_bytes_remaining.min(bytes);
    parent.values.io.write_bytes_remaining = parent.values.io.write_bytes_remaining.min(bytes);
    current.key_work.bytes_remaining = current.key_work.bytes_remaining.min(bytes);
    parent.key_work.bytes_remaining = parent.key_work.bytes_remaining.min(bytes);
    work.lifecycle.factory.scan.read_bytes_remaining =
        work.lifecycle.factory.scan.read_bytes_remaining.min(bytes);
    work.lifecycle.industry.scan.read_bytes_remaining =
        work.lifecycle.industry.scan.read_bytes_remaining.min(bytes);
    work.lifecycle.native_scan.read_bytes_remaining =
        work.lifecycle.native_scan.read_bytes_remaining.min(bytes);
    work.authority_scan.read_bytes_remaining = work.authority_scan.read_bytes_remaining.min(bytes);
    work.lifecycle.bytes_remaining = work.lifecycle.bytes_remaining.min(bytes);
    work.lifecycle.lifecycle.comparison_bytes_remaining = work
        .lifecycle
        .lifecycle
        .comparison_bytes_remaining
        .min(bytes);
    work.builds.comparison_bytes_remaining = work.builds.comparison_bytes_remaining.min(bytes);
    work.receipts.bytes_remaining = work.receipts.bytes_remaining.min(bytes);
    work.authority.bytes_remaining = work.authority.bytes_remaining.min(bytes);
    work.factory.bytes_remaining = work.factory.bytes_remaining.min(bytes);
    work.logistics.bytes_remaining = work.logistics.bytes_remaining.min(bytes);
    work.inventory_directory.comparison_bytes_remaining = work
        .inventory_directory
        .comparison_bytes_remaining
        .min(bytes);
    work.material_profiles.scan.read_bytes_remaining =
        work.material_profiles.scan.read_bytes_remaining.min(bytes);
    work.invocation.instance_scan.read_bytes_remaining = work
        .invocation
        .instance_scan
        .read_bytes_remaining
        .min(bytes);
    work.invocation.registry_scan.read_bytes_remaining = work
        .invocation
        .registry_scan
        .read_bytes_remaining
        .min(bytes);
    work.invocation.comparison_bytes_remaining =
        work.invocation.comparison_bytes_remaining.min(bytes);
    work.config_scan.read_bytes_remaining = work.config_scan.read_bytes_remaining.min(bytes);
    let before_steps = [
        current.values.io.nodes_remaining,
        parent.values.io.nodes_remaining,
        work.lifecycle.steps_remaining,
        work.lifecycle.factory.cells_remaining,
        work.lifecycle.factory.scan.nodes_remaining,
        work.lifecycle.factory.scan.leaves_remaining,
        work.lifecycle.industry.cells_remaining,
        work.lifecycle.industry.scan.nodes_remaining,
        work.lifecycle.industry.scan.leaves_remaining,
        work.lifecycle.lifecycle.cells_remaining,
        work.lifecycle.native_scan.nodes_remaining,
        work.lifecycle.native_scan.leaves_remaining,
        work.lifecycle.native_read.json.cells_remaining,
        work.lifecycle.native_read.json.json_nodes_remaining,
        work.builds.cells_remaining,
        work.receipts.cells_remaining,
        work.authority.cells_remaining,
        work.authority_cells,
        work.authority_scan.nodes_remaining,
        work.authority_scan.leaves_remaining,
        work.factory.cells_remaining,
        work.logistics.cells_remaining,
        work.inventory_directory.visits_remaining,
        work.inventory_family.records_remaining,
        work.inventory_family.index_steps_remaining,
        work.material_profiles.scan.nodes_remaining,
        work.material_profiles.scan.leaves_remaining,
        work.material_profiles.cells_remaining,
        work.invocation.instance_scan.nodes_remaining,
        work.invocation.instance_scan.leaves_remaining,
        work.invocation.registry_scan.nodes_remaining,
        work.invocation.registry_scan.leaves_remaining,
        work.config.cells_remaining,
        work.config.json.cells_remaining,
        work.config.json.json_nodes_remaining,
        work.config_scan.nodes_remaining,
        work.config_scan.leaves_remaining,
        work.lifecycle.pending_entries_remaining as u64,
        work.lifecycle.lifecycle.pending_entries_remaining as u64,
        work.inventory_directory.pending_entries_remaining as u64,
        work.invocation.pending_entries as u64,
    ];
    let before_bytes = [
        current.values.io.read_bytes_remaining,
        current.values.io.write_bytes_remaining,
        parent.values.io.read_bytes_remaining,
        parent.values.io.write_bytes_remaining,
        current.key_work.bytes_remaining,
        parent.key_work.bytes_remaining,
        work.lifecycle.factory.scan.read_bytes_remaining,
        work.lifecycle.industry.scan.read_bytes_remaining,
        work.lifecycle.native_scan.read_bytes_remaining,
        work.authority_scan.read_bytes_remaining,
        work.lifecycle.bytes_remaining,
        work.lifecycle.lifecycle.comparison_bytes_remaining,
        work.builds.comparison_bytes_remaining,
        work.receipts.bytes_remaining,
        work.authority.bytes_remaining,
        work.factory.bytes_remaining,
        work.logistics.bytes_remaining,
        work.inventory_directory.comparison_bytes_remaining,
        work.material_profiles.scan.read_bytes_remaining,
        work.invocation.instance_scan.read_bytes_remaining,
        work.invocation.registry_scan.read_bytes_remaining,
        work.invocation.comparison_bytes_remaining,
        work.config_scan.read_bytes_remaining,
    ];
    let result = {
        let stop = std::cell::RefCell::new(stop);
        let mut cs = || (current.cancelled)() || (stop.borrow_mut())();
        let mut ps = || (parent.cancelled)() || (stop.borrow_mut())();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut *current.transaction,
            values: ValueReadContext {
                source: current.values.source,
                memory: current.values.memory,
                class: current.values.class,
                io: &mut *current.values.io,
            },
            key_work: &mut *current.key_work,
            semantic_bytes: current.semantic_bytes,
            record_bytes: current.record_bytes,
            take_state_error: &mut current.take_state_error,
            cancelled: &mut cs as &mut dyn FnMut() -> bool,
        };
        let mut read = WorldFieldReadContextV1 {
            root: parent.root,
            state: parent.state,
            values: ValueReadContext {
                source: parent.values.source,
                memory: parent.values.memory,
                class: parent.values.class,
                io: &mut *parent.values.io,
            },
            key_work: &mut *parent.key_work,
            semantic_bytes: parent.semantic_bytes,
            record_bytes: parent.record_bytes,
            take_state_error: &mut parent.take_state_error,
            cancelled: &mut ps as &mut dyn FnMut() -> bool,
        };
        operation(&mut overlay, &mut read, work, module.as_deref_mut())
    };
    let after_steps = [
        current.values.io.nodes_remaining,
        parent.values.io.nodes_remaining,
        work.lifecycle.steps_remaining,
        work.lifecycle.factory.cells_remaining,
        work.lifecycle.factory.scan.nodes_remaining,
        work.lifecycle.factory.scan.leaves_remaining,
        work.lifecycle.industry.cells_remaining,
        work.lifecycle.industry.scan.nodes_remaining,
        work.lifecycle.industry.scan.leaves_remaining,
        work.lifecycle.lifecycle.cells_remaining,
        work.lifecycle.native_scan.nodes_remaining,
        work.lifecycle.native_scan.leaves_remaining,
        work.lifecycle.native_read.json.cells_remaining,
        work.lifecycle.native_read.json.json_nodes_remaining,
        work.builds.cells_remaining,
        work.receipts.cells_remaining,
        work.authority.cells_remaining,
        work.authority_cells,
        work.authority_scan.nodes_remaining,
        work.authority_scan.leaves_remaining,
        work.factory.cells_remaining,
        work.logistics.cells_remaining,
        work.inventory_directory.visits_remaining,
        work.inventory_family.records_remaining,
        work.inventory_family.index_steps_remaining,
        work.material_profiles.scan.nodes_remaining,
        work.material_profiles.scan.leaves_remaining,
        work.material_profiles.cells_remaining,
        work.invocation.instance_scan.nodes_remaining,
        work.invocation.instance_scan.leaves_remaining,
        work.invocation.registry_scan.nodes_remaining,
        work.invocation.registry_scan.leaves_remaining,
        work.config.cells_remaining,
        work.config.json.cells_remaining,
        work.config.json.json_nodes_remaining,
        work.config_scan.nodes_remaining,
        work.config_scan.leaves_remaining,
        work.lifecycle.pending_entries_remaining as u64,
        work.lifecycle.lifecycle.pending_entries_remaining as u64,
        work.inventory_directory.pending_entries_remaining as u64,
        work.invocation.pending_entries as u64,
    ];
    let after_bytes = [
        current.values.io.read_bytes_remaining,
        current.values.io.write_bytes_remaining,
        parent.values.io.read_bytes_remaining,
        parent.values.io.write_bytes_remaining,
        current.key_work.bytes_remaining,
        parent.key_work.bytes_remaining,
        work.lifecycle.factory.scan.read_bytes_remaining,
        work.lifecycle.industry.scan.read_bytes_remaining,
        work.lifecycle.native_scan.read_bytes_remaining,
        work.authority_scan.read_bytes_remaining,
        work.lifecycle.bytes_remaining,
        work.lifecycle.lifecycle.comparison_bytes_remaining,
        work.builds.comparison_bytes_remaining,
        work.receipts.bytes_remaining,
        work.authority.bytes_remaining,
        work.factory.bytes_remaining,
        work.logistics.bytes_remaining,
        work.inventory_directory.comparison_bytes_remaining,
        work.material_profiles.scan.read_bytes_remaining,
        work.invocation.instance_scan.read_bytes_remaining,
        work.invocation.registry_scan.read_bytes_remaining,
        work.invocation.comparison_bytes_remaining,
        work.config_scan.read_bytes_remaining,
    ];
    // Return only the parked slice; actual IO/codec use stays consumed.
    current.values.io.nodes_remaining =
        original_steps[0].saturating_sub(before_steps[0].saturating_sub(after_steps[0]));
    parent.values.io.nodes_remaining =
        original_steps[1].saturating_sub(before_steps[1].saturating_sub(after_steps[1]));
    work.lifecycle.steps_remaining =
        original_steps[2].saturating_sub(before_steps[2].saturating_sub(after_steps[2]));
    work.lifecycle.factory.cells_remaining =
        original_steps[3].saturating_sub(before_steps[3].saturating_sub(after_steps[3]));
    work.lifecycle.factory.scan.nodes_remaining =
        original_steps[4].saturating_sub(before_steps[4].saturating_sub(after_steps[4]));
    work.lifecycle.factory.scan.leaves_remaining =
        original_steps[5].saturating_sub(before_steps[5].saturating_sub(after_steps[5]));
    work.lifecycle.industry.cells_remaining =
        original_steps[6].saturating_sub(before_steps[6].saturating_sub(after_steps[6]));
    work.lifecycle.industry.scan.nodes_remaining =
        original_steps[7].saturating_sub(before_steps[7].saturating_sub(after_steps[7]));
    work.lifecycle.industry.scan.leaves_remaining =
        original_steps[8].saturating_sub(before_steps[8].saturating_sub(after_steps[8]));
    work.lifecycle.lifecycle.cells_remaining =
        original_steps[9].saturating_sub(before_steps[9].saturating_sub(after_steps[9]));
    work.lifecycle.native_scan.nodes_remaining =
        original_steps[10].saturating_sub(before_steps[10].saturating_sub(after_steps[10]));
    work.lifecycle.native_scan.leaves_remaining =
        original_steps[11].saturating_sub(before_steps[11].saturating_sub(after_steps[11]));
    work.lifecycle.native_read.json.cells_remaining =
        original_steps[12].saturating_sub(before_steps[12].saturating_sub(after_steps[12]));
    work.lifecycle.native_read.json.json_nodes_remaining =
        original_steps[13].saturating_sub(before_steps[13].saturating_sub(after_steps[13]));
    work.builds.cells_remaining =
        original_steps[14].saturating_sub(before_steps[14].saturating_sub(after_steps[14]));
    work.receipts.cells_remaining =
        original_steps[15].saturating_sub(before_steps[15].saturating_sub(after_steps[15]));
    work.authority.cells_remaining =
        original_steps[16].saturating_sub(before_steps[16].saturating_sub(after_steps[16]));
    work.authority_cells =
        original_steps[17].saturating_sub(before_steps[17].saturating_sub(after_steps[17]));
    work.authority_scan.nodes_remaining =
        original_steps[18].saturating_sub(before_steps[18].saturating_sub(after_steps[18]));
    work.authority_scan.leaves_remaining =
        original_steps[19].saturating_sub(before_steps[19].saturating_sub(after_steps[19]));
    work.factory.cells_remaining =
        original_steps[20].saturating_sub(before_steps[20].saturating_sub(after_steps[20]));
    work.logistics.cells_remaining =
        original_steps[21].saturating_sub(before_steps[21].saturating_sub(after_steps[21]));
    work.inventory_directory.visits_remaining =
        original_steps[22].saturating_sub(before_steps[22].saturating_sub(after_steps[22]));
    work.inventory_family.records_remaining =
        original_steps[23].saturating_sub(before_steps[23].saturating_sub(after_steps[23]));
    work.inventory_family.index_steps_remaining =
        original_steps[24].saturating_sub(before_steps[24].saturating_sub(after_steps[24]));
    work.material_profiles.scan.nodes_remaining =
        original_steps[25].saturating_sub(before_steps[25].saturating_sub(after_steps[25]));
    work.material_profiles.scan.leaves_remaining =
        original_steps[26].saturating_sub(before_steps[26].saturating_sub(after_steps[26]));
    work.material_profiles.cells_remaining =
        original_steps[27].saturating_sub(before_steps[27].saturating_sub(after_steps[27]));
    work.invocation.instance_scan.nodes_remaining =
        original_steps[28].saturating_sub(before_steps[28].saturating_sub(after_steps[28]));
    work.invocation.instance_scan.leaves_remaining =
        original_steps[29].saturating_sub(before_steps[29].saturating_sub(after_steps[29]));
    work.invocation.registry_scan.nodes_remaining =
        original_steps[30].saturating_sub(before_steps[30].saturating_sub(after_steps[30]));
    work.invocation.registry_scan.leaves_remaining =
        original_steps[31].saturating_sub(before_steps[31].saturating_sub(after_steps[31]));
    work.config.cells_remaining =
        original_steps[32].saturating_sub(before_steps[32].saturating_sub(after_steps[32]));
    work.config.json.cells_remaining =
        original_steps[33].saturating_sub(before_steps[33].saturating_sub(after_steps[33]));
    work.config.json.json_nodes_remaining =
        original_steps[34].saturating_sub(before_steps[34].saturating_sub(after_steps[34]));
    work.config_scan.nodes_remaining =
        original_steps[35].saturating_sub(before_steps[35].saturating_sub(after_steps[35]));
    work.config_scan.leaves_remaining =
        original_steps[36].saturating_sub(before_steps[36].saturating_sub(after_steps[36]));
    work.lifecycle.pending_entries_remaining = original_steps[37]
        .saturating_sub(before_steps[37].saturating_sub(after_steps[37]))
        as usize;
    work.lifecycle.lifecycle.pending_entries_remaining = original_steps[38]
        .saturating_sub(before_steps[38].saturating_sub(after_steps[38]))
        as usize;
    work.inventory_directory.pending_entries_remaining = original_steps[39]
        .saturating_sub(before_steps[39].saturating_sub(after_steps[39]))
        as usize;
    work.invocation.pending_entries = original_steps[40]
        .saturating_sub(before_steps[40].saturating_sub(after_steps[40]))
        as usize;
    current.values.io.read_bytes_remaining =
        original_bytes[0].saturating_sub(before_bytes[0].saturating_sub(after_bytes[0]));
    current.values.io.write_bytes_remaining =
        original_bytes[1].saturating_sub(before_bytes[1].saturating_sub(after_bytes[1]));
    parent.values.io.read_bytes_remaining =
        original_bytes[2].saturating_sub(before_bytes[2].saturating_sub(after_bytes[2]));
    parent.values.io.write_bytes_remaining =
        original_bytes[3].saturating_sub(before_bytes[3].saturating_sub(after_bytes[3]));
    current.key_work.bytes_remaining =
        original_bytes[4].saturating_sub(before_bytes[4].saturating_sub(after_bytes[4]));
    parent.key_work.bytes_remaining =
        original_bytes[5].saturating_sub(before_bytes[5].saturating_sub(after_bytes[5]));
    work.lifecycle.factory.scan.read_bytes_remaining =
        original_bytes[6].saturating_sub(before_bytes[6].saturating_sub(after_bytes[6]));
    work.lifecycle.industry.scan.read_bytes_remaining =
        original_bytes[7].saturating_sub(before_bytes[7].saturating_sub(after_bytes[7]));
    work.lifecycle.native_scan.read_bytes_remaining =
        original_bytes[8].saturating_sub(before_bytes[8].saturating_sub(after_bytes[8]));
    work.authority_scan.read_bytes_remaining =
        original_bytes[9].saturating_sub(before_bytes[9].saturating_sub(after_bytes[9]));
    work.lifecycle.bytes_remaining =
        original_bytes[10].saturating_sub(before_bytes[10].saturating_sub(after_bytes[10]));
    work.lifecycle.lifecycle.comparison_bytes_remaining =
        original_bytes[11].saturating_sub(before_bytes[11].saturating_sub(after_bytes[11]));
    work.builds.comparison_bytes_remaining =
        original_bytes[12].saturating_sub(before_bytes[12].saturating_sub(after_bytes[12]));
    work.receipts.bytes_remaining =
        original_bytes[13].saturating_sub(before_bytes[13].saturating_sub(after_bytes[13]));
    work.authority.bytes_remaining =
        original_bytes[14].saturating_sub(before_bytes[14].saturating_sub(after_bytes[14]));
    work.factory.bytes_remaining =
        original_bytes[15].saturating_sub(before_bytes[15].saturating_sub(after_bytes[15]));
    work.logistics.bytes_remaining =
        original_bytes[16].saturating_sub(before_bytes[16].saturating_sub(after_bytes[16]));
    work.inventory_directory.comparison_bytes_remaining =
        original_bytes[17].saturating_sub(before_bytes[17].saturating_sub(after_bytes[17]));
    work.material_profiles.scan.read_bytes_remaining =
        original_bytes[18].saturating_sub(before_bytes[18].saturating_sub(after_bytes[18]));
    work.invocation.instance_scan.read_bytes_remaining =
        original_bytes[19].saturating_sub(before_bytes[19].saturating_sub(after_bytes[19]));
    work.invocation.registry_scan.read_bytes_remaining =
        original_bytes[20].saturating_sub(before_bytes[20].saturating_sub(after_bytes[20]));
    work.invocation.comparison_bytes_remaining =
        original_bytes[21].saturating_sub(before_bytes[21].saturating_sub(after_bytes[21]));
    work.config_scan.read_bytes_remaining =
        original_bytes[22].saturating_sub(before_bytes[22].saturating_sub(after_bytes[22]));
    let delta = |before: &[u64], after: &[u64]| {
        before
            .iter()
            .zip(after)
            .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
    };
    let extra_after = module.as_deref().map(|m| m.io_snapshot_v1());
    if let (Some(original), Some(before), Some(after), Some(module)) = (
        module_original,
        module_before,
        extra_after,
        module.as_deref_mut(),
    ) {
        module.restore_io_v1(original, before, after)?;
    }
    let extra_delta = match (module_before, extra_after) {
        (Some((bs, bb)), Some((a, b))) => delta(&bs, &a).zip(delta(&bb, &b)),
        (None, None) => Some((0, 0)),
        _ => None,
    };
    let Some((extra_steps, extra_bytes)) = extra_delta else {
        return result.and(Err(invalid().into()));
    };
    let Some(steps) = delta(&before_steps, &after_steps).and_then(|n| n.checked_add(extra_steps))
    else {
        return result.and(Err(invalid().into()));
    };
    let Some(bytes) = delta(&before_bytes, &after_bytes).and_then(|n| n.checked_add(extra_bytes))
    else {
        return result.and(Err(invalid().into()));
    };
    let settled = lease
        .consume(steps, bytes)
        .map_err(|e| BodyMutationError::Original(Box::new(e)));
    lease.finish();
    // Preserve the original first error even if its attempted delta also exhausts
    // the meter. Panic/accounting corruption leaves the lease un-finished, zeroing
    // both balances through the Common implementation.
    let value = result?;
    settled?;
    common
        .consume_steps(0)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    if (current.cancelled)() || (parent.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(value)
}
