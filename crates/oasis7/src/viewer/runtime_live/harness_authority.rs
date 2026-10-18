//! Oasis-owned Runtime authority callbacks for the extracted agent harness.
//!
//! The Harness consumes only projections returned here after Oasis has
//! validated the corresponding durable Runtime record. This adapter never
//! creates leases, receipts, or continuation authority.

use oasis7_agent_api::{
    CognitionError, CognitionLeaseConsumptionViewV1, ContinuousAgentRequestContextV1,
    FeedbackEnvelopeV1, ResponseArtifactIdentityV1, RuntimeReceiptLineageV1,
};
use oasis7_agent_harness::{
    ContinuationHostProjection, ContinuationStatusProjectionV1, FeedbackHistoryLineage,
    FeedbackHistoryProjection, MemoryWritePolicyContextV1, RuntimeAuthority,
};

use crate::runtime::{
    AgentContinuation, CognitionLeaseStatusV1, CognitionLeaseV1,
    RuntimeCognitionResponseArtifactV1, RuntimeReceiptLineageV1 as RuntimeOwnedReceiptLineageV1,
    World as RuntimeWorld,
};
use crate::simulator::{
    AsyncAgentRunnerError, ContinuousAgentTurnContextV1,
    FeedbackEnvelopeV1 as OasisFeedbackEnvelopeV1, RuntimeReceiptReadbackHandleV1,
    RuntimeReceiptReadbackVerifier, agent_api_compat, validated_cognition_lease_consumption_view,
};

/// Read-only adapter over one actual Runtime World. Runtime owns this value's
/// lifetime; the Harness receives no reference to it and cannot mutate World.
pub(super) struct RuntimeWorldAuthority<'a> {
    world: &'a RuntimeWorld,
}

impl<'a> RuntimeWorldAuthority<'a> {
    pub(super) fn new(world: &'a RuntimeWorld) -> Self {
        Self { world }
    }

    fn receipt_lineage(
        &self,
        receipt: &RuntimeOwnedReceiptLineageV1,
    ) -> Result<RuntimeOwnedReceiptLineageV1, CognitionError> {
        receipt
            .validate()
            .map_err(|error| cognition_error("runtime_receipt_invalid", error))?;
        let actual = self
            .world
            .read_runtime_receipt_lineage(&receipt.receipt_id)
            .map_err(|error| {
                cognition_error("runtime_receipt_readback_failed", format!("{error:?}"))
            })?;
        if actual != *receipt {
            return Err(CognitionError::new(
                "runtime_receipt_readback_mismatch",
                "supplied receipt lineage differs from Runtime durable readback",
            ));
        }
        Ok(actual)
    }

    fn committed_feedback_lineage(
        &self,
        feedback: &OasisFeedbackEnvelopeV1,
    ) -> Result<RuntimeOwnedReceiptLineageV1, CognitionError> {
        let api_feedback = agent_api_compat::feedback_to_api(feedback);
        api_feedback
            .validate()
            .map_err(|error| cognition_error("runtime_feedback_invalid", error))?;
        if api_feedback.status != "committed" {
            return Err(CognitionError::new(
                "runtime_feedback_not_committed",
                "receipt lineage is required only for committed feedback",
            ));
        }
        let receipt_id = api_feedback.runtime_receipt_id.as_deref().ok_or_else(|| {
            CognitionError::new(
                "runtime_feedback_receipt_missing",
                "committed feedback has no Runtime receipt identity",
            )
        })?;
        let lineage = self
            .world
            .read_runtime_receipt_lineage(receipt_id)
            .map_err(|error| {
                cognition_error("runtime_receipt_readback_failed", format!("{error:?}"))
            })?;
        lineage
            .validate()
            .map_err(|error| cognition_error("runtime_receipt_invalid", error))?;
        if lineage.status != "committed"
            || lineage.receipt_id != receipt_id
            || lineage.agent_id != api_feedback.agent_subject
            || lineage.agent_session_id != api_feedback.agent_session_id
            || lineage.agent_turn_id != api_feedback.agent_turn_id
            || lineage.decision_request_id != api_feedback.decision_request_id
            || lineage.request_digest != api_feedback.request_digest.to_string()
            || lineage.feedback_id != api_feedback.feedback_id
            || api_feedback
                .candidate_action_id
                .is_none_or(|action_id| lineage.action_id != format!("action:{action_id}"))
        {
            return Err(CognitionError::new(
                "runtime_feedback_receipt_mismatch",
                "committed feedback does not match its Runtime receipt lineage",
            ));
        }
        Ok(lineage)
    }

