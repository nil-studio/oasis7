//! One-shot hold for the real signed Admit that the test will stale before dispatch.
use super::*;
use std::path::Path;

const CANDIDATE_WAIT_BUDGET: Duration = Duration::from_secs(20);
const HOLD_BUDGET: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
enum CandidateValidationStage {
    SubmitEnvelope,
    SchedulerPayloadKind,
    SchedulerSignature,
    AdmitOperation,
    CapturedBaseValidity,
    ProposalSelfDigest,
    Correlation,
    SignedIdentity,
    ArmedTuple,
    CandidateEvidenceDigest,
}

impl CandidateValidationStage {
    fn code(self) -> &'static str {
        match self {
            Self::SubmitEnvelope => "submit_envelope",
            Self::SchedulerPayloadKind => "scheduler_payload_kind",
            Self::SchedulerSignature => "scheduler_signature",
            Self::AdmitOperation => "admit_operation",
            Self::CapturedBaseValidity => "captured_base_validity",
            Self::ProposalSelfDigest => "proposal_self_digest",
            Self::Correlation => "correlation",
            Self::SignedIdentity => "signed_identity",
            Self::ArmedTuple => "armed_tuple",
            Self::CandidateEvidenceDigest => "candidate_evidence_digest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdmitRejectedArm {
    world: WorldIdentity,
    agent_id: String,
    origin_request_digest: String,
    continuation_proposal_id_digest: String,
}

#[derive(Clone)]
struct AdmitRejectedCandidate {
    world: WorldIdentity,
    agent_id: String,
    origin_request_digest: String,
    continuation_proposal_id_digest: String,
    proposal_digest: String,
    identity_digest: String,
    correlation_digest: String,
    payload_digest: String,
    request_bytes_digest: String,
    captured_base_binding_digest: String,
    captured_base_binding: oasis7_runtime::RuntimeCognitionBaseBindingV1,
    original_request: SubmitIntentRequest<WorldServicePayloadV1>,
}

#[derive(Default)]
pub(in super::super) struct AdmitRejectedGateState {
    arm: Option<AdmitRejectedArm>,
    candidate: Option<AdmitRejectedCandidate>,
    candidate_conflict: bool,
    claimed: bool,
    owner: Option<thread::ThreadId>,
    released: bool,
    handler_returned: bool,
    joined: bool,
}

/// Safe in-process evidence for the exact request currently held by the
/// listener. `original_request` is intentionally not serializable or logged.
#[derive(Clone)]
pub(in super::super) struct AdmitRejectedCandidateEvidence {
    original_request: SubmitIntentRequest<WorldServicePayloadV1>,
}

impl AdmitRejectedCandidateEvidence {
    pub(in super::super) fn original_request(&self) -> &SubmitIntentRequest<WorldServicePayloadV1> {
        &self.original_request
    }
}

fn opaque_digest(domain: &str, value: &str) -> Result<String, String> {
    oasis7::world_service::authority::request_digest(domain, &value)
}

#[expect(
    clippy::too_many_arguments,
    reason = "The candidate digest explicitly joins the complete authenticated request and captured canonical boundary"
)]
fn candidate_identity_digest(
    world: &WorldIdentity,
    agent_id: &str,
    origin_request_digest: &str,
    continuation_proposal_id_digest: &str,
    proposal_digest: &str,
    correlation_digest: &str,
    payload_digest: &str,
    captured_base_binding_digest: &str,
) -> Result<String, String> {
    oasis7::world_service::authority::request_digest(
        "admit-rejection-candidate-v1",
        &(
            world,
            agent_id,
            origin_request_digest,
            continuation_proposal_id_digest,
            proposal_digest,
            correlation_digest,
            payload_digest,
            captured_base_binding_digest,
        ),
    )
}

