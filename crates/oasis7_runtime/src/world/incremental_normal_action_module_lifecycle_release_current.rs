//! Sole fixed Current release-request reads through the existing Common IO loan.
use super::*;
use crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1;
use oasis7_formal_protocol::incremental_value::ValueNodeReader;

use super::super::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1;
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
use oasis7_formal_protocol::incremental_value::ValueReadContext;
use oasis7_world_state::incremental_field_member::FieldDestination;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
fn namespace(
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<StateNamespaceV1, BodyMutationError> {
    common
        .charge(24, 0)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let descriptor =
        oasis7_world_state::incremental_world_native::world_native_field_descriptors_v1()?
            .into_iter()
            .find(|f| f.name == "module_release_requests")
            .ok_or_else(invalid)?;
    let FieldDestination::State(ns) = descriptor.destination else {
        return Err(invalid());
    };
    Ok(ns)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_module_release_request_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    id: u64,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<
        oasis7_world_state::incremental_world_native::CurrentFactoryNativeRecordV1<
            oasis7_world_state::state::ModuleReleaseRequestState,
        >,
    >,
    FactoryScheduleReadErrorV1,
> {
    let ns = namespace(common)?;
    with_module_lifecycle_read_io_v1(
        sources.families.roots,
        ctx,
        parent,
        factory,
        common,
        module,
        |ctx, parent, factory, module| {
            let mut read = WorldFieldReadContextV1 {
                root: sources.families.roots.state_root(ns),
                state: parent.state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let guard = ctx.transaction.guard();
            Ok(oasis7_world_state::incremental_world_native::read_current_module_release_request_v1(id, sources.factory.lifecycle.native,sources.families.roots,ctx.transaction,&mut read,&mut factory.lifecycle.native_scan,&mut factory.lifecycle.native_read, &guard,live).map_err(overlay_error)?)
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_module_release_requests_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<oasis7_world_state::incremental_world_native::CurrentModuleReleaseRequestsV1>,
    FactoryScheduleReadErrorV1,
> {
    let ns = namespace(common)?;
    with_module_lifecycle_read_io_v1(
        sources.families.roots,
        ctx,
        parent,
        factory,
        common,
        module,
        |ctx, parent, factory, module| {
            let mut read = WorldFieldReadContextV1 {
                root: sources.families.roots.state_root(ns),
                state: parent.state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let guard = ctx.transaction.guard();
            Ok(oasis7_world_state::incremental_world_native::read_current_module_release_requests_v1( sources.factory.lifecycle.native,sources.families.roots,ctx.transaction,&mut read,&mut factory.lifecycle.native_scan,&mut factory.lifecycle.native_read,module.instance_cells,module.pending_entries, &guard,live).map_err(overlay_error)?)
        },
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_module_release_manifest_mapping_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    id: u64,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<
        oasis7_world_state::incremental_world_native::CurrentFactoryNativeRecordV1<
            oasis7_world_state::state::ModuleReleaseManifestMappingState,
        >,
    >,
    FactoryScheduleReadErrorV1,
> {
    let ns = mapping_namespace(common)?;
    with_module_lifecycle_read_io_v1(
        sources.families.roots,
        ctx,
        parent,
        factory,
        common,
        module,
        |ctx, parent, factory, module| {
            let mut read = WorldFieldReadContextV1 {
                root: sources.families.roots.state_root(ns),
                state: parent.state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let guard = ctx.transaction.guard();
            Ok(oasis7_world_state::incremental_world_native::read_current_module_release_manifest_mapping_v1(id, sources.factory.lifecycle.native,sources.families.roots,ctx.transaction,&mut read,&mut factory.lifecycle.native_scan,&mut factory.lifecycle.native_read, &guard,live).map_err(overlay_error)?)
        },
    )
}

fn mapping_namespace(
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<StateNamespaceV1, BodyMutationError> {
    common
        .charge(24, 0)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let descriptor =
        oasis7_world_state::incremental_world_native::world_native_field_descriptors_v1()?
            .into_iter()
            .find(|f| f.name == "module_release_manifest_mappings")
            .ok_or_else(invalid)?;
    let FieldDestination::State(ns) = descriptor.destination else {
        return Err(invalid());
    };
    Ok(ns)
}
