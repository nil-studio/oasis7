//! Two closed source selection routes sharing the same actual codec and original kernel.
use super::*;
/// Original due/command instance selection and suppression semantics.
#[expect(
    clippy::too_many_arguments,
    reason = "Actual source contexts and continuous finite work remain explicit."
)]
pub fn prepare_root_module_invocation_v1<M, R, E, C, O>(
    request: ModuleInvocationPreparationRequestV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    originals: &mut O,
    original_work: &mut RuntimeArtifactOriginalWorkV1,
    limits: &mut ModuleInvocationPreparationLimitsV1,
) -> Result<Option<PreparedRootModuleInvocationV1>, ModuleInvocationPreparationErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    prepare_root_module_invocation_route_v1(
        InvocationRouteV1::InstanceOrActive,
        request,
        state,
        ctx,
        scan,
        work,
        originals,
        original_work,
        limits,
    )
}
/// Pure policy-hook active registry selection, independent of installed instances.
#[expect(
    clippy::too_many_arguments,
    reason = "Actual source contexts and continuous finite work remain explicit."
)]
pub fn prepare_root_module_registry_invocation_v1<M, R, E, C, O>(
    request: ModuleInvocationPreparationRequestV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ArtifactConfigReadWorkV1,
    originals: &mut O,
    original_work: &mut RuntimeArtifactOriginalWorkV1,
    limits: &mut ModuleInvocationPreparationLimitsV1,
) -> Result<Option<PreparedRootModuleInvocationV1>, ModuleInvocationPreparationErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    prepare_root_module_invocation_route_v1(
        InvocationRouteV1::RegistryActive,
        request,
        state,
        ctx,
        scan,
        work,
        originals,
        original_work,
        limits,
    )
}
