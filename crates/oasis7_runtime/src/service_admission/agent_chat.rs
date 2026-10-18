//! Canonical owner chat. Economic grants and provider delegation do not grant control.
use crate::{AgentIntentAuthorityContext, World};
use oasis7_proto::{player_auth::verify_agent_chat_auth_proof, viewer::AgentChatRequest};

pub fn authority(request: &AgentChatRequest) -> AgentIntentAuthorityContext {
    AgentIntentAuthorityContext {
        intent_tick: request.intent_tick,
        world_id: request.world_id.clone(),
        reorg_epoch: request.reorg_epoch,
        authority_scope: request.authority_scope.clone(),
        replaces_intent_id: request.replaces_intent_id.clone(),
    }
}

/// Authentication/identity admission only: durable correlation replay is checked
/// before mutable replacement/nonce fences, but never bypasses current ownership.
pub fn validate_owner(world: &World, request: &AgentChatRequest) -> Result<(), String> {
    let proof = request
        .auth
        .as_ref()
        .ok_or("canonical chat proof required")?;
    let verified = verify_agent_chat_auth_proof(request, proof)?;
    let identity = world
        .capability_revocation_state()
        .agent_identities
        .get(&request.agent_id)
        .ok_or("canonical Agent identity unavailable")?;
    let extension = request
        .canonical_authority
        .as_ref()
        .ok_or("canonical chat authority required")?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    if verified.player_id != identity.owner_binding
        || verified.public_key
            != super::agent_authority::owner_public_key(world, &request.agent_id)?
        || extension.agent_identity_generation != identity.generation
        || extension.branch_id != binding.branch_id
        || request.world_id.as_deref() != Some(binding.world_id.as_str())
        || request.reorg_epoch != Some(binding.reorg_epoch)
        || request.authority_scope.as_deref() != Some("player_agent_chat")
        || request.intent_seq.is_none_or(|seq| seq == 0)
        || request.intent_tick.is_none()
    {
        return Err("canonical chat owner, world, branch or identity fence mismatch".into());
    }
    Ok(())
}

pub fn validate_fresh(world: &World, request: &AgentChatRequest) -> Result<(), String> {
    validate_owner(world, request)?;
    let proof = request
        .auth
        .as_ref()
        .ok_or("canonical chat proof required")?;
    let retained_nonce =
        super::retained_queries::owner_chat_nonce_high_water(world, &proof.public_key)?;
    if retained_nonce.is_some_and(|nonce| nonce >= proof.nonce)
        || world.capability_revocation_state().world_service_results.values().any(|value| {
        serde_json::from_value::<super::CanonicalIntentResultV1>(value.clone()).is_ok_and(|result| {
            matches!(&result.request.signed_payload, super::WorldServicePayloadV1::AgentChat(old)
                if old.auth.as_ref().is_some_and(|auth| auth.public_key == proof.public_key && auth.nonce >= proof.nonce))
        })
    }) {
        return Err("canonical chat nonce did not advance".into());
    }
    let expected = world
        .state()
        .agents
        .get(&request.agent_id)
        .and_then(|agent| agent.intent.as_ref())
        .filter(|intent| {
            !matches!(
                intent.status.as_str(),
                "completed" | "rejected" | "expired" | "cancelled" | "superseded"
            )
        })
        .map(|intent| intent.intent_id.as_str());
    if request.replaces_intent_id.as_deref() != expected {
        return Err("canonical chat current intent replacement mismatch".into());
    }
    if world
        .agent_chat_intent_replay_disposition(
            proof.player_id.as_str(),
            request.agent_id.as_str(),
            request.intent_seq.unwrap(),
            request.message.as_str(),
            authority(request),
        )
        .map_err(|error| format!("{error:?}"))?
        .is_some()
    {
        return Err("canonical chat intent sequence already has a committed request".into());
    }
    // Validate the original intent kernel's bound/transition checks without mutation.
    let mut candidate = world.clone();
    candidate
        .record_agent_chat_intent_with_authority(
            proof.player_id.as_str(),
            request.agent_id.as_str(),
            request.intent_seq.unwrap(),
            request.message.as_str(),
            authority(request),
        )
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

impl World {
    pub fn apply_authenticated_agent_chat(
        &mut self,
        request: &AgentChatRequest,
    ) -> Result<serde_json::Value, String> {
        validate_fresh(self, request)?;
        let proof = request
            .auth
            .as_ref()
            .ok_or("canonical chat proof required")?;
        self.record_agent_chat_intent_with_authority(
            proof.player_id.as_str(),
            request.agent_id.as_str(),
            request.intent_seq.unwrap(),
            request.message.as_str(),
            authority(request),
        )
        .map_err(|e| format!("{e:?}"))?;
        let disposition = self
            .agent_chat_intent_replay_disposition(
                proof.player_id.as_str(),
                request.agent_id.as_str(),
                request.intent_seq.unwrap(),
                request.message.as_str(),
                authority(request),
            )
            .map_err(|e| format!("{e:?}"))?
            .ok_or("canonical intent disposition missing")?;
        serde_json::to_value(disposition).map_err(|e| e.to_string())
    }
}

/// Recover the goal only from the original successful canonical result bound to
/// the current intent. Raw player text never enters the sanitized public events.
pub fn owner_view(
    world: &World,
    agent: &str,
) -> Result<Option<oasis7_proto::viewer::CanonicalAgentChatViewV1>, String> {
    use oasis7_proto::viewer::*;
    let Some(identity) = world
        .capability_revocation_state()
        .agent_identities
        .get(agent)
    else {
        return Ok(None);
    };
    let Ok(public_key) = super::agent_authority::owner_public_key(world, agent) else {
        return Ok(None);
    };
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("{e:?}"))?;
    let current = world
        .state()
        .agents
        .get(agent)
        .and_then(|cell| cell.intent.as_ref());
    let current_intent_id = current
        .filter(|intent| {
            !matches!(
                intent.status.as_str(),
                "completed" | "rejected" | "expired" | "cancelled" | "superseded"
            )
        })
        .map(|intent| intent.intent_id.clone());
    let mut goal = None;
    if let Some(intent) = current {
        for result in
            super::retained_queries::agent_chat_goal_results(world, agent, &intent.intent_id)?
                .into_values()
        {
            if result.rejected.is_some()
                || result
                    .receipt
                    .get("intent_id")
                    .and_then(serde_json::Value::as_str)
                    != Some(intent.intent_id.as_str())
            {
                continue;
            }
            let super::WorldServicePayloadV1::AgentChat(request) = result.request.signed_payload
            else {
                continue;
            };
            if request.agent_id != agent || validate_owner(world, &request).is_err() {
                continue;
            }
            if goal.is_some() {
                return Err("ambiguous canonical Agent goal result".into());
            }
            goal = Some(CanonicalAgentGoalV1 {
                intent_id: intent.intent_id.clone(),
                message: request.message,
                status: intent.status.clone(),
                event_seq: intent.event_seq,
                logical_time: intent.logical_time,
            });
        }
    }
    Ok(Some(CanonicalAgentChatViewV1 {
        agent_id: agent.into(),
        player_id: identity.owner_binding.clone(),
        public_key,
        world_id: binding.world_id,
        reorg_epoch: binding.reorg_epoch,
        authority_scope: "player_agent_chat".into(),
        canonical_authority: CanonicalAgentChatAuthorityV1 {
            branch_id: binding.branch_id,
            agent_identity_generation: identity.generation,
        },
        current_intent_id,
        goal,
    }))
}

#[cfg(test)]
#[path = "agent_chat_tests.rs"]
mod tests;