fn parse_candidate(
    arm: &AdmitRejectedArm,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    request_bytes: &[u8],
) -> Result<AdmitRejectedCandidate, CandidateValidationStage> {
    request
        .validate()
        .map_err(|_| CandidateValidationStage::SubmitEnvelope)?;
    let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
        return Err(CandidateValidationStage::SchedulerPayloadKind);
    };
    oasis7::world_service::authority::verify_read_request("scheduler", signed)
        .map_err(|_| CandidateValidationStage::SchedulerSignature)?;
    let SchedulerOperationV1::AdmitContinuation(proposal) = &signed.request.operation else {
        return Err(CandidateValidationStage::AdmitOperation);
    };
    // The proposal keeps its originating Wait binding; the signed request
    // separately captures the scheduler's current base for stale-base checks.
    let base = &signed.request.captured_base_binding;
    base.validate()
        .map_err(|_| CandidateValidationStage::CapturedBaseValidity)?;
    if proposal.proposal_digest != proposal.proposal_digest() {
        return Err(CandidateValidationStage::ProposalSelfDigest);
    }
    let derived = correlation::derive_correlation(
        request.correlation.key.world.clone(),
        &request.signed_payload,
    )
    .map_err(|_| CandidateValidationStage::Correlation)?;
    if derived != request.correlation {
        return Err(CandidateValidationStage::Correlation);
    }
    if request.correlation.key.world != arm.world
        || request.correlation.key.operation_domain != "scheduler"
        || request.correlation.key.nonce_scope != "agent_scheduler"
        || proposal.world_id != arm.world.world_id
        || proposal.agent_id != arm.agent_id
        || signed.request.agent_id != arm.agent_id
        || base.world_id != arm.world.world_id
        || proposal.world_id != base.world_id
    {
        return Err(CandidateValidationStage::SignedIdentity);
    }
    let origin_request_digest = opaque_digest(
        "admit-rejection-origin-request-v1",
        &proposal.origin_request_digest,
    )
    .map_err(|_| CandidateValidationStage::ArmedTuple)?;
    let continuation_proposal_id_digest = opaque_digest(
        "admit-rejection-proposal-id-v1",
        &proposal.continuation_proposal_id,
    )
    .map_err(|_| CandidateValidationStage::ArmedTuple)?;
    if origin_request_digest != arm.origin_request_digest
        || continuation_proposal_id_digest != arm.continuation_proposal_id_digest
    {
        return Err(CandidateValidationStage::ArmedTuple);
    }
    let correlation_digest = correlation::key_digest(&derived.key)
        .map_err(|_| CandidateValidationStage::CandidateEvidenceDigest)?;
    let payload_digest = derived.payload_digest;
    let captured_base_binding_digest =
        oasis7::world_service::authority::request_digest("admit-rejection-captured-base-v1", base)
            .map_err(|_| CandidateValidationStage::CandidateEvidenceDigest)?;
    let identity_digest = candidate_identity_digest(
        &arm.world,
        &arm.agent_id,
        &origin_request_digest,
        &continuation_proposal_id_digest,
        &proposal.proposal_digest,
        &correlation_digest,
        &payload_digest,
        &captured_base_binding_digest,
    )
    .map_err(|_| CandidateValidationStage::CandidateEvidenceDigest)?;
    Ok(AdmitRejectedCandidate {
        world: arm.world.clone(),
        agent_id: arm.agent_id.clone(),
        origin_request_digest,
        continuation_proposal_id_digest,
        proposal_digest: proposal.proposal_digest.clone(),
        identity_digest,
        correlation_digest,
        payload_digest,
        request_bytes_digest: blake3::hash(request_bytes).to_hex().to_string(),
        captured_base_binding_digest,
        captured_base_binding: base.clone(),
        original_request: request.clone(),
    })
}

fn candidate_evidence(candidate: &AdmitRejectedCandidate) -> AdmitRejectedCandidateEvidence {
    AdmitRejectedCandidateEvidence {
        original_request: candidate.original_request.clone(),
    }
}

fn maybe_admit_continuation(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Option<(&str, &str)> {
    let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
        return None;
    };
    let SchedulerOperationV1::AdmitContinuation(proposal) = &signed.request.operation else {
        return None;
    };
    Some((
        proposal.origin_request_digest.as_str(),
        proposal.continuation_proposal_id.as_str(),
    ))
}

