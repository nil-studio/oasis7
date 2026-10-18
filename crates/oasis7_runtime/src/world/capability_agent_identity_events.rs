//! Replay validation for canonical Agent owner identity generations.
use super::{CapabilityAgentIdentity, World, WorldError, deny};

pub(super) fn apply_agent_identity(
    world: &mut World,
    agent_id: &str,
    identity: &CapabilityAgentIdentity,
) -> Result<(), WorldError> {
    super::super::capability_authorization::validate_agent_identity(agent_id, identity)?;
    let Some(agent) = world.state.agents.get(agent_id) else {
        return Err(deny("capability agent identity requires a live agent"));
    };
    if agent.state.agent_id != agent_id {
        return Err(deny("live agent state id does not match its registry key"));
    }
    if let Some(existing) = world
        .capability_revocation_state
        .agent_identities
        .get(agent_id)
    {
        if identity.generation < existing.generation {
            return Err(deny("capability agent identity generation regressed"));
        }
        if identity.generation == existing.generation && existing != identity {
            return Err(deny(
                "capability agent identity changed without a new generation",
            ));
        }
    }
    world
        .capability_revocation_state
        .agent_identities
        .insert(agent_id.to_string(), identity.clone());
    Ok(())
}
