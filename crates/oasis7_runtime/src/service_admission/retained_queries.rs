//! Bounded hot/cold result access preserving the original authorization filters.
use super::{CanonicalIntentResultV1, agent_authority, correlation};
use crate::{
    AgentChatIntentLookupV1, GameTaskControlLookupV1, World, retained_history_digest_text,
};
use std::collections::BTreeMap;

pub(crate) fn hot_results(
    world: &World,
) -> Result<BTreeMap<String, CanonicalIntentResultV1>, String> {
    world
        .capability_revocation_state()
        .world_service_results
        .iter()
        .map(|(key, value)| {
            correlation::validate_result(key, value)?;
            let result =
                serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
            Ok((key.clone(), result))
        })
        .collect()
}
fn merge(
    results: &mut BTreeMap<String, CanonicalIntentResultV1>,
    cold: Vec<CanonicalIntentResultV1>,
) -> Result<(), String> {
    for result in cold {
        let key = correlation::key_digest(&result.request.correlation.key)?;
        if let Some(hot) = results.get(&key) {
            if hot != &result {
                return Err("hot/cold canonical result conflict".into());
            }
        } else {
            results.insert(key, result);
        }
    }
    Ok(())
}
pub(crate) fn owner_chat_nonce_high_water(
    world: &World,
    public_key: &str,
) -> Result<Option<u64>, String> {
    if world.state().retained_history_root.is_none() {
        return Ok(None);
    }
    world
        .read_retained_owner_chat_nonce_high_water(public_key)
        .map_err(|error| error.to_string())
}
pub(crate) fn agent_chat_goal_results(
    world: &World,
    agent: &str,
    intent_id: &str,
) -> Result<BTreeMap<String, CanonicalIntentResultV1>, String> {
    let mut results = hot_results(world)?;
    if world.state().retained_history_root.is_some() {
        let identity = world
            .capability_revocation_state()
            .agent_identities
            .get(agent)
            .ok_or("retained chat owner identity unavailable")?;
        let binding = world
            .current_cognition_runtime_binding()
            .map_err(|error| format!("{error:?}"))?;
        let public_key = agent_authority::owner_public_key(world, agent)?;
        let lookup = AgentChatIntentLookupV1 {
            agent_id: agent.into(),
            intent_id: intent_id.into(),
            owner_public_key_digest: retained_history_digest_text("owner-public-key", &public_key)?,
            player_id: identity.owner_binding.clone(),
            world_id: binding.world_id,
            branch_id: binding.branch_id,
            identity_generation: identity.generation,
            reorg_epoch: binding.reorg_epoch,
            authority_scope: "player_agent_chat".into(),
        };
        merge(
            &mut results,
            world
                .read_retained_agent_chat_intent_results(&lookup)
                .map_err(|error| error.to_string())?,
        )?;
    }
    Ok(results)
}
pub(crate) fn game_task_results(
    world: &World,
    agent: &str,
) -> Result<BTreeMap<String, CanonicalIntentResultV1>, String> {
    let mut results = hot_results(world)?;
    if world.state().retained_history_root.is_some() {
        let identity = world
            .capability_revocation_state()
            .agent_identities
            .get(agent)
            .ok_or("retained Game task owner identity unavailable")?;
        let binding = world
            .current_cognition_runtime_binding()
            .map_err(|error| format!("{error:?}"))?;
        let public_key = agent_authority::owner_public_key(world, agent)?;
        let lookup = GameTaskControlLookupV1 {
            agent_id: agent.into(),
            owner_binding: identity.owner_binding.clone(),
            owner_generation: identity.generation,
            owner_public_key_digest: retained_history_digest_text("owner-public-key", &public_key)?,
            world_id: binding.world_id,
            genesis_digest: world
                .chain_resource_manifest()
                .genesis_ref
                .clone()
                .ok_or("retained Game task genesis unavailable")?,
            branch_id: binding.branch_id,
            reorg_epoch: binding.reorg_epoch,
        };
        merge(
            &mut results,
            world
                .read_retained_game_task_control_results(&lookup)
                .map_err(|error| error.to_string())?,
        )?;
    }
    Ok(results)
}