impl super::WorldGate {
    pub(in super::super) fn arm_admit_rejected_gate(
        &self,
        expected_world: WorldIdentity,
        expected_agent_id: &str,
        origin_request_digest: &str,
        continuation_proposal_id: &str,
    ) -> Result<(), String> {
        expected_world
            .validate()
            .map_err(|_| "Admit rejection gate world is invalid".to_string())?;
        if expected_agent_id.trim().is_empty()
            || origin_request_digest.trim().is_empty()
            || continuation_proposal_id.trim().is_empty()
        {
            return Err("Admit rejection gate arm identity is empty".into());
        }
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("Admit rejection gate root is not configured")?;
        let arm = AdmitRejectedArm {
            world: expected_world,
            agent_id: expected_agent_id.to_string(),
            origin_request_digest: opaque_digest(
                "admit-rejection-origin-request-v1",
                origin_request_digest,
            )?,
            continuation_proposal_id_digest: opaque_digest(
                "admit-rejection-proposal-id-v1",
                continuation_proposal_id,
            )?,
        };
        let mut state = self.rejected_admit.lock().unwrap();
        if state.arm.is_some() || state.candidate.is_some() || state.candidate_conflict {
            return Err("Admit rejection gate arm is one-shot".into());
        }
        super::write_new_json(
            &root.join("world-admit-rejected-arm.json"),
            &serde_json::json!({
                "operation_kind": "AdmitContinuation",
                "world": arm.world,
                "agent_id": arm.agent_id,
                "origin_request_digest": arm.origin_request_digest,
                "continuation_proposal_id_digest": arm.continuation_proposal_id_digest,
            }),
        )
        .map_err(|error| format!("immutable Admit rejection arm failed: {error}"))?;
        state.arm = Some(arm);
        Ok(())
    }

