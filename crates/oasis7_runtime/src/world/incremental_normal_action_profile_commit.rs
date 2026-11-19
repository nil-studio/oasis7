//! Actual Current CAS through the original material/native profile codecs.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn apply<M, P, V, W, E, C, ME, D>(
    event: &crate::DomainEvent,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &ProfileGovernanceSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut ProfileGovernanceWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
{
    let guard = step.transaction.guard();
    materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
    let stop = RefCell::new(&mut read.cancelled);
    let mutation = RefCell::new(&mut step.mutation.cancelled);
    let mut write = MemberRecordWriteContext {
        sink: &mut *step.mutation.sink,
        memory: step.mutation.memory,
        io: &mut *step.mutation.io,
        key_work: &mut *materials.write_keys,
        limits: materials.write_limits,
        cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
    };
    if let crate::DomainEvent::MaterialProfileGoverned { profile, .. } = event {
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: ValueReadContext {
                source: read.values.source,
                memory: step.mutation.memory,
                class: MemoryClass::Ordinary,
                io: &mut *materials.read_io,
            },
            key_work: &mut *materials.read_keys,
            semantic_bytes: materials.semantic_bytes,
            record_bytes: materials.record_bytes,
            take_state_error: &mut materials.take_state_error,
            cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
        };
        let guard = ctx.transaction.guard();
        return oasis7_world_state::incremental_material_records::upsert_current_material_profile_v1(profile,sources.materials,&mut ctx,read.state,&mut write,&mut work.material,&mut work.material_scan,&guard,live).map_err(overlay_error);
    }
    let roots = materials.roots;
    let mut ctx = WorldFieldReadContextV1 {
        root: roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: MemoryClass::Ordinary,
            io: &mut *materials.read_io,
        },
        key_work: &mut *materials.read_keys,
        semantic_bytes: materials.semantic_bytes,
        record_bytes: materials.record_bytes,
        take_state_error: &mut materials.take_state_error,
        cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
    };
    macro_rules! publish {
        ($profile:expr,$id:expr,$read:ident,$write:ident) => {{
            let guard = step.transaction.guard();
            let current = oasis7_world_state::incremental_world_native::$read(
                $id,
                sources.native,
                roots,
                step.transaction,
                &mut ctx,
                &mut work.shared.native_scan,
                &mut work.shared.native_read,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            oasis7_world_state::incremental_world_native::$write(
                &current,
                $id,
                $profile,
                step.transaction,
                &mut ctx,
                &mut write,
                &mut work.shared.records_remaining,
                &mut work.shared.native_write,
                live,
            )
            .map_err(overlay_error)?;
        }};
    }
    match event {
        crate::DomainEvent::ProductProfileGoverned { profile, .. } => publish!(
            profile,
            &profile.product_id,
            read_current_governed_product_profile_v1,
            write_current_governed_product_profile_v1
        ),
        crate::DomainEvent::RecipeProfileGoverned { profile, .. } => publish!(
            profile,
            &profile.recipe_id,
            read_current_governed_recipe_profile_v1,
            write_current_governed_recipe_profile_v1
        ),
        crate::DomainEvent::FactoryProfileGoverned { profile, .. } => publish!(
            profile,
            &profile.factory_id,
            read_current_governed_factory_profile_v1,
            write_current_governed_factory_profile_v1
        ),
        _ => return Err(BodyMutationError::Invalid("profile publication domain")),
    }
    Ok(())
}
