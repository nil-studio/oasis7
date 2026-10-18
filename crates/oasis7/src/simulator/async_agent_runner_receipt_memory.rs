//! Shared receipt-gated projection; no actor identity or awaiting outcome is synthesized.
use super::*;

pub fn project_receipt_memory<A: RuntimeAuthority>(
    context: &ContinuousAgentTurnContextV1,
    feedback: &FeedbackEnvelopeV1,
    runtime_receipt: &RuntimeReceiptLineageV1,
    authority_receipt: &A::Receipt,
    authority: &A,
    intents: &[MemoryWriteIntent],
    store: &mut MemoryWriteStore,
) -> Result<(), AsyncAgentRunnerError> {
    let agent_id = context.agent_id.as_str();
    context
        .validate_for_agent(agent_id)
        .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
    validate_feedback(context, agent_id, feedback)?;
    if feedback.status != "committed" {
        return Err(AsyncAgentRunnerError::Cognition(
            "receipt memory projection requires committed feedback".into(),
        ));
    }
    validate_runtime_receipt_lineage(context, feedback, runtime_receipt)?;
    let policy_context = MemoryWritePolicyContextV1 {
        agent_id: context.agent_id.clone(),
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        request_digest: context.request_digest.to_string(),
        source: "provider".to_string(),
        provenance: "provider_unverified".to_string(),
    };
    let policy = MemoryWriteIntentPolicyV1::default();
    runtime_receipt.validate().map_err(|error| {
        AsyncAgentRunnerError::Cognition(format!(
            "Runtime receipt lineage invalid for memory projection: {error}"
        ))
    })?;
    let mut normalized = Vec::with_capacity(intents.len());
    for intent in intents.iter().cloned() {
        let intent = MemoryWriteIntentV1 {
            schema_version: 1,
            scope: intent.scope,
            summary: Some(intent.summary),
            tags: intent.tags,
            compatibility_reason: None,
        };
        let intent = match policy.normalize(intent, &policy_context) {
            Ok(intent) => intent,
            Err(error) => {
                // The Runtime receipt already committed the world action.
                // A provider-authored memory intent is a separate bounded
                // projection and may be rejected without rewriting that
                // authoritative disposition as ActionRejected.
                tracing::warn!(
                    agent_id,
                    code = error.code(),
                    error = %error,
                    "provider memory intent rejected after Runtime commit"
                );
                continue;
            }
        };
        let digest = match policy.intent_digest(&intent, &policy_context) {
            Ok(digest) => digest,
            Err(error) => {
                tracing::warn!(
                    agent_id,
                    code = error.code(),
                    error = %error,
                    "provider memory intent digest rejected after Runtime commit"
                );
                continue;
            }
        };
        normalized.push((intent, digest));
    }
    for (intent, digest) in normalized {
        store
            .apply_runtime_receipt_with_authority(
                intent,
                digest,
                authority_receipt,
                &policy_context,
                authority,
            )
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
    }
    store
        .finalize_corrections_with_authority(authority_receipt, &policy_context, authority)
        .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
    Ok(())
}