    fn verify_response_identity(
        &self,
        lineage: &RuntimeOwnedReceiptLineageV1,
        identity: &ResponseArtifactIdentityV1,
    ) -> Result<(), CognitionError> {
        identity
            .validate()
            .map_err(|error| cognition_error("runtime_response_identity_invalid", error))?;
        let record = self
            .world
            .replay_cognition_response(&lineage.envelope_digest)
            .map_err(|error| {
                cognition_error("runtime_response_readback_failed", format!("{error:?}"))
            })?;
        let artifact: RuntimeCognitionResponseArtifactV1 =
            serde_json::from_value(record.response_artifact.ok_or_else(|| {
                CognitionError::new(
                    "runtime_response_artifact_missing",
                    "committed Runtime response has no durable artifact identity",
                )
            })?)
            .map_err(|error| cognition_error("runtime_response_artifact_invalid", error))?;
        artifact
            .validate()
            .map_err(|error| cognition_error("runtime_response_artifact_invalid", error))?;
        if artifact.schema_version != identity.schema_version
            || artifact.context_discriminator != identity.context_discriminator
            || artifact.context_version != identity.context_version
            || artifact.agent_session_id != identity.agent_session_id
            || artifact.agent_turn_id != identity.agent_turn_id
            || artifact.decision_request_id != identity.decision_request_id
            || artifact.retry_seq != identity.retry_seq
            || artifact.transport_attempt != identity.transport_attempt
            || artifact.request_digest != identity.request_digest.to_string()
            || artifact.response_digest != identity.response_digest.to_string()
            || artifact.artifact_digest != identity.artifact_digest.to_string()
        {
            return Err(CognitionError::new(
                "runtime_response_identity_readback_mismatch",
                "response artifact identity differs from durable Runtime readback",
            ));
        }
        Ok(())
    }
}

