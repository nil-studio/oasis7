//! Public admission and recovery handoff reuse one authorization/deadline path.
use super::*;
use crate::formal_live_runtime::pending_handoff::{PendingImportObservation, request_identity};
use oasis7_distfs::controlled_authority::formal::RecoveryPendingRequestV1;

enum AdmissionPhase {
    Public,
    RecoveryStartup,
}

impl FormalLiveService {
    pub(super) fn submit(
        &self,
        request: SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<IntentResponse<serde_json::Value>, (u16, &'static str, String)> {
        let _admission = self.admission_gate.lock().map_err(|_| {
            (
                503,
                "runtime_unavailable",
                "Node admission gate poisoned".into(),
            )
        })?;
        self.submit_locked(request, AdmissionPhase::Public)
    }

    fn submit_locked(
        &self,
        request: SubmitIntentRequest<WorldServicePayloadV1>,
        phase: AdmissionPhase,
    ) -> Result<IntentResponse<serde_json::Value>, (u16, &'static str, String)> {
        request
            .validate()
            .map_err(|error| (400, "invalid_request", error.to_string()))?;
        // Startup holds the original admission gate for the entire imported
        // set. Only its private caller may enqueue before publishing Ready.
        let available = match phase {
            AdmissionPhase::Public => self.runtime_status().ready,
            AdmissionPhase::RecoveryStartup => {
                self.state.node_running.load(Ordering::Acquire)
                    && !self.state.stopping.load(Ordering::Acquire)
            }
        };
        if !available {
            return Err((503, "not_ready", "formal runtime is not ready".into()));
        }
        if request
            .deadline_unix_ms
            .is_some_and(|deadline| deadline < now_unix_ms())
        {
            return Err((408, "expired", "submit deadline has expired".into()));
        }
        let cut = self
            .published
            .current()
            .map_err(|error| (503, "not_ready", error))?;
        if request.correlation.key.world != self.world {
            return Err((
                400,
                "world_mismatch",
                "submit world differs from formal runtime".into(),
            ));
        }
        let actual_correlation =
            correlation::derive_correlation(self.world.clone(), &request.signed_payload)
                .map_err(|error| (400, "invalid_payload", error))?;
        if actual_correlation != request.correlation {
            return Err((
                400,
                "correlation_mismatch",
                "signed payload correlation mismatch".into(),
            ));
        }
        validate_payload(&cut.world, &request.signed_payload)
            .map_err(|error| (400, "invalid_payload", error))?;
        if let WorldServicePayloadV1::GameParticipantRegister { owner, admission } =
            &request.signed_payload
        {
            let binding = cut
                .world
                .current_cognition_runtime_binding()
                .map_err(|e| (400, "invalid_payload", format!("{e:?}")))?;
            oasis7_runtime::service_admission::participant::validate_registration(
                &cut.world,
                owner,
                admission,
                &binding.branch_id,
                binding.reorg_epoch,
            )
            .map_err(|e| (400, "invalid_payload", e))?;
        }
        let key_digest = correlation::key_digest(&request.correlation.key)
            .map_err(|error| (400, "invalid_payload", error))?;
        let digest = blake3::hash(key_digest.as_bytes());
        let action_id =
            u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("hash width")).max(1);
        let action = oasis7_world_service::correlation::encode_consensus_intent(&request)
            .map_err(|error| (400, "invalid_payload", error))?;
        let runtime = self.runtime.lock().map_err(|_| {
            (
                503,
                "runtime_unavailable",
                "Node runtime lock poisoned".into(),
            )
        })?;
        let original = action.clone();
        let enqueue = || {
            runtime
                .submit_consensus_action_payload(action_id, action)
                .map_err(|error| format!("Node action queue rejected submit: {error:?}"))
        };
        match &self.pending_handoff {
            Some(pending) => pending.admit(request_identity(&request), original, enqueue),
            None => enqueue(),
        }
        .map_err(|error| (503, "action_queue_unavailable", error))?;
        Ok(IntentResponse {
            terminal_recovery_authority: None,
            formal_profile: Some(FormalReadProfileV1::controlled()),
            execution_evidence_scope: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation: actual_correlation,
            outcome: IntentOutcome::Received {
                durability: AdmissionDurability::Volatile,
            },
        })
    }

    /// The authenticated plan preserves originals, not permission to execute.
    /// Already canonical originals return their original result without queueing.
    pub(crate) fn resume_pending_handoff_locked(
        &self,
        original: &RecoveryPendingRequestV1,
        _admission: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<PendingImportObservation, String> {
        use oasis7_world_service::consensus_action_payload::{
            ConsensusActionPayloadBody, decode_consensus_action_payload_envelope,
        };
        let envelope = decode_consensus_action_payload_envelope(&original.signed_request)?;
        let ConsensusActionPayloadBody::RuntimeAction {
            action: oasis7_runtime::Action::WorldServiceIntent { request },
        } = envelope.body
        else {
            return Err("recovery pending original is not a public Submit intent".into());
        };
        let request: SubmitIntentRequest<WorldServicePayloadV1> =
            serde_json::from_value(request).map_err(|error| error.to_string())?;
        request.validate().map_err(|error| error.to_string())?;
        if request.correlation.key.world != self.world
            || request_identity(&request) != original.identity
            || correlation::derive_correlation(self.world.clone(), &request.signed_payload)?
                != request.correlation
            || oasis7_world_service::correlation::encode_consensus_intent(&request)?
                != original.signed_request
        {
            return Err("recovery pending original world/identity/signed bytes mismatch".into());
        }
        let cut = self.published.current()?;
        let key = correlation::key_digest(&request.correlation.key)?;
        let canonical: Option<oasis7_world_service::CanonicalIntentResultV1> = if let Some(value) =
            cut.world
                .capability_revocation_state()
                .world_service_results
                .get(&key)
        {
            correlation::validate_result(&key, value)?;
            Some(serde_json::from_value(value.clone()).map_err(|error| error.to_string())?)
        } else if cut.world.state().retained_history_root.is_some() {
            cut.world
                .read_retained_world_service_result(&key)
                .map_err(|error| error.to_string())?
        } else {
            None
        };
        if let Some(canonical) = canonical {
            if canonical.request != request {
                return Err(
                    "recovery pending identity conflicts with its canonical original result".into(),
                );
            }
            return Ok(PendingImportObservation::AlreadyCanonical);
        }
        match self.submit_locked(request, AdmissionPhase::RecoveryStartup) {
            Ok(response)
                if matches!(
                    response.outcome,
                    IntentOutcome::Received {
                        durability: AdmissionDurability::Volatile,
                    }
                ) =>
            {
                Ok(PendingImportObservation::ReceivedVolatile)
            }
            Ok(_) => Err("recovery pending Submit returned an unexpected admission outcome".into()),
            Err((status @ (400 | 408), code, _)) => {
                Ok(PendingImportObservation::Rejected { status, code })
            }
            Err((status, code, _)) => Err(format!(
                "recovery pending original remains unresolved: HTTP {status} {code}"
            )),
        }
    }
}
