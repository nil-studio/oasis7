//! Exact original gameplay effect for zero-height crash/recovery fixtures.
use super::*;
pub(super) fn first_agent_effect(
    world: &oasis7_runtime::World,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<Vec<u8>, String> {
    let WorldServicePayloadV1::GameplayJson(bytes) = &request.signed_payload else {
        return Err("first-agent fixture needs actual GameplayJson execution".into());
    };
    let action: oasis7::viewer::GameplayActionRequest =
        serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let verified = oasis7::viewer::verify_gameplay_action_auth_proof(
        &action,
        action
            .auth
            .as_ref()
            .ok_or("first-agent signature missing")?,
    )?;
    if action.action_id != oasis7::viewer::ACTION_CLAIM_FIRST_AGENT
        || action.target_agent_id != oasis7::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID
        || verified.player_id != "owner"
        || Some(&verified.public_key) != action.public_key.as_ref()
        || world.state().agents.len() != 1
    {
        return Err("first-agent signed owner/target/exact registration mismatch".into());
    }
    let agent = world
        .state()
        .agents
        .get(&action.target_agent_id)
        .ok_or("original first agent absent")?;
    let oasis7_runtime::Action::RegisterAgent { agent_id, pos } =
        oasis7::viewer::build_runtime_action_from_gameplay_request(&action)
            .map_err(|e| e.message)?
    else {
        return Err("first-agent request did not produce RegisterAgent".into());
    };
    if agent.state.agent_id != agent_id || agent.state.pos != pos {
        return Err("original first-agent identity/spawn differs".into());
    }
    // AgentState includes exact body and ResourceStock. The signed request is
    // the player/key authority for RegisterAgent; it does not create a separate
    // GameTask or durable CapabilityAgentIdentity ownership grant.
    let result_key =
        correlation::key_digest(&request.correlation.key).map_err(|e| e.to_string())?;
    let result = retained_result_from_hot_or_cold(world, &result_key)?;
    serde_cbor::to_vec(&(agent, &verified.player_id, &verified.public_key, result))
        .map_err(|e| e.to_string())
}