impl RuntimeAuthority for RuntimeWorldAuthority<'_> {
    type Lease = CognitionLeaseV1;
    type Receipt = RuntimeOwnedReceiptLineageV1;
    type Continuation = AgentContinuation;

    fn validate_lease(
        &self,
        lease: &Self::Lease,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<CognitionLeaseConsumptionViewV1, CognitionError> {
        request.validate_production_lane()?;
        let current_binding = self
            .world
            .current_cognition_runtime_binding()
            .map_err(|error| {
                cognition_error("runtime_binding_readback_failed", format!("{error:?}"))
            })?;
        if current_binding != request.runtime_binding {
            return Err(CognitionError::new(
                "runtime_binding_changed",
                "request Runtime binding no longer matches the current World",
            ));
        }
        let economy = self.world.cognition_economy().map_err(|error| {
            cognition_error("runtime_lease_readback_failed", format!("{error:?}"))
        })?;
        if economy.leases.get(&lease.lease_id) != Some(lease)
            || lease.status != CognitionLeaseStatusV1::Reserved
        {
            return Err(CognitionError::new(
                "runtime_lease_readback_mismatch",
                "lease is not the current reserved Runtime record",
            ));
        }
        validated_cognition_lease_consumption_view(
            request.agent_subject.as_str(),
            &agent_api_compat::request_context_from_api(request.clone()),
            lease,
            self.world.state().time,
        )
        .map_err(|error| cognition_error("runtime_lease_invalid", error))
    }

    fn verify_receipt_readback(
        &self,
        feedback: &FeedbackEnvelopeV1,
        receipt: &Self::Receipt,
        response_identity: &ResponseArtifactIdentityV1,
    ) -> Result<RuntimeReceiptLineageV1, CognitionError> {
        feedback
            .validate()
            .map_err(|error| cognition_error("runtime_feedback_invalid", error))?;
        let actual = self.receipt_lineage(receipt)?;
        let oasis_feedback = agent_api_compat::feedback_from_api(feedback.clone());
        let expected = self.committed_feedback_lineage(&oasis_feedback)?;
        if actual != expected {
            return Err(CognitionError::new(
                "runtime_receipt_feedback_mismatch",
                "receipt argument differs from the feedback's Runtime receipt",
            ));
        }
        self.verify_response_identity(&actual, response_identity)?;
        Ok(oasis7_agent_api::RuntimeReceiptLineageV1::from(&actual))
    }

    fn verify_memory_receipt(
        &self,
        receipt: &Self::Receipt,
        context: Option<&MemoryWritePolicyContextV1>,
    ) -> Result<RuntimeReceiptLineageV1, CognitionError> {
        let context = context.ok_or_else(|| {
            CognitionError::new(
                "runtime_memory_context_missing",
                "Runtime memory readback requires its provider turn context",
            )
        })?;
        if context.source != "provider" || context.provenance != "provider_unverified" {
            return Err(CognitionError::new(
                "runtime_memory_context_invalid",
                "memory intent is not from the unverified provider lane",
            ));
        }
        let actual = self.receipt_lineage(receipt)?;
        if actual.status != "committed"
            || actual.agent_id != context.agent_id
            || actual.agent_session_id != context.agent_session_id
            || actual.agent_turn_id != context.agent_turn_id
            || actual.request_digest != context.request_digest
        {
            return Err(CognitionError::new(
                "runtime_memory_receipt_mismatch",
                "committed receipt does not match the provider memory intent context",
            ));
        }
        Ok(oasis7_agent_api::RuntimeReceiptLineageV1::from(&actual))
    }

    fn validated_feedback_history(&self) -> Result<Vec<FeedbackHistoryProjection>, CognitionError> {
        let records = self.world.runtime_feedback_outbox().map_err(|error| {
            cognition_error(
                "runtime_feedback_history_readback_failed",
                format!("{error:?}"),
            )
        })?;
        records
            .into_iter()
            .map(|record| {
                record
                    .validate()
                    .map_err(|error| cognition_error("runtime_feedback_record_invalid", error))?;
                let feedback =
                    serde_json::from_value::<OasisFeedbackEnvelopeV1>(record.payload.clone())
                        .map_err(|error| {
                            cognition_error("runtime_feedback_payload_invalid", error)
                        })?;
                let receipt_lineage = if feedback.status == "committed" {
                    Some(oasis7_agent_api::RuntimeReceiptLineageV1::from(
                        &self.committed_feedback_lineage(&feedback)?,
                    ))
                } else {
                    None
                };
                Ok(FeedbackHistoryProjection {
                    feedback: agent_api_compat::feedback_to_api(&feedback),
                    acknowledged: record.state == "acked",
                    lineage: Some(FeedbackHistoryLineage {
                        original_envelope_digest: record.envelope_digest,
                        delivery_state: record.state,
                        receipt_lineage,
                    }),
                })
            })
            .collect()
    }

    fn continuation_projection(
        &self,
        continuation: &Self::Continuation,
    ) -> Result<ContinuationHostProjection, CognitionError> {
        continuation
            .validate_authoritative()
            .map_err(|error| cognition_error("runtime_continuation_invalid", error))?;
        Ok(ContinuationHostProjection {
            continuation_proposal_id: continuation.continuation_proposal_id.clone(),
            proposal_digest: continuation.proposal_digest.clone(),
            world_id: continuation.world_id.clone(),
            agent_id: continuation.agent_id.clone(),
            agent_session_id: continuation.agent_session_id.clone(),
            agent_turn_id: continuation.agent_turn_id.clone(),
            decision_request_id: continuation.decision_request_id.clone(),
            origin_turn_id: continuation.origin_turn_id.clone(),
            origin_request_digest: continuation.origin_request_digest.clone(),
            action_or_envelope_digest: continuation.action_or_envelope_digest.clone(),
            precondition_digest: continuation.precondition_digest.clone(),
            valid_until_tick: continuation.valid_until_tick,
            continuation_id: continuation.continuation_id.clone(),
            wake_id: continuation.wake_id.clone(),
            wake_seq: continuation.wake_seq,
            continuation_digest: continuation.continuation_digest(),
            continuation_status_digest: continuation
                .continuation_status_digest
                .clone()
                .expect("validate_authoritative requires a Runtime-issued status digest"),
            status: match continuation.status {
                crate::runtime::ContinuationStatusV1::Scheduled => {
                    ContinuationStatusProjectionV1::Scheduled
                }
                crate::runtime::ContinuationStatusV1::Pending => {
                    ContinuationStatusProjectionV1::Pending
                }
                crate::runtime::ContinuationStatusV1::Waking => {
                    ContinuationStatusProjectionV1::Waking
                }
                crate::runtime::ContinuationStatusV1::Consumed => {
                    ContinuationStatusProjectionV1::Consumed
                }
                crate::runtime::ContinuationStatusV1::Completed => {
                    ContinuationStatusProjectionV1::Completed
                }
                crate::runtime::ContinuationStatusV1::Cancelled => {
                    ContinuationStatusProjectionV1::Cancelled
                }
                crate::runtime::ContinuationStatusV1::Invalidated => {
                    ContinuationStatusProjectionV1::Invalidated
                }
                crate::runtime::ContinuationStatusV1::Expired => {
                    ContinuationStatusProjectionV1::Expired
                }
                crate::runtime::ContinuationStatusV1::Rejected => {
                    ContinuationStatusProjectionV1::Rejected
                }
            },
            terminal_disposition: continuation.terminal_disposition.clone(),
            remaining_budget: oasis7_agent_api::ContinuationBudgetV1 {
                unit: continuation.remaining_budget.unit.clone(),
                value: continuation.remaining_budget.value,
            },
        })
    }
}

