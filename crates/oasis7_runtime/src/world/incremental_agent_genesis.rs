//! Runtime World adapter for the sole original borrowed Agent producer.
use super::{World, incremental_body::BodyMutationError};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1 as Ns, incremental_value::ValueNodeWriter,
};
use oasis7_world_state::DomainEvent;
pub(super) use oasis7_world_state::incremental_agent_genesis::write_agent_cell_fields;
pub use oasis7_world_state::incremental_agent_genesis::{
    AgentGenesisContext, write_agent_scalar_value,
};
impl World {
    /// Emit one borrowed agent's full subfield component for a new world. A
    /// successful count is not a complete world, closure receipt or activation.
    pub fn write_iws_agent_genesis_fields<
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(Ns, &[u8], [u8; 32]) -> Result<(), BodyMutationError>,
        L: FnMut(u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
    >(
        &self,
        agent: &str,
        context: &mut AgentGenesisContext<'_, W, C, F, L>,
    ) -> Result<u64, BodyMutationError> {
        let cell = self
            .state
            .agents
            .get(agent)
            .ok_or(BodyMutationError::Invalid("genesis agent absent"))?;
        write_agent_cell_fields(agent, cell, context)
    }
}