    pub(in super::super) fn wait_admit_rejected_candidate(
        &self,
        timeout: Duration,
    ) -> Result<AdmitRejectedCandidateEvidence, String> {
        let mut state = self.rejected_admit.lock().unwrap();
        let deadline = Instant::now() + timeout.min(CANDIDATE_WAIT_BUDGET);
        while state.candidate.is_none() && !state.candidate_conflict && Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (next, _) = self
                .rejected_admit_changed
                .wait_timeout(state, remaining)
                .unwrap();
            state = next;
        }
        if state.candidate_conflict {
            return Err("Admit rejection gate observed an ambiguous candidate".into());
        }
        state
            .candidate
            .as_ref()
            .map(candidate_evidence)
            .ok_or_else(|| "actual signed Admit candidate timed out".into())
    }

    pub(in super::super) fn release_admit_rejected_gate_after_clock_commit(
        &self,
        post_commit_view: &oasis7::world_service::verified_view::VerifiedWorldView,
    ) -> Result<(), String> {
        let root = self
            .root
            .lock()
            .unwrap()
            .clone()
            .ok_or("Admit rejection gate root is not configured")?;
        let mut state = self.rejected_admit.lock().unwrap();
        let candidate = state
            .candidate
            .as_ref()
            .ok_or("Admit rejection candidate is not held")?;
        if state.candidate_conflict || !state.claimed || state.released {
            return Err("Admit rejection gate is not held for one-shot release".into());
        }
        let commit = &post_commit_view.version().commit;
        commit
            .validate()
            .map_err(|_| "post-commit View CommitRef is invalid".to_string())?;
        let expected_scope = format!("agent:{}", candidate.agent_id);
        if commit.world != candidate.world
            || post_commit_view.version().visibility_scope != expected_scope
            || post_commit_view.continuation().scope_id != expected_scope
            || post_commit_view.continuation().commit != *commit
        {
            return Err("post-commit verified View identity differs from held Admit".into());
        }
        let runtime_binding = post_commit_view
            .projection()
            .runtime_binding
            .as_ref()
            .ok_or("post-commit verified View has no runtime base binding")?;
        let captured = &candidate.captured_base_binding;
        let base_unchanged = runtime_binding.world_id == captured.world_id
            && runtime_binding.branch_id == captured.branch_id
            && runtime_binding.finality_epoch == captured.finality_epoch
            && runtime_binding
                .finality_block_hash
                .as_ref()
                .map(ToString::to_string)
                == captured.finality_block_hash
            && runtime_binding.finality_status == captured.finality_status
            && runtime_binding.base_tick == captured.base_tick
            && runtime_binding.base_world_hash.to_string() == captured.base_world_hash
            && runtime_binding.reorg_epoch == captured.reorg_epoch
            && runtime_binding.runtime_manifest_hash.to_string() == captured.runtime_manifest_hash;
        if base_unchanged {
            return Err("verified post-commit View did not change the captured Admit base".into());
        }
        let current_base_binding_digest = oasis7::world_service::authority::request_digest(
            "admit-rejection-post-commit-base-v1",
            runtime_binding,
        )?;
        let release = serde_json::json!({
            "operation_kind": "AdmitContinuation",
            "candidate_identity_digest": candidate.identity_digest,
            "correlation_digest": candidate.correlation_digest,
            "payload_digest": candidate.payload_digest,
            "request_bytes_digest": candidate.request_bytes_digest,
            "captured_base_binding_digest": candidate.captured_base_binding_digest,
            "post_commit_base_binding_digest": current_base_binding_digest,
            "captured_base_binding_changed": true,
            "post_commit_view_verified": true,
            "post_commit_ref": commit,
            "original_request_bytes_preserved": true,
        });
        super::write_new_json(&root.join("world-admit-rejected-release.json"), &release)
            .map_err(|error| format!("immutable Admit rejection release failed: {error}"))?;
        state.released = true;
        self.rejected_admit_changed.notify_all();
        Ok(())
    }

    pub(in super::super) fn pause_rejected_admit_submit(
        &self,
        root: &Path,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
        original_bytes: &[u8],
    ) {
        let (origin, proposal_id) = match maybe_admit_continuation(request) {
            Some(tuple) => tuple,
            None => return,
        };
        let mut state = self.rejected_admit.lock().unwrap();
        let Some(arm) = state.arm.clone() else {
            return;
        };
        let origin_digest = match opaque_digest("admit-rejection-origin-request-v1", origin) {
            Ok(value) => value,
            Err(_) => return,
        };
        let proposal_id_digest = match opaque_digest("admit-rejection-proposal-id-v1", proposal_id)
        {
            Ok(value) => value,
            Err(_) => return,
        };
        if origin_digest != arm.origin_request_digest
            || proposal_id_digest != arm.continuation_proposal_id_digest
        {
            return;
        }
        let parsed = parse_candidate(&arm, request, original_bytes);
        let candidate = match parsed {
            Ok(candidate) => candidate,
            Err(stage) => {
                state.candidate_conflict = true;
                super::write_new_json(
                    &root.join("world-admit-rejected-conflict.json"),
                    &serde_json::json!({
                        "operation_kind": "AdmitContinuation",
                        "candidate_validation_failed": true,
                        "validation_stage": stage.code(),
                        "origin_request_digest": origin_digest,
                        "continuation_proposal_id_digest": proposal_id_digest,
                    }),
                )
                .expect("immutable invalid Admit candidate marker must be written");
                self.rejected_admit_changed.notify_all();
                return;
            }
        };
        if candidate.world != arm.world || candidate.agent_id != arm.agent_id {
            state.candidate_conflict = true;
            super::write_new_json(
                &root.join("world-admit-rejected-conflict.json"),
                &serde_json::json!({
                    "operation_kind": "AdmitContinuation",
                    "world_or_agent_mismatch": true,
                    "candidate_identity_digest": candidate.identity_digest,
                }),
            )
            .expect("immutable mismatched Admit candidate marker must be written");
            self.rejected_admit_changed.notify_all();
            return;
        }
        if state.candidate.is_some() || state.claimed {
            state.candidate_conflict = true;
            super::write_new_json(
                &root.join("world-admit-rejected-conflict.json"),
                &serde_json::json!({
                    "operation_kind": "AdmitContinuation",
                    "duplicate_candidate": true,
                    "candidate_identity_digest": candidate.identity_digest,
                }),
            )
            .expect("immutable duplicate Admit candidate marker must be written");
            self.rejected_admit_changed.notify_all();
            return;
        }
        let started = serde_json::json!({
            "operation_kind": "AdmitContinuation",
            "signature_verified": true,
            "request_correlation_verified": true,
            "canonical_proposal_self_digest_verified": true,
            "candidate_identity_digest": candidate.identity_digest,
            "world": candidate.world,
            "agent_id": candidate.agent_id,
            "origin_request_digest": candidate.origin_request_digest,
            "continuation_proposal_id_digest": candidate.continuation_proposal_id_digest,
            "proposal_digest": candidate.proposal_digest,
            "correlation_digest": candidate.correlation_digest,
            "payload_digest": candidate.payload_digest,
            "request_bytes_digest": candidate.request_bytes_digest,
            "captured_base_binding_digest": candidate.captured_base_binding_digest,
            "pre_maybe_handle": true,
        });
        if let Err(error) =
            super::write_new_json(&root.join("world-admit-rejected-started.json"), &started)
        {
            state.candidate_conflict = true;
            self.rejected_admit_changed.notify_all();
            panic!("immutable Admit rejection candidate marker failed: {error}");
        }
        state.owner = Some(thread::current().id());
        state.claimed = true;
        state.candidate = Some(candidate.clone());
        self.rejected_admit_changed.notify_all();

        let deadline = Instant::now() + HOLD_BUDGET;
        while !state.released && !state.candidate_conflict && Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (next, _) = self
                .rejected_admit_changed
                .wait_timeout(state, remaining)
                .unwrap();
            state = next;
        }
        if state.released && !state.candidate_conflict {
            let bytes_unchanged =
                blake3::hash(original_bytes).to_hex().to_string() == candidate.request_bytes_digest;
            if !bytes_unchanged {
                state.candidate_conflict = true;
                super::write_new_json(
                    &root.join("world-admit-rejected-bytes-changed.json"),
                    &serde_json::json!({"original_request_bytes_preserved": false}),
                )
                .expect("immutable changed-Admit-bytes marker must be written");
                self.rejected_admit_changed.notify_all();
                panic!("held Admit request bytes changed before real dispatch");
            }
            super::write_new_json(
                &root.join("world-admit-rejected-gate-returned.json"),
                &serde_json::json!({
                    "gate_returned": true,
                    "original_request_bytes_preserved": true,
                    "candidate_identity_digest": candidate.identity_digest,
                    "pre_maybe_handle": true,
                }),
            )
            .expect("immutable Admit rejection gate-returned marker must be written");
        } else {
            super::write_new_json(
                &root.join("world-admit-rejected-timeout.json"),
                &serde_json::json!({
                    "gate_returned": false,
                    "release_observed": state.released,
                    "candidate_conflict": state.candidate_conflict,
                    "candidate_identity_digest": candidate.identity_digest,
                }),
            )
            .expect("immutable Admit rejection timeout marker must be written");
        }
    }

    pub(in super::super) fn rejected_admit_connection_worker_finished(
        &self,
        root: &Path,
        current: thread::ThreadId,
    ) {
        let marker = {
            let mut state = self.rejected_admit.lock().unwrap();
            if state.owner == Some(current) && state.released && !state.handler_returned {
                state.handler_returned = true;
                state.candidate.as_ref().map(|candidate| {
                    serde_json::json!({
                        "connection_handler_returned": true,
                        "candidate_identity_digest": candidate.identity_digest,
                        "original_request_bytes_preserved": true,
                    })
                })
            } else {
                None
            }
        };
        if let Some(marker) = marker {
            super::write_new_json(
                &root.join("world-admit-rejected-handler-returned.json"),
                &marker,
            )
            .expect("immutable actual Admit handler-returned marker must be unique");
        }
    }

    pub(in super::super) fn rejected_admit_connection_worker_joined(
        &self,
        root: &Path,
        worker_id: thread::ThreadId,
    ) {
        let marker = {
            let mut state = self.rejected_admit.lock().unwrap();
            if state.owner == Some(worker_id)
                && state.released
                && state.handler_returned
                && !state.joined
            {
                state.joined = true;
                state.candidate.as_ref().map(|candidate| {
                    serde_json::json!({
                        "listener_owner_join_observed": true,
                        "worker_join_succeeded": true,
                        "candidate_identity_digest": candidate.identity_digest,
                        "original_request_bytes_preserved": true,
                    })
                })
            } else {
                None
            }
        };
        if let Some(marker) = marker {
            super::write_new_json(
                &root.join("world-admit-rejected-worker-joined.json"),
                &marker,
            )
            .expect("immutable actual Admit worker-joined marker must be unique");
        }
    }
}