impl RuntimeReceiptReadbackVerifier for RuntimeWorldAuthority<'_> {
    fn verify_world_readback(
        &self,
        context: &ContinuousAgentTurnContextV1,
        feedback: &OasisFeedbackEnvelopeV1,
        receipt: &RuntimeOwnedReceiptLineageV1,
        response_identity: &ResponseArtifactIdentityV1,
    ) -> Result<RuntimeReceiptReadbackHandleV1, AsyncAgentRunnerError> {
        context
            .validate_for_agent(context.agent_id.as_str())
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        let api_feedback = agent_api_compat::feedback_to_api(feedback);
        api_feedback
            .validate()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if api_feedback.agent_subject != context.agent_id
            || api_feedback.agent_session_id != context.agent_session_id
            || api_feedback.agent_turn_id != context.agent_turn_id
            || api_feedback.decision_request_id != context.decision_request_id
            || api_feedback.request_digest.as_str() != context.request_digest.as_str()
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "Runtime feedback does not match the host turn context".to_string(),
            ));
        }

        // The input lineage is only a lookup key and expected identity. Both
        // receipt and feedback are independently resolved through this World,
        // then compared before the opaque capability is minted.
        let durable_receipt = self
            .receipt_lineage(receipt)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        let feedback_receipt = self
            .committed_feedback_lineage(feedback)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if durable_receipt != feedback_receipt
            || durable_receipt.agent_id != context.agent_id
            || durable_receipt.agent_session_id != context.agent_session_id
            || durable_receipt.agent_turn_id != context.agent_turn_id
            || durable_receipt.decision_request_id != context.decision_request_id
            || durable_receipt.request_digest != context.request_digest.to_string()
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "Runtime receipt, feedback, and host context identity mismatch".to_string(),
            ));
        }
        self.verify_response_identity(&durable_receipt, response_identity)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;

        Ok(RuntimeReceiptReadbackHandleV1::from_durable_world_readback(
            &durable_receipt,
            response_identity,
        ))
    }
}

fn cognition_error(code: &str, error: impl std::fmt::Display) -> CognitionError {
    CognitionError::new(code, error.to_string())
}

#[cfg(test)]
mod world_readback_tests {
    use super::*;
    use crate::geometry::GeoPos;
    use crate::runtime::{
        Action, RuntimeCognitionBaseBindingV1, RuntimeCognitionCommitRequestV1,
        RuntimeCognitionResponseArtifactV1,
    };
    use crate::simulator::{
        ContinuousAgentTurnContextV1, Digest32, FeedbackEnvelopeV1, GoalSnapshotV1,
        MemoryContextSnapshotV1, ResponseArtifactIdentityV1, RuntimeReceiptReadbackVerifier,
    };

    fn digest(hex: char) -> String {
        format!("blake3:{}", hex.to_string().repeat(64))
    }

