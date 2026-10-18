use super::llm_sidecar::RuntimeProviderActionContext;
use super::*;
use crate::runtime::{
    CognitionCommitRejectReasonV1, RuntimeCognitionBaseBindingV1, RuntimeCognitionCommitRequestV1,
    RuntimeCognitionResponseArtifactV1, RuntimeFeedbackProjectionV1, World as RuntimeWorld,
    classify_cognition_commit_error,
};
use crate::simulator::h_v1;
use oasis7_simulator::Action as SimulatorAction;

impl ViewerRuntimeLiveServer {
    /// Retry sidecar finalization from the committed Runtime receipt.
    pub(super) fn retry_committed_provider_action(&mut self) -> Result<(), String> {
        if self.chain_link_enabled() {
            if self.config.world_service.is_none() {
                return Err("canonical Agent receipt lookup requires service configuration".into());
            }
            let pending = self
                .llm_sidecar
                .provider_service_pending
                .values()
                .next()
                .cloned();
            let Some(pending) = pending else {
                return Ok(());
            };
            let crate::world_service::wire::WorldServicePayloadV1::Cognition(signed) =
                &pending.payload
            else {
                return Err("pending Agent recovery has wrong operation".into());
            };
            return self
                .submit_provider_service_action(
                    &signed.request.action,
                    &pending.cognition,
                    pending.action.clone(),
                )
                .map_err(|error| format!("{error:?}"));
        }
        let Some((action_id, _agent_id, cognition)) =
            self.llm_sidecar.pending_provider_action_for_recovery()
        else {
            return Ok(());
        };
        let request = &cognition.request.request_context;
        let marker = self
            .world
            .cognition()
            .get("commit_records")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|value| {
                serde_json::from_value::<crate::runtime::WorldCommitRecordV1>(value.clone()).ok()
            })
            .find(|marker| {
                marker.status == "committed"
                    && marker.agent_id == request.agent_subject
                    && marker.agent_session_id == request.agent_session_id
                    && marker.agent_turn_id == request.agent_turn_id
                    && marker.decision_request_id == request.decision_request_id
                    && marker.request_digest == request.request_digest.to_string()
            });
        let Some(marker) = marker else {
            return Ok(());
        };
        let lineage = self
            .world
            .read_runtime_receipt_lineage(marker.receipt_id.as_str())
            .map_err(|error| {
                format!("Runtime cognition receipt recovery readback failed: {error:?}")
            })?;
        self.world
            .verify_runtime_receipt_lineage(&lineage)
            .map_err(|error| {
                format!("Runtime cognition receipt recovery verification failed: {error:?}")
            })?;
        self.settle_provider_cognition_lease_for_request(
            request.agent_subject.as_str(),
            cognition.cognition_lease.clone(),
            Some(request),
        )?;

