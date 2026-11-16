//! Fixed complete selected BindRoles source; the existing Common bank owns every loan.
use super::*;
use crate::world::incremental_normal_action_factory_schedule::{
    FactoryScheduleReadErrorV1, with_module_lifecycle_read_io_v1,
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_module_release_roles_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    subject: &str,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    ReservedValue<
        oasis7_world_state::incremental_world_collection_controls::CurrentModuleReleaseRolesV1,
    >,
    FactoryScheduleReadErrorV1,
> {
    with_module_lifecycle_read_io_v1(
        sources.families.roots,
        ctx,
        parent,
        factory,
        common,
        module,
        |ctx, parent, _, module| {
            let mut read = WorldFieldReadContextV1 {
                root: sources
                    .families
                    .roots
                    .state_root(StateNamespaceV1::Authorization),
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
            Ok(oasis7_world_state::incremental_world_collection_controls::read_current_module_release_roles_v1(sources.controls,sources.families.roots,subject,ctx.transaction,&mut read,module.controls,module.pending_entries,&guard,live).map_err(overlay_error)?)
        },
    )
}
