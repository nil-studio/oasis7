use super::*;
use crate::viewer::runtime_live::control_plane::provider_action_commit::provider_cognition_commit_inputs;
#[cfg(any(test, feature = "test_tier_required"))]
use crate::viewer::runtime_live::control_plane::provider_action_commit::{
    ProviderServiceCognitionReceipt, validate_provider_service_receipt,
};

pub(in crate::viewer::runtime_live) fn validate_original_memory_checkpoint(
    world: &RuntimeWorld,
    pending: &lineage_persistence::PendingProviderServiceIntent,
) -> Result<(), String> {
    let crate::world_service::wire::WorldServicePayloadV1::Cognition(signed) = &pending.payload
    else {
        return Err("restored memory checkpoint operation mismatch".into());
    };
    if crate::world_service::derive_correlation(
        pending.correlation.key.world.clone(),
        &pending.payload,
    )? != pending.correlation
    {
        return Err("restored memory checkpoint correlation mismatch".into());
    }
    let cognition = &pending.cognition;
    let request = &cognition.request.request_context;
    let turn = &cognition.request.turn_context;
    request
        .validate_production_lane()
        .map_err(|error| error.to_string())?;
    turn.validate_for_agent(&request.agent_subject)
        .map_err(|error| error.to_string())?;
    if turn.agent_session_id != request.agent_session_id
        || turn.agent_turn_id != request.agent_turn_id
        || turn.decision_request_id != request.decision_request_id
        || turn.request_digest != request.request_digest
        || crate::simulator::Digest32::from(turn.memory_snapshot.digest.clone())
            != request.memory_snapshot_digest
        || crate::simulator::Digest32::from(turn.goal_snapshot.digest.clone())
            != request.goal_snapshot_digest
        || crate::simulator::h_v1("oasis7.cognition.continuation.v1", &turn.continuation)
            != request.continuation_digest
        || serde_json::to_value(&cognition.memory_write_intents)
            .map_err(|error| error.to_string())?
            != serde_json::to_value(
                &cognition
                    .response
                    .base_decision_response
                    .memory_write_intents,
            )
            .map_err(|error| error.to_string())?
    {
        return Err("restored memory original context or captured intents mismatch".into());
    }
    let (mut original, artifact) = provider_cognition_commit_inputs(world, cognition)?;
    // The canonical service supplies these authority roots, not the local shadow world.
    original.capability_snapshot_hash = signed.request.request.capability_snapshot_hash.clone();
    original.authority_context_hash = signed.request.request.authority_context_hash.clone();
    if serde_json::to_value(original).map_err(|error| error.to_string())?
        != serde_json::to_value(&signed.request.request).map_err(|error| error.to_string())?
        || serde_json::to_value(artifact).map_err(|error| error.to_string())?
            != serde_json::to_value(&signed.request.response_artifact)
                .map_err(|error| error.to_string())?
    {
        return Err("restored memory signed request or response artifact mismatch".into());
    }
    Ok(())
}
impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn validate_hosted_restored_original(
        &mut self,
        world: &RuntimeWorld,
        pending: &lineage_persistence::PendingProviderServiceIntent,
    ) -> Result<(), String> {
        let agent = &pending.cognition.request.request_context.agent_subject;
        let Some(restored) = self.provider_restored_service_checkpoints.get(agent) else {
            return Ok(());
        };
        let validation = (|| {
            // The restored Cognition/captured-memory authority is immutable.
            // ACK consumption and its delivery progress are later transitions,
            // not part of the original model request snapshot.
            let mut original = restored.clone();
            let mut current = pending.clone();
            let original_ack = original.feedback_ack.take();
            let current_ack = current.feedback_ack.take();
            if serde_json::to_value(&original).map_err(|error| error.to_string())?
                != serde_json::to_value(&current).map_err(|error| error.to_string())?
            {
                return Err("restored original memory checkpoint changed; fenced".into());
            }
            match (original_ack, current_ack) {
                (Some(mut original), Some(mut current)) => {
                    // These flags never authenticate delivery or network effects.
                    // All signed bytes, raw feedback, receipt and correlation stay exact.
                    original.delivered = false;
                    original.issued = false;
                    current.delivered = false;
                    current.issued = false;
                    if serde_json::to_value(original).map_err(|error| error.to_string())?
                        != serde_json::to_value(current).map_err(|error| error.to_string())?
                    {
                        return Err("restored original ACK checkpoint changed; fenced".into());
                    }
                    self.validate_pending_feedback_consumption(pending)?;
                }
                (None, Some(_)) => self.validate_pending_feedback_consumption(pending)?,
                (Some(_), None) => return Err("restored ACK consumption removed; fenced".into()),
                (None, None) => {}
            }
            validate_original_memory_checkpoint(world, pending)
        })();
        if validation.is_err() {
            #[cfg(any(test, feature = "test_tier_required"))]
            {
                self.hosted_service_memory_failure = Some("restored_memory_checkpoint_invalid");
            }
        }
        validation
    }
}
#[cfg(any(test, feature = "test_tier_required"))]
impl crate::viewer::ViewerRuntimeLiveServer {
    /// Pure gate probe: no live actor, sidecar, canonical state or recovery marker is changed.
    pub fn test_project_original_receipt_memory(
        pending: serde_json::Value,
        receipt: serde_json::Value,
    ) -> serde_json::Value {
        let mut store = crate::simulator::MemoryWriteStore::default();
        let result: Result<(), String> = (|| {
            let pending: lineage_persistence::PendingProviderServiceIntent =
                serde_json::from_value(pending).map_err(|error| error.to_string())?;
            let receipt: ProviderServiceCognitionReceipt =
                serde_json::from_value(receipt).map_err(|error| error.to_string())?;
            let world = crate::runtime::World::new();
            validate_original_memory_checkpoint(&world, &pending)?;
            validate_provider_service_receipt(&pending, &receipt)?;
            let authority =
                super::service_feedback_ack::VerifiedProviderServiceMemoryAuthority::new(
                    &pending, &receipt,
                )?;
            crate::runtime_host::feedback::project_receipt_memory(
                &pending.cognition.request.turn_context,
                &receipt.feedback,
                &receipt.lineage,
                &receipt,
                &authority,
                &pending.cognition.memory_write_intents,
                &mut store,
            )
            .map_err(|error| error.to_string())
        })();
        serde_json::json!({"accepted":result.is_ok(),"error_category":result.err().map(|_|"receipt_memory_gate_rejected"),"memory_store":store})
    }
}