        let existing_feedback = self
            .world
            .runtime_feedback_outbox()
            .map_err(|error| format!("Runtime feedback recovery outbox read failed: {error:?}"))?
            .into_iter()
            .find(|record| record.feedback_id == lineage.feedback_id);
        let feedback = if let Some(record) = existing_feedback {
            let payload = record
                .transport_payload()
                .map_err(|error| format!("Runtime feedback recovery payload invalid: {error}"))?;
            let feedback = serde_json::from_value::<crate::simulator::FeedbackEnvelopeV1>(payload)
                .map_err(|error| {
                    format!("Runtime feedback recovery payload decode failed: {error}")
                })?;
            if feedback.status != "committed"
                || feedback.runtime_receipt_id.as_deref() != Some(lineage.receipt_id.as_str())
                || feedback.agent_subject != lineage.agent_id
                || feedback.agent_session_id != lineage.agent_session_id
                || feedback.agent_turn_id != lineage.agent_turn_id
                || feedback.decision_request_id != lineage.decision_request_id
                || feedback.request_digest.to_string() != lineage.request_digest
            {
                return Err("Runtime feedback recovery identity mismatch".to_string());
            }
            feedback
        } else {
            let feedback = self.llm_sidecar.provider_feedback(
                &cognition,
                Some(action_id),
                "committed",
                Some(lineage.receipt_id.clone()),
                Some(lineage.feedback_id.clone()),
                None,
            );
            let projection = RuntimeFeedbackProjectionV1 {
                envelope_digest: Some(lineage.envelope_digest.clone()),
                emitted_events: Vec::new(),
                committed_event_summary: Some(format!(
                    "runtime_receipt_id={} action_id={}",
                    lineage.receipt_id, lineage.action_id
                )),
                world_delta_summary: None,
            };
            self.allocate_runtime_feedback(feedback, projection)
                .map_err(|error| format!("Runtime feedback recovery allocation failed: {error}"))?
                .transport_payload()
                .map_err(|error| format!("Runtime feedback recovery payload invalid: {error}"))
                .and_then(|payload| {
                    serde_json::from_value(payload).map_err(|error| {
                        format!("Runtime feedback recovery payload decode failed: {error}")
                    })
                })?
        };
        self.llm_sidecar
            .consume_provider_memory_after_receipt(
                Some(&self.world),
                request.agent_subject.as_str(),
                feedback.clone(),
                &lineage,
                cognition.memory_write_intents.as_slice(),
            )
            .map_err(|error| {
                format!("Runtime receipt recovery memory projection failed: {error}")
            })?;
        self.llm_sidecar
            .finalize_provider_action_with_feedback_checked(action_id, feedback)
            .map_err(|error| format!("Runtime receipt recovery finalization failed: {error}"))?;
        self.drain_provider_feedback_outbox();
        if let Err(error) = self.handoff_runtime_wake_for_agent(
            request.agent_subject.as_str(),
            crate::runtime::ContinuationStatusV1::Completed,
            "provider_action_committed",
        ) {
            return Err(format!(
                "Runtime receipt recovery wake handoff failed: {error}"
            ));
        }
        Ok(())
    }

    pub(super) fn commit_provider_runtime_action(
        &mut self,
        runtime_action: &crate::runtime::Action,
        cognition: &RuntimeProviderActionContext,
        simulator_action: SimulatorAction,
    ) -> Result<(), ProviderRuntimeActionCommitError> {
        if self.chain_link_enabled() {
            return self.submit_provider_service_action(
                runtime_action,
                cognition,
                simulator_action,
            );
        }
        if self.world.pending_actions_len() != 0 {
            return Err(ProviderRuntimeActionCommitError::Message(
                "Runtime has pending actions; provider cognition action rejected".to_string(),
            ));
        }

        let (request, response_artifact) = provider_cognition_commit_inputs(&self.world, cognition)
            .map_err(ProviderRuntimeActionCommitError::Message)?;
        let mut transaction = self.world.clone();
        let explanation = cognition
            .response
            .base_decision_response
            .public_agency_explanation();
        let evidence = crate::runtime::ProviderDecisionEvidenceV1 {
            // This is a proposal, explicitly distinct from the actual
            // domain event references published by the transaction.
            expected_consequence: serde_json::json!({"status":explanation.status,"provenance":explanation.provenance,"prediction":explanation.expected_consequence}),
            stakes: serde_json::json!({"provenance":"agent_explanation_unverified","summary":explanation.stakes}),
            alternative: serde_json::json!({"provenance":"agent_explanation_unverified","alternatives":explanation.alternatives}),
            evidence_refs: vec![
                request.observation_digest.clone(),
                cognition
                    .request
                    .request_context
                    .memory_snapshot_digest
                    .to_string(),
            ],
            correction_refs: self
                .llm_sidecar
                .correction_refs_for_decision(&request.agent_id, &request.decision_request_id),
            reason: explanation.reason,
            dissent: explanation.dissent,
        };
        let (committed, returned_lineage) = match transaction.commit_provider_cognition_action(
            request,
            runtime_action.clone(),
            response_artifact,
            evidence,
        ) {
            Ok(result) => result,
            Err(crate::runtime::ProviderCognitionCommitError::AgencyBinding(error)) => {
                self.world = transaction;
                return Err(ProviderRuntimeActionCommitError::Message(format!(
                    "Runtime agency binding rejected: {error:?}"
                )));
            }
            Err(crate::runtime::ProviderCognitionCommitError::Commit(error)) => {
                self.world = transaction;
                return Err(
                    if matches!(
                        classify_cognition_commit_error(&error),
                        Some(CognitionCommitRejectReasonV1::StaleBase)
                    ) {
                        ProviderRuntimeActionCommitError::StaleBase
                    } else {
                        ProviderRuntimeActionCommitError::Message(format!(
                            "Runtime cognition action commit rejected provider action: {error:?}"
                        ))
                    },
                );
            }
        };
        self.world = transaction;
        let action_id = committed
            .action_id
            .strip_prefix("action:")
            .ok_or_else(|| {
                ProviderRuntimeActionCommitError::PostCommit(
                    "Runtime cognition commit returned an invalid action id".to_string(),
                )
            })?
            .parse::<u64>()
            .map_err(|error| {
                ProviderRuntimeActionCommitError::PostCommit(format!(
                    "Runtime cognition action id is not numeric: {error}"
                ))
            })?;
        self.llm_sidecar.track_action(
            action_id,
            cognition.request.request_context.agent_subject.clone(),
            simulator_action,
            Some(cognition.clone()),
        );
        // The Runtime commit is authoritative. Persist the recovery record
        // before settling so an economy persistence fault can retry the same
        // idempotent operation after restart.
        self.settle_provider_cognition_lease_for_request(
            cognition.request.request_context.agent_subject.as_str(),
            cognition.cognition_lease.clone(),
            Some(&cognition.request.request_context),
        )
        .map_err(ProviderRuntimeActionCommitError::PostCommit)?;
        let lineage = self
            .world
            .read_runtime_receipt_lineage(returned_lineage.receipt_id.as_str())
            .map_err(|error| {
                ProviderRuntimeActionCommitError::PostCommit(format!(
                    "Runtime cognition receipt readback failed: {error:?}"
                ))
            })?;
        self.world
            .verify_runtime_receipt_lineage(&lineage)
            .map_err(|error| {
                ProviderRuntimeActionCommitError::PostCommit(format!(
                    "Runtime cognition receipt verification failed: {error:?}"
                ))
            })?;
        if lineage != returned_lineage || lineage.receipt_id != committed.receipt_id {
            return Err(ProviderRuntimeActionCommitError::PostCommit(
                "Runtime cognition receipt readback identity mismatch".to_string(),
            ));
        }
        self.llm_sidecar
            .clear_provider_stale_replans(cognition.request.request_context.agent_subject.as_str());
        let feedback = self.llm_sidecar.provider_feedback(
            cognition,
            Some(action_id),
            "committed",
            Some(committed.receipt_id.clone()),
            Some(lineage.feedback_id.clone()),
            None,
        );
        let feedback_projection = RuntimeFeedbackProjectionV1 {
            envelope_digest: Some(lineage.envelope_digest.clone()),
            emitted_events: Vec::new(),
            committed_event_summary: Some(format!(
                "runtime_receipt_id={} action_id={}",
                lineage.receipt_id, lineage.action_id
            )),
            world_delta_summary: None,
        };
        let queued_feedback = self
            .allocate_runtime_feedback(feedback.clone(), feedback_projection)
            .map_err(|error| {
                tracing::warn!(
                    error,
                    "Runtime receipt committed but feedback outbox allocation failed"
                );
                ProviderRuntimeActionCommitError::PostCommit(format!(
                    "Runtime receipt committed but feedback outbox allocation failed: {error}"
                ))
            })?;
        let feedback = queued_feedback
            .transport_payload()
            .ok()
            .and_then(|payload| serde_json::from_value(payload).ok())
            .unwrap_or(feedback);
        self.llm_sidecar
            .consume_provider_memory_after_receipt(
                Some(&self.world),
                cognition.request.request_context.agent_subject.as_str(),
                feedback.clone(),
                &lineage,
                cognition.memory_write_intents.as_slice(),
            )
            .map_err(|error| {
                ProviderRuntimeActionCommitError::PostCommit(format!(
                    "Runtime receipt committed but provider memory projection failed: {error}"
                ))
            })?;

        #[cfg(test)]
        if std::env::var("OASIS7_TEST_PROVIDER_ACTION_FAULT").as_deref() == Ok("persistence") {
            let fault_result = self
                .llm_sidecar
                .install_test_provider_lineage_checkpoint_blocker();
            fault_result.map_err(|error| {
                ProviderRuntimeActionCommitError::PostCommit(format!(
                    "provider action finalization checkpoint fault setup failed: {error}"
                ))
            })?;
        }
        self.llm_sidecar
            .finalize_provider_action_with_feedback_checked(action_id, feedback)
            .map_err(|error| {
                ProviderRuntimeActionCommitError::PostCommit(format!(
                    "provider cognition receipt finalization remains pending: {error}"
                ))
            })?;
        self.drain_provider_feedback_outbox();
        if let Err(error) = self.handoff_runtime_wake_for_agent(
            cognition.request.request_context.agent_subject.as_str(),
            crate::runtime::ContinuationStatusV1::Completed,
            "provider_action_committed",
        ) {
            self.llm_sidecar.retain_provider_wake_recovery_pending(
                cognition.request.request_context.agent_subject.as_str(),
                &cognition.request,
                crate::runtime::ContinuationStatusV1::Completed,
                "provider_action_committed",
            );
            return Err(ProviderRuntimeActionCommitError::WakeHandoff(format!(
                "Runtime wake handoff failed: {error}"
            )));
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn prepare_provider_service_action_checkpoint(
        &mut self,
        runtime_action: &crate::runtime::Action,
        cognition: &RuntimeProviderActionContext,
        simulator_action: SimulatorAction,
    ) -> Result<
        (
            super::llm_sidecar::lineage_persistence::PendingProviderServiceIntent,
            bool,
        ),
        ProviderRuntimeActionCommitError,
    > {
        let client = self
            .world_service_client()
            .map_err(|error| ProviderRuntimeActionCommitError::Message(format!("{error:?}")))?
            .ok_or_else(|| {
                ProviderRuntimeActionCommitError::Message(
                    "canonical Agent service is not configured".into(),
                )
            })?;
        let agent_id = cognition.request.request_context.agent_subject.clone();
        let existing = self
            .llm_sidecar
            .provider_service_pending
            .get(&agent_id)
            .cloned();
        let pending = if let Some(pending) = existing.as_ref() {
            let crate::world_service::wire::WorldServicePayloadV1::Cognition(signed) =
                &pending.payload
            else {
                return Err(ProviderRuntimeActionCommitError::Message(
                    "pending cognition operation mismatch".into(),
                ));
            };
            let signer = self
                .config
                .world_service_agent_signer
                .as_ref()
                .ok_or_else(|| {
                    ProviderRuntimeActionCommitError::Message(
                        "explicit Agent signer is required for recovery".into(),
                    )
                })?;
            let key_bytes: [u8; 32] = hex::decode(&signer.private_key_hex)
                .map_err(|_| {
                    ProviderRuntimeActionCommitError::Message("Agent signer key is invalid".into())
                })?
                .try_into()
                .map_err(|_| {
                    ProviderRuntimeActionCommitError::Message("Agent signer key is invalid".into())
                })?;
            let public_key = hex::encode(
                ed25519_dalek::SigningKey::from_bytes(&key_bytes)
                    .verifying_key()
                    .to_bytes(),
            );
            if signed.request.delegation_generation != signer.delegation_generation
                || signed.subject_public_key != public_key
            {
                return Err(ProviderRuntimeActionCommitError::Message(
                    "pending Agent signer or delegation generation changed; original intent fenced"
                        .into(),
                ));
            }
            if pending.cognition.request.request_context != cognition.request.request_context {
                return Err(ProviderRuntimeActionCommitError::Message(
                    "an earlier canonical Agent intent remains pending".into(),
                ));
            }
            pending.clone()
        } else {
            self.config
                .ensure_service_agent_lineage_store()
                .map_err(ProviderRuntimeActionCommitError::Message)?;
            let signer = self
                .config
                .world_service_agent_signer
                .as_ref()
                .ok_or_else(|| {
                    ProviderRuntimeActionCommitError::Message(
                        "explicit Agent signer/delegation is required".into(),
                    )
                })?;
            let authority = self
                .verified_world_view
                .as_ref()
                .and_then(|view| view.projection().agent_context.as_ref())
                .ok_or_else(|| {
                    ProviderRuntimeActionCommitError::Message(
                        "authorized canonical Agent view context missing".into(),
                    )
                })?;
            let payload = provider_service_cognition_payload(
                &self.world,
                runtime_action,
                cognition,
                signer,
                authority,
                self.llm_sidecar.correction_refs_for_decision(
                    &cognition.request.request_context.agent_subject,
                    &cognition.request.request_context.decision_request_id,
                ),
            )
            .map_err(ProviderRuntimeActionCommitError::Message)?;
            let correlation = crate::world_service::derive_correlation(
                client.config().expected_world.clone(),
                &payload,
            )
            .map_err(ProviderRuntimeActionCommitError::Message)?;
            let pending = super::llm_sidecar::lineage_persistence::PendingProviderServiceIntent {
                correlation,
                payload,
                cognition: cognition.clone(),
                action: simulator_action,
                feedback_ack: None,
            };
            self.llm_sidecar
                .provider_service_pending
                .insert(agent_id, pending.clone());
            // Persist before crossing the network, including the original signature.
            self.llm_sidecar
                .persist_provider_lineage()
                .map_err(ProviderRuntimeActionCommitError::Message)?;
            pending
        };
        Ok((pending, existing.is_some()))
    }

    fn submit_provider_service_action(
        &mut self,
        runtime_action: &crate::runtime::Action,
        cognition: &RuntimeProviderActionContext,
        simulator_action: SimulatorAction,
    ) -> Result<(), ProviderRuntimeActionCommitError> {
        self.ensure_direct_hosted_service_owner(cognition)
            .map_err(ProviderRuntimeActionCommitError::Message)?;
        let (pending, existed) = self.prepare_provider_service_action_checkpoint(
            runtime_action,
            cognition,
            simulator_action,
        )?;
        self.drive_direct_hosted_service_action(pending, !existed)
            .map_err(|error| self.classify_direct_hosted_service_error(error))
    }
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub(in crate::viewer::runtime_live) struct ProviderServiceCognitionReceipt {
    pub(in crate::viewer::runtime_live) commit_record: crate::runtime::WorldCommitRecordV1,
    pub(in crate::viewer::runtime_live) lineage: crate::runtime::RuntimeReceiptLineageV1,
    pub(in crate::viewer::runtime_live) feedback: crate::simulator::FeedbackEnvelopeV1,
    #[serde(default)]
    pub(in crate::viewer::runtime_live) raw_feedback: serde_json::Value,
}
pub(in crate::viewer::runtime_live) fn decode_provider_service_receipt(
    value: serde_json::Value,
) -> Result<ProviderServiceCognitionReceipt, String> {
    let raw = value
        .get("feedback")
        .cloned()
        .ok_or("canonical feedback payload missing")?;
    let mut receipt: ProviderServiceCognitionReceipt =
        serde_json::from_value(value).map_err(|e| e.to_string())?;
    receipt.raw_feedback = raw;
    Ok(receipt)
}

pub(in crate::viewer::runtime_live) fn validate_provider_service_receipt(
    pending: &super::llm_sidecar::lineage_persistence::PendingProviderServiceIntent,
    receipt: &ProviderServiceCognitionReceipt,
) -> Result<(), String> {
    let crate::world_service::wire::WorldServicePayloadV1::Cognition(signed) = &pending.payload
    else {
        return Err("canonical cognition receipt has wrong pending operation".into());
    };
    crate::world_service::authority::verify_read_request("cognition", signed)
        .map_err(|error| format!("canonical cognition request signature invalid: {error}"))?;
    let request = &signed.request.request;
    let marker = &receipt.commit_record;
    let lineage = &receipt.lineage;
    let mut response_artifact = serde_json::to_value(&signed.request.response_artifact)
        .map_err(|error| format!("canonical response artifact encoding failed: {error}"))?;
    response_artifact["outer_lineage"] = serde_json::json!({
        "agent_id": request.agent_id,
        "world_id": request.captured_base_binding.world_id,
        "branch_id": request.captured_base_binding.branch_id,
        "finality_epoch": request.captured_base_binding.finality_epoch,
        "finality_block_hash": request.captured_base_binding.finality_block_hash,
        "finality_status": request.captured_base_binding.finality_status,
        "base_tick": request.captured_base_binding.base_tick,
        "base_world_hash": request.captured_base_binding.base_world_hash,
        "reorg_epoch": request.captured_base_binding.reorg_epoch,
        "runtime_manifest_hash": request.captured_base_binding.runtime_manifest_hash,
    });
    let exact_response_artifact_digest = oasis7_agent_api::world_cognition::cognition_digest_v1(
        "oasis7.cognition.response.v1",
        &response_artifact,
    );
    if marker.response_artifact_digest.is_empty()
        || marker.response_artifact_digest != exact_response_artifact_digest
    {
        return Err("canonical cognition response artifact digest mismatch".into());
    }
    lineage
        .validate()
        .map_err(|error| format!("canonical receipt lineage invalid: {error:?}"))?;
    if marker.status != "committed"
        || marker.world_id != request.captured_base_binding.world_id
        || marker.branch_id != request.captured_base_binding.branch_id
        || marker.finality_epoch != request.captured_base_binding.finality_epoch
        || marker.finality_block_hash != request.captured_base_binding.finality_block_hash
        || marker.finality_status != request.captured_base_binding.finality_status
        || marker.reorg_epoch != request.captured_base_binding.reorg_epoch
        || marker.parent_tick != request.captured_base_binding.base_tick
        || marker.normalized_base_binding() != request.captured_base_binding
        || marker.agent_id != request.agent_id
        || marker.agent_session_id != request.agent_session_id
        || marker.agent_turn_id != request.agent_turn_id
        || marker.decision_request_id != request.decision_request_id
        || marker.request_digest != request.request_digest
        || lineage
            != &crate::runtime::RuntimeReceiptLineageV1::from_commit_record(
                marker,
                request.agent_id.clone(),
                request.agent_session_id.clone(),
                request.agent_turn_id.clone(),
                request.decision_request_id.clone(),
                request.request_digest.clone(),
                marker.feedback_id.clone(),
            )
    {
        let mut fields = Vec::new();
        if marker.normalized_base_binding() != request.captured_base_binding {
            fields.push("normalized_base_binding");
        }
        if fields.is_empty() {
            fields.push("other_request_world_or_lineage_fields");
        }
        return Err(format!(
            "canonical cognition receipt request/world/lineage mismatch: fields={}",
            fields.join(",")
        ));
    }
    let feedback = &receipt.feedback;
    if feedback.status != "committed"
        || feedback.runtime_receipt_id.as_deref() != Some(lineage.receipt_id.as_str())
        || feedback.feedback_id != lineage.feedback_id
        || feedback.agent_subject != request.agent_id
        || feedback.agent_session_id != request.agent_session_id
        || feedback.agent_turn_id != request.agent_turn_id
        || feedback.decision_request_id != request.decision_request_id
        || feedback.request_digest.to_string() != request.request_digest
    {
        return Err("canonical cognition feedback identity mismatch".into());
    }
    Ok(())
}

pub(in crate::viewer::runtime_live) fn provider_cognition_commit_inputs(
    world: &RuntimeWorld,
    cognition: &RuntimeProviderActionContext,
) -> Result<
    (
        RuntimeCognitionCommitRequestV1,
        RuntimeCognitionResponseArtifactV1,
    ),
    String,
> {
    let response = &cognition.response;
    let request = &cognition.request.request_context;
    let response_identity = response.response_artifact_identity();
    response
        .validate_response_artifact_identity(&response_identity)
        .map_err(|error| format!("provider response identity rejected: {error}"))?;
    let capability_root = world.capability_authorization_root().to_string();
    let capability_snapshot_hash = h_v1("oasis7.runtime.manifest.v1", &capability_root).to_string();
    let authority_context_hash =
        h_v1("oasis7.runtime.authority-context.v1", &capability_root).to_string();
    Ok((
        RuntimeCognitionCommitRequestV1 {
            agent_id: request.agent_subject.clone(),
            agent_session_id: request.agent_session_id.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            retry_seq: request.retry_seq,
            transport_attempt: request.transport_attempt,
            request_digest: request.request_digest.to_string(),
            observation_digest: request.observation_digest.to_string(),
            context_digest: super::llm_sidecar::runtime_provider_context_digest(request),
            capability_snapshot_hash,
            authority_context_hash,
            captured_base_binding: RuntimeCognitionBaseBindingV1 {
                world_id: request.runtime_binding.world_id.clone(),
                branch_id: request.runtime_binding.branch_id.clone(),
                finality_epoch: request.runtime_binding.finality_epoch,
                finality_block_hash: request
                    .runtime_binding
                    .finality_block_hash
                    .as_ref()
                    .map(ToString::to_string),
                finality_status: request.runtime_binding.finality_status.clone(),
                base_tick: request.runtime_binding.base_tick,
                base_world_hash: request.runtime_binding.base_world_hash.to_string(),
                reorg_epoch: request.runtime_binding.reorg_epoch,
                runtime_manifest_hash: request.runtime_binding.runtime_manifest_hash.to_string(),
            },
        },
        RuntimeCognitionResponseArtifactV1 {
            schema_version: response.context_version,
            context_discriminator: response.context_discriminator.clone(),
            context_version: response.context_version,
            agent_session_id: response.agent_session_id.clone(),
            agent_turn_id: response.agent_turn_id.clone(),
            decision_request_id: response.decision_request_id.clone(),
            retry_seq: response.retry_seq,
            transport_attempt: response.transport_attempt,
            request_digest: response.request_digest.to_string(),
            response_digest: response.response_digest.to_string(),
            artifact_digest: response_identity.artifact_digest.to_string(),
        },
    ))
}

/// Sign the actual provider cognition artifact with an explicitly configured
/// delegate. The service resolves that delegate against canonical authority;
/// a local provider key binding is never used to authorize this payload.
fn provider_service_cognition_payload(
    world: &RuntimeWorld,
    runtime_action: &crate::runtime::Action,
    cognition: &RuntimeProviderActionContext,
    signer: &crate::world_service::client::WorldServiceAgentSignerConfig,
    authority: &crate::world_service::projection::WorldServiceAgentContext,
    correction_refs: Vec<String>,
) -> Result<crate::world_service::WorldServicePayloadV1, String> {
    let (mut request, response_artifact) = provider_cognition_commit_inputs(world, cognition)?;
    if authority.agent_id != request.agent_id {
        return Err("canonical Agent view subject mismatch".into());
    }
    request.capability_snapshot_hash = authority.capability_snapshot_hash.clone();
    request.authority_context_hash = authority.authority_context_hash.clone();
    let explanation = cognition
        .response
        .base_decision_response
        .public_agency_explanation();
    let causal_proposal = crate::world_service::CognitionCausalProposalV1 {
        expected_consequence: serde_json::json!({"status": explanation.status, "provenance": explanation.provenance, "prediction": explanation.expected_consequence}),
        stakes: serde_json::json!({"provenance": "agent_explanation_unverified", "summary": explanation.stakes}),
        alternative: serde_json::json!({"provenance": "agent_explanation_unverified", "alternatives": explanation.alternatives}),
        evidence_refs: vec![
            request.observation_digest.clone(),
            cognition
                .request
                .request_context
                .memory_snapshot_digest
                .to_string(),
        ],
        correction_refs,
        reason: explanation.reason,
        dissent: explanation.dissent,
    };
    let intent = crate::world_service::CognitionIntentV1 {
        request,
        action: runtime_action.clone(),
        response_artifact,
        delegation_generation: signer.delegation_generation,
        causal_proposal: Some(causal_proposal),
        task_fence: None,
    };
    let signed = crate::world_service::authority::sign_read_request(
        "cognition",
        intent,
        &signer.private_key_hex,
    )?;
    Ok(crate::world_service::WorldServicePayloadV1::Cognition(
        signed,
    ))
}

#[derive(Debug)]
pub(in crate::viewer::runtime_live) enum ProviderRuntimeActionCommitError {
    StaleBase,
    WakeHandoff(String),
    PostCommit(String),
    Message(String),
}

impl ProviderRuntimeActionCommitError {
    pub(super) fn is_stale_base(&self) -> bool {
        matches!(self, Self::StaleBase)
    }

    pub(super) fn is_wake_handoff(&self) -> bool {
        matches!(self, Self::WakeHandoff(_))
    }

    pub(super) fn is_post_commit(&self) -> bool {
        matches!(self, Self::PostCommit(_))
    }

    pub(super) fn reason(&self) -> String {
        match self {
            Self::StaleBase => CognitionCommitRejectReasonV1::StaleBase.code().to_string(),
            Self::WakeHandoff(reason) => reason.clone(),
            Self::PostCommit(reason) => reason.clone(),
            Self::Message(reason) => reason.clone(),
        }
    }
}
