//! Shared prepared/replay transition for canonical owner identity generations.
use super::{CapabilityAgentIdentity, World, WorldError, deny};

pub(super) fn apply_agent_identity(
    world: &mut World,
    agent_id: &str,
    identity: &CapabilityAgentIdentity,
) -> Result<(), WorldError> {
    project_agent_identity(
        &world.state,
        &mut world.capability_revocation_state,
        agent_id,
        identity,
    )
}

pub(in crate::world) fn project_agent_identity(
    state: &crate::WorldState,
    revocation: &mut crate::capability_authorization::CapabilityRevocationState,
    agent_id: &str,
    identity: &CapabilityAgentIdentity,
) -> Result<(), WorldError> {
    super::super::capability_authorization::validate_agent_identity(agent_id, identity)?;
    if state.is_manufactured_agent(agent_id) {
        return Err(deny(
            "manufactured identity is derived from its claim and root",
        ));
    }
    let Some(agent) = state.agents.get(agent_id) else {
        return Err(deny("capability agent identity requires a live agent"));
    };
    if agent.state.agent_id != agent_id {
        return Err(deny("live agent state id does not match its registry key"));
    }
    let previous = revocation.agent_identities.get(agent_id);
    if let Some(existing) = previous {
        if identity.generation < existing.generation {
            return Err(deny("capability agent identity generation regressed"));
        }
        if identity.generation == existing.generation && existing != identity {
            return Err(deny(
                "capability agent identity changed without a new generation",
            ));
        }
    }
    // Validate every dependent counter before modifying either registry. Use
    // the supplied projected registry so sequential events in a batch compose.
    let mut checkpoints = Vec::new();
    if previous.is_some_and(|old| old != identity) {
        for (target, claim) in &state.agent_claims {
            let Some(source) = &claim.control_source else {
                continue;
            };
            if source.root_agent_id != agent_id || !state.is_manufactured_agent(target) {
                continue;
            }
            let sequence = *state
                .agent_claim_acquisition_sequences
                .get(target)
                .ok_or_else(|| deny("manufactured identity acquisition sequence missing"))?;
            if sequence != source.acquisition_sequence {
                return Err(deny("manufactured identity acquisition sequence mismatch"));
            }
            let generation = super::super::industrial_control::manufactured_identity_generation(
                revocation, target, sequence,
            )
            .and_then(|g| g.checked_add(1))
            .ok_or_else(|| deny("manufactured identity generation overflow"))?;
            checkpoints.push((
                target.clone(),
                sequence,
                CapabilityAgentIdentity {
                    owner_binding: source.player_id.clone(),
                    generation,
                },
            ));
        }
    }
    for (target, sequence, checkpoint) in checkpoints {
        revocation
            .manufactured_identity_acquisition_sequences
            .insert(target.clone(), sequence);
        revocation.agent_identities.insert(target, checkpoint);
    }
    revocation
        .agent_identities
        .insert(agent_id.to_string(), identity.clone());
    Ok(())
}
