//! Narrow actual Current owner source for fixed consumers. DATA only.
use super::*;

/// The actual narrow sources of WorldState::resolved_agent_owner. This is a
/// borrowed data bundle, not a dispatch/permission capability.
pub(in crate::world) struct CurrentStateOwnerSourcesV1<'a, 'source> {
    pub(in crate::world) roots: &'a oasis7_formal_protocol::incremental_execution::RootSetV1,
    pub(in crate::world) orders: &'a ReservedValue<
        crate::world::incremental_factory_directory_orders::CurrentFactoryOrderDirectoryV1<'source>,
    >,
    pub(in crate::world) agents: &'a ReservedValue<
        oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    >,
    pub(in crate::world) native: &'a ReservedValue<
        oasis7_world_state::incremental_world_native::WorldNativeFieldsQualificationV1,
    >,
}
impl<'a, 'source> CurrentStateOwnerSourcesV1<'a, 'source> {
    pub(super) fn from_industrial(source: &IndustrialEligibilitySourcesV1<'a, 'source>) -> Self {
        Self {
            roots: source.roots,
            orders: source.orders,
            agents: source.agents,
            native: source.native,
        }
    }
}
impl OwnedCurrentIndustrialStateOwnerV1 {
    pub(in crate::world) fn primary_agent_id(&self) -> &str {
        &self.primary_agent_id
    }
    pub(in crate::world) fn player_id(&self) -> &str {
        &self.player_id
    }
    pub(in crate::world) fn source_id(&self) -> &str {
        &self.source_id
    }
    pub(in crate::world) fn public_key(&self) -> Option<&str> {
        self.public_key.as_deref()
    }
    pub(in crate::world) fn generation(&self) -> u64 {
        self.generation
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn resolve_current_state_owner_v1<M, R, E, C, P, V, ER, CR>(
    sources: &CurrentStateOwnerSourcesV1<'_, '_>,
    agent: &str,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>, IndustrialEligibilityErrorV1>
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
    let result = resolve_current_industrial_state_owner_inner(
        sources,
        agent,
        overlay,
        parent_read,
        work,
        guard,
        live,
    );
    poison_on_error(&mut overlay.transaction, result.is_err());
    result.map_err(Into::into)
}
