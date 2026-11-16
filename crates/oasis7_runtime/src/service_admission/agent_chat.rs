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
    readonly_rules::flatten_legacy(readonly_rules::validate_owner_with_facts_v1(&readonly_rules::Legacy(world),request))
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
    readonly_rules::flatten_legacy(readonly_rules::owner_view_with_facts_v1(&readonly_rules::Legacy(world),agent))
}
#[path="agent_chat_readonly.rs"]
pub(crate) mod readonly_rules;

#[cfg(test)]
#[path = "agent_chat_tests.rs"]
mod tests;