    #[test]
    fn runtime_world_authority_verifies_durable_receipt_and_response_before_minting_handle() {
        let mut world = crate::runtime_host::world::new();
        world
            .bind_cognition_runtime("world-readback", "main", 0, None, "pending", 0)
            .expect("bind Runtime World");
        let binding = world
            .current_cognition_runtime_binding()
            .expect("read current Runtime binding");
        let capability_root = world.capability_authorization_root().to_string();
        let request = RuntimeCognitionCommitRequestV1 {
            agent_id: "agent-readback".to_string(),
            agent_session_id: "session-readback".to_string(),
            agent_turn_id: "turn-readback".to_string(),
            decision_request_id: "request-readback".to_string(),
            retry_seq: 1,
            transport_attempt: 1,
            request_digest: digest('a'),
            observation_digest: digest('b'),
            context_digest: digest('c'),
            capability_snapshot_hash: crate::simulator::h_v1(
                "oasis7.runtime.manifest.v1",
                &capability_root,
            )
            .to_string(),
            authority_context_hash: crate::simulator::h_v1(
                "oasis7.runtime.authority-context.v1",
                &capability_root,
            )
            .to_string(),
            captured_base_binding: RuntimeCognitionBaseBindingV1 {
                world_id: binding.world_id,
                branch_id: binding.branch_id,
                finality_epoch: binding.finality_epoch,
                finality_block_hash: binding.finality_block_hash.map(|hash| hash.to_string()),
                finality_status: binding.finality_status,
                base_tick: binding.base_tick,
                base_world_hash: binding.base_world_hash.to_string(),
                reorg_epoch: binding.reorg_epoch,
                runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
            },
        };
        let mut artifact = RuntimeCognitionResponseArtifactV1 {
            schema_version: 1,
            context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR
                .to_string(),
            context_version: RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION,
            agent_session_id: request.agent_session_id.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            retry_seq: request.retry_seq,
            transport_attempt: request.transport_attempt,
            request_digest: request.request_digest.clone(),
            response_digest: digest('d'),
            artifact_digest: String::new(),
        };
        artifact.refresh_artifact_digest();
        let (commit, runtime_receipt) = world
            .commit_cognition_action(
                request.clone(),
                Action::RegisterAgent {
                    agent_id: request.agent_id.clone(),
                    pos: GeoPos::new(0, 0, 0),
                },
                artifact.clone(),
            )
            .expect("commit a real Runtime action and response");
        let feedback = FeedbackEnvelopeV1 {
            feedback_id: runtime_receipt.feedback_id.clone(),
            feedback_seq: 1,
            agent_subject: request.agent_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            candidate_action_id: Some(
                runtime_receipt
                    .action_id
                    .strip_prefix("action:")
                    .expect("Runtime action prefix")
                    .parse()
                    .expect("numeric action ID"),
            ),
            runtime_receipt_id: Some(runtime_receipt.receipt_id.clone()),
            status: "committed".to_string(),
            request_digest: Digest32(request.request_digest.clone()),
            reject_reason: None,
            provenance: "runtime_authoritative".to_string(),
        };
        let context = ContinuousAgentTurnContextV1 {
            agent_id: request.agent_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            request_digest: Digest32(request.request_digest.clone()),
            memory_snapshot: MemoryContextSnapshotV1::empty("session_private"),
            goal_snapshot: GoalSnapshotV1::empty(),
            continuation: None,
        };
        let response_identity = ResponseArtifactIdentityV1 {
            schema_version: artifact.schema_version,
            context_discriminator: artifact.context_discriminator.clone(),
            context_version: artifact.context_version,
            agent_session_id: artifact.agent_session_id.clone(),
            agent_turn_id: artifact.agent_turn_id.clone(),
            decision_request_id: artifact.decision_request_id.clone(),
            retry_seq: artifact.retry_seq,
            transport_attempt: artifact.transport_attempt,
            request_digest: Digest32(artifact.request_digest.clone()),
            response_digest: Digest32(artifact.response_digest.clone()),
            artifact_digest: Digest32(artifact.artifact_digest.clone()),
        };
        let authority = RuntimeWorldAuthority::new(&world);

        let handle = authority
            .verify_world_readback(&context, &feedback, &runtime_receipt, &response_identity)
            .expect("real World durable receipt and response readback produces handle");
        assert_eq!(handle.receipt_id(), runtime_receipt.receipt_id);
        assert_eq!(
            handle.receipt_digest().as_str(),
            runtime_receipt.receipt_digest
        );
        assert_eq!(
            handle.response_artifact_digest(),
            &response_identity.artifact_digest
        );

        let mut forged_receipt = runtime_receipt.clone();
        forged_receipt.receipt_digest = digest('e');
        assert!(
            authority
                .verify_world_readback(&context, &feedback, &forged_receipt, &response_identity,)
                .is_err(),
            "a caller-provided receipt DTO cannot replace durable readback"
        );

        let mut forged_response = response_identity;
        forged_response.response_digest = Digest32(digest('f'));
        assert!(
            authority
                .verify_world_readback(&context, &feedback, &runtime_receipt, &forged_response,)
                .is_err(),
            "a caller-provided response identity cannot replace durable readback"
        );
        assert_eq!(commit.receipt_id, runtime_receipt.receipt_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeFeedbackOutboxRecordV1;

    fn rejected_feedback(agent: &str, session: &str, seq: u64) -> OasisFeedbackEnvelopeV1 {
        OasisFeedbackEnvelopeV1 {
            feedback_id: format!("{agent}-{session}-{seq}"),
            feedback_seq: seq,
            agent_subject: agent.to_string(),
            agent_session_id: session.to_string(),
            agent_turn_id: format!("turn-{seq}"),
            decision_request_id: format!("request-{seq}"),
            candidate_action_id: None,
            runtime_receipt_id: None,
            status: "rejected".to_string(),
            request_digest: crate::simulator::Digest32(format!("blake3:{}", "a".repeat(64))),
            reject_reason: Some("stale_base".to_string()),
            provenance: "runtime_authoritative".to_string(),
        }
    }

    fn record(feedback: &OasisFeedbackEnvelopeV1, state: &str) -> RuntimeFeedbackOutboxRecordV1 {
        let mut record = crate::runtime_host::world::new()
            .enqueue_runtime_feedback(crate::simulator::agent_api_compat::feedback_to_api(
                feedback,
            ))
            .expect("feedback creates a valid outbox record");
        record.state = state.to_string();
        record
            .validate()
            .expect("record validates after state transition");
        record
    }

    #[test]
    fn runtime_history_projects_ack_state_from_durable_outbox() {
        let mut world = crate::runtime_host::world::new();
        let acked = rejected_feedback("agent-a", "session-a", 1);
        let pending = rejected_feedback("agent-a", "session-a", 2);
        world
            .enqueue_runtime_feedback(crate::simulator::agent_api_compat::feedback_to_api(&acked))
            .unwrap();
        world.claim_runtime_feedback(&acked.feedback_id).unwrap();
        world.ack_runtime_feedback(&acked.feedback_id).unwrap();
        world
            .enqueue_runtime_feedback(crate::simulator::agent_api_compat::feedback_to_api(
                &pending,
            ))
            .unwrap();

        let authority = RuntimeWorldAuthority::new(&world);
        let history = authority.validated_feedback_history().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(
            history[0].feedback,
            agent_api_compat::feedback_to_api(&acked)
        );
        assert!(history[0].acknowledged);
        assert_eq!(
            history[1].feedback,
            agent_api_compat::feedback_to_api(&pending)
        );
        assert!(!history[1].acknowledged);
    }

    #[test]
    fn history_rejects_a_forged_durable_envelope_digest() {
        let mut world = crate::runtime_host::world::new();
        let feedback = rejected_feedback("agent-a", "session-a", 1);
        let mut forged = record(&feedback, "acked");
        forged.envelope_digest = format!("blake3:{}", "b".repeat(64));
        world
            .enqueue_runtime_feedback(crate::simulator::agent_api_compat::feedback_to_api(
                &feedback,
            ))
            .expect("Runtime accepts the original payload");
        // The authority reads only the actual World record; a detached record
        // cannot be substituted for that durable history.
        assert!(forged.validate().is_err());
        let history = RuntimeWorldAuthority::new(&world)
            .validated_feedback_history()
            .unwrap();
        assert_eq!(history.len(), 1);
    }
}
