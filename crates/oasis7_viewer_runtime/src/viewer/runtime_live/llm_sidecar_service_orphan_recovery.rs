//! Recover an original hosted dispatch without repeating an external model call.
use super::*;
use crate::viewer::runtime_live::{
    agent_service_io::{AgentServiceIoOperation, AgentServiceIoResponse},
    agent_service_phase::{HostedServicePhase, original_intent_io, original_response},
};
use oasis7_client_api::world_service::*;

const ORPHAN_REASON: &str = "executor_lost_after_dispatch:model_outcome_unknown";

pub(super) fn same_provider_context(
    active: &cognition_context::ProviderContextState,
    candidate: &cognition_context::ProviderContextState,
) -> bool {
    serde_json::to_value(active).is_ok_and(|original| {
        serde_json::to_value(candidate).is_ok_and(|retained| original == retained)
    })
}

#[derive(Clone, Debug, serde::Serialize)]
pub(in crate::viewer::runtime_live) struct OrphanPhase {
    pub(in crate::viewer::runtime_live) agent: String,
    stage: String,
    pub(in crate::viewer::runtime_live) submit: bool,
    commit: Option<CommitRef>,
    original_id: Option<String>,
}
impl OrphanPhase {
    pub(in crate::viewer::runtime_live) fn identity(&self) -> Result<String, String> {
        let mut identity = self.clone();
        identity.submit = false; // Dispatch reservation is not phase identity.
        let bytes = serde_json::to_vec(&identity).map_err(|e| e.to_string())?;
        Ok(format!("orphan:{}", blake3::hash(&bytes).to_hex()))
    }
}

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn classify_hosted_orphans(&mut self) {
        // Process-local workers cannot survive this restore. Preserve the
        // exact marker and lease until canonical orphan reconciliation ends.
        for (agent, active) in self.provider_active_turns.clone() {
            let retained = self
                .provider_held_decisions
                .get(&agent)
                .and_then(|d| d.cognition.as_ref())
                .is_some_and(|c| same_provider_context(&active, &c.request))
                || self
                    .provider_completed_decisions
                    .iter()
                    .filter_map(|d| d.cognition.as_ref())
                    .any(|c| same_provider_context(&active, &c.request))
                || self
                    .provider_service_pending
                    .get(&agent)
                    .is_some_and(|p| same_provider_context(&active, &p.cognition.request))
                || self
                    .hosted_admission
                    .as_ref()
                    .is_some_and(|p| p.owns_provider_context(&active))
                || self
                    .hosted_wait
                    .as_ref()
                    .is_some_and(|p| p.owns_provider_context(&active))
                || self
                    .hosted_resume
                    .as_ref()
                    .is_some_and(|p| p.owns_provider_context(&active));
            if !retained {
                self.provider_recovery_pending.entry(agent).or_insert(
                    lineage_persistence::ProviderRecoveryPending {
                        active,
                        reason: "executor_lost_after_dispatch:model_outcome_unknown".into(),
                    },
                );
            }
        }
    }

    pub(in crate::viewer::runtime_live) fn begin_hosted_orphan_recovery(&mut self) {
        if let Some((agent, _)) = self
            .provider_recovery_pending
            .iter()
            .find(|(_, p)| p.reason.starts_with(ORPHAN_REASON))
        {
            self.hosted_service_phase = Some(HostedServicePhase::Orphan(OrphanPhase {
                agent: agent.clone(),
                stage: "view".into(),
                submit: false,
                commit: None,
                original_id: None,
            }));
        }
    }

    fn orphan_context(
        &self,
        agent: &str,
    ) -> Result<cognition_context::ProviderContextState, String> {
        let recovery = self
            .provider_recovery_pending
            .get(agent)
            .ok_or("orphan recovery missing")?;
        for context in [
            self.provider_active_turns.get(agent),
            self.provider_contexts.get(agent),
        ] {
            if serde_json::to_value(context).map_err(|e| e.to_string())?
                != serde_json::to_value(Some(&recovery.active)).map_err(|e| e.to_string())?
            {
                return Err("orphan original context mismatch; model_outcome_unknown".into());
            }
        }
        Ok(recovery.active.clone())
    }

    /// The projection is installed only by the authenticated read-view path.
    fn validate_orphan_authority(
        &self,
        agent: &str,
    ) -> Result<crate::runtime::CognitionLeaseV1, String> {
        let context = self.orphan_context(agent)?;
        let request = &context.request_context;
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("orphan verified view missing")?;
        let binding = view
            .runtime_binding
            .as_ref()
            .ok_or("orphan canonical binding missing")?;
        let authority = view
            .agent_context
            .as_ref()
            .filter(|a| a.agent_id == agent)
            .ok_or("orphan current Agent authority missing")?;
        let invocation = request
            .base_decision_request
            .capability_invocation_context
            .as_ref()
            .ok_or("orphan original capability missing")?;
        let saved = &request.runtime_binding;
        if saved.world_id != binding.world_id
            || saved.branch_id != binding.branch_id
            || saved.reorg_epoch != binding.reorg_epoch
            || saved.finality_epoch != binding.finality_epoch
            || saved.finality_block_hash != binding.finality_block_hash
            || saved.finality_status != binding.finality_status
            || saved.runtime_manifest_hash != binding.runtime_manifest_hash
            || invocation.subject != authority.capability_invocation_context.subject
            || invocation.grant_id != authority.capability_invocation_context.grant_id
        {
            return Err(
                "orphan current authority or Runtime binding changed; model_outcome_unknown".into(),
            );
        }
        let lease = self
            .provider_cognition_leases
            .get(agent)
            .ok_or("orphan original lease missing")?;
        lineage_generation_recovery::validate_provider_lease_identity(agent, request, lease)?;
        if lease.reserved_amount != 1 {
            return Err("orphan recovery requires original fixed cognition unit".into());
        }
        let canonical = view
            .cognition_leases
            .iter()
            .find(|l| l.lease_id == lease.lease_id)
            .ok_or("orphan canonical lease missing")?;
        // Keep the local original Reserved; validate the canonical lifecycle
        // separately instead of changing a genuine Settled record's status.
        if canonical.schema_version != lease.schema_version
            || canonical.lease_id != lease.lease_id
            || canonical.idempotency_key != lease.idempotency_key
            || canonical.account_id != lease.account_id
            || canonical.agent_id != lease.agent_id
            || canonical.agent_session_id != lease.agent_session_id
            || canonical.agent_turn_id != lease.agent_turn_id
            || canonical.decision_request_id != lease.decision_request_id
            || canonical.request_digest != lease.request_digest
            || canonical.reserved_at_tick != lease.reserved_at_tick
        {
            return Err("orphan canonical lease immutable identity changed".into());
        }
        canonical
            .validate()
            .map_err(|e| format!("orphan canonical lease invalid: {e:?}"))?;
        if !matches!(
            canonical.status,
            crate::runtime::CognitionLeaseStatusV1::Reserved
                | crate::runtime::CognitionLeaseStatusV1::Settled
        ) || canonical.refunded_amount != 0
            || canonical.released_amount != 0
            || canonical.quote != lease.quote
            || canonical.reserved_amount != lease.reserved_amount
        {
            return Err("orphan canonical lease accounting changed".into());
        }
        Ok(canonical.clone())
    }

    fn original_orphan_dispatch(&self, agent: &str) -> Result<String, String> {
        use crate::world_service::wire::{SchedulerOperationV1, WorldServicePayloadV1};
        let context = self.orphan_context(agent)?;
        let request = &context.request_context;
        let config = self
            .provider_service_config
            .as_ref()
            .ok_or("orphan service missing")?;
        let signer = self
            .provider_service_signer
            .as_ref()
            .ok_or("orphan current signer missing")?;
        let key: [u8; 32] = hex::decode(&signer.private_key_hex)
            .map_err(|_| "invalid Agent signer")?
            .try_into()
            .map_err(|_| "invalid Agent signer")?;
        let public = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&key)
                .verifying_key()
                .to_bytes(),
        );
        let expected = SchedulerOperationV1::ProviderPrefix {
            request: crate::runtime_host::feedback::request_context_to_api(request),
            context_digest: async_support::runtime_provider_context_digest(request),
        };
        let mut originals = Vec::new();
        for (id, pending) in &self.provider_scheduler_pending {
            let WorldServicePayloadV1::Scheduler(signed) = &pending.payload else {
                continue;
            };
            let matches = signed.request.operation == expected
                || (matches!(
                    signed.request.operation,
                    SchedulerOperationV1::ResumeWake { .. }
                ) && pending.resume_context.as_ref().is_some_and(|saved| {
                    serde_json::to_value(saved).ok() == serde_json::to_value(&context).ok()
                }));
            if !matches {
                continue;
            }
            Self::validate_scheduler_checkpoint_integrity(&config.expected_world, id, pending)?;
            if signed.request.agent_id != agent
                || signed.subject_public_key != public
                || signed.request.delegation_generation != signer.delegation_generation
            {
                return Err("orphan original dispatch signer or delegation changed; fenced".into());
            }
            originals.push(id.clone());
        }
        if originals.len() != 1 {
            return Err("orphan original canonical dispatch correlation missing or ambiguous; model_outcome_unknown".into());
        }
        Ok(originals.remove(0))
    }

    pub(in crate::viewer::runtime_live) fn hosted_orphan_operation(
        &mut self,
        phase: &OrphanPhase,
    ) -> Result<AgentServiceIoOperation, String> {
        if phase.stage.ends_with("view") {
            let config = self
                .provider_service_config
                .as_ref()
                .ok_or("orphan service missing")?;
            return Ok(AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: config.expected_world.clone(),
                scope_id: config.scope_id.clone(),
                min_commit: phase.commit.clone(),
                fixed_commit: None,
                deadline_unix_ms: None,
            }));
        }
        self.validate_orphan_authority(&phase.agent)?;
        let context = self.orphan_context(&phase.agent)?;
        let request = &context.request_context;
        let pending = match phase.stage.as_str() {
            "dispatch" => self
                .provider_scheduler_pending
                .get(
                    phase
                        .original_id
                        .as_ref()
                        .ok_or("orphan original dispatch ID missing")?,
                )
                .cloned()
                .ok_or("orphan original dispatch checkpoint missing")?,
            "failure" => {
                self.prepare_service_scheduler_checkpoint(
                    request,
                    &format!("failure:{}", request.transport_attempt),
                    crate::world_service::wire::SchedulerOperationV1::ProviderFailure {
                        request: crate::runtime_host::feedback::request_context_to_api(request),
                        reason: "cognition_failed".into(),
                    },
                    None,
                )?
                .0
            }
            "settle" => {
                let lease = self
                    .provider_cognition_leases
                    .get(&phase.agent)
                    .ok_or("orphan lease missing")?;
                self.prepare_service_scheduler_checkpoint(
                    request,
                    "settle",
                    crate::world_service::wire::SchedulerOperationV1::SettleLease {
                        lease_id: lease.lease_id.clone(),
                        consumed_amount: 1,
                    },
                    None,
                )?
                .0
            }
            _ => return Err("unknown orphan recovery phase".into()),
        };
        Ok(original_intent_io(
            &pending.correlation,
            &pending.payload,
            phase.submit && phase.stage != "dispatch",
        ))
    }

    fn orphan_pending_id(&self, phase: &OrphanPhase) -> Result<String, String> {
        let context = self.orphan_context(&phase.agent)?;
        let request = context.request_context;
        Ok(match phase.stage.as_str() {
            "dispatch" => phase
                .original_id
                .clone()
                .ok_or("orphan original dispatch missing")?,
            "failure" => format!(
                "{}:failure:{}",
                request.provider_invocation_key(),
                request.transport_attempt
            ),
            "settle" => format!("{}:settle", request.provider_invocation_key()),
            _ => return Err("orphan phase has no canonical request".into()),
        })
    }

    fn retire_canonical_orphan(&mut self, agent: &str, original_id: &str) -> Result<(), String> {
        let lease = self.validate_orphan_authority(agent)?;
        if lease.status != crate::runtime::CognitionLeaseStatusV1::Settled
            || lease.settled_amount != 1
            || lease.refunded_amount != 0
            || lease.released_amount != 0
            || lease.compensated_amount != 0
            || lease.net_amount != 1
        {
            return Err("orphan canonical settled readback missing; original retained".into());
        }
        // Rollback the complete private checkpoint if persistence fails. Neither
        // Runtime nor the external model ledger is mutated here.
        let active = self.provider_active_turns.remove(agent);
        let context = self.provider_contexts.remove(agent);
        let recovery = self.provider_recovery_pending.remove(agent);
        let original_lease = self.provider_cognition_leases.remove(agent);
        let original_dispatch = self.provider_scheduler_pending.remove(original_id);
        if let Err(error) = self.persist_provider_lineage() {
            if let Some(v) = active {
                self.provider_active_turns.insert(agent.into(), v);
            }
            if let Some(v) = context {
                self.provider_contexts.insert(agent.into(), v);
            }
            if let Some(v) = recovery {
                self.provider_recovery_pending.insert(agent.into(), v);
            }
            if let Some(v) = original_lease {
                self.provider_cognition_leases.insert(agent.into(), v);
            }
            if let Some(v) = original_dispatch {
                self.provider_scheduler_pending
                    .insert(original_id.into(), v);
            }
            return Err(error);
        }
        Ok(())
    }
}

impl crate::viewer::ViewerRuntimeLiveServer {
    pub(in crate::viewer::runtime_live) fn apply_hosted_orphan_response(
        &mut self,
        mut phase: OrphanPhase,
        response: AgentServiceIoResponse,
    ) -> Result<(), String> {
        if phase.stage.ends_with("view") {
            let AgentServiceIoResponse::View(view) = response else {
                return Err("orphan expected authenticated view".into());
            };
            if let Some(commit) = &phase.commit
                && !view
                    .version()
                    .commit
                    .satisfies_minimum(commit)
                    .map_err(|e| e.to_string())?
            {
                return Err("orphan readback precedes canonical commit".into());
            }
            self.apply_hosted_verified_view(*view)?;
            self.llm_sidecar.validate_orphan_authority(&phase.agent)?;
            match phase.stage.as_str() {
                "view" => {
                    phase.original_id =
                        Some(self.llm_sidecar.original_orphan_dispatch(&phase.agent)?);
                    phase.stage = "dispatch".into();
                }
                "failure_view" => phase.stage = "settle".into(),
                "settled_view" => {
                    self.llm_sidecar.retire_canonical_orphan(
                        &phase.agent,
                        phase
                            .original_id
                            .as_deref()
                            .ok_or("orphan original ID missing")?,
                    )?;
                    self.llm_sidecar.hosted_service_phase = None;
                    self.llm_sidecar.hosted_service_config_binding = None;
                    return Ok(());
                }
                _ => return Err("unknown orphan view stage".into()),
            }
            phase.submit = false;
            self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Orphan(phase));
            return Ok(());
        }
        let Some(response) = original_response(response)? else {
            return Ok(());
        };
        let id = self.llm_sidecar.orphan_pending_id(&phase)?;
        let pending = self
            .llm_sidecar
            .provider_scheduler_pending
            .get(&id)
            .ok_or("orphan canonical original missing")?;
        response
            .validate(&pending.correlation)
            .map_err(|e| e.to_string())?;
        match response.outcome {
            IntentOutcome::Committed { commit, receipt } => {
                let context = self.llm_sidecar.orphan_context(&phase.agent)?;
                let request = &context.request_context;
                if phase.stage == "settle" {
                    RuntimeLlmSidecar::validate_hosted_settlement_receipt(
                        request,
                        self.llm_sidecar
                            .provider_cognition_leases
                            .get(&phase.agent)
                            .ok_or("orphan lease missing")?,
                        receipt,
                    )?;
                    phase.stage = "settled_view".into();
                } else {
                    if phase.stage == "dispatch" {
                        let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
                            &pending.payload
                        else {
                            return Err("orphan dispatch operation mismatch".into());
                        };
                        match &signed.request.operation {
                            crate::world_service::wire::SchedulerOperationV1::ProviderPrefix {
                                ..
                            } => {
                                if receipt
                                    .get("dispatched")
                                    .and_then(serde_json::Value::as_bool)
                                    != Some(true)
                                    || receipt
                                        .get("request_digest")
                                        .and_then(serde_json::Value::as_str)
                                        != Some(request.request_digest.as_str())
                                {
                                    return Err("orphan original dispatch receipt mismatch".into());
                                }
                            }
                            crate::world_service::wire::SchedulerOperationV1::ResumeWake {
                                wake_id,
                                proposal,
                                resume,
                                ..
                            } => {
                                let result: crate::runtime::CognitionWakeHandoffResultV1 =
                                    serde_json::from_value(receipt).map_err(|e| e.to_string())?;
                                let next = result
                                    .replanned_continuation
                                    .as_ref()
                                    .ok_or("orphan ResumeWake next continuation missing")?;
                                if result.wake.wake_id != *wake_id
                                    || result.wake.agent_id != phase.agent
                                    || result.continuation.agent_id != phase.agent
                                    || result.continuation.continuation_id
                                        != result.wake.continuation_id
                                    || next.continuation_proposal_id
                                        != proposal.continuation_proposal_id
                                    || next.proposal_digest != proposal.proposal_digest
                                    || resume.agent_session_id != request.agent_session_id
                                    || resume.agent_turn_id != request.agent_turn_id
                                    || resume.decision_request_id != request.decision_request_id
                                    || resume.request_digest != request.request_digest.to_string()
                                {
                                    return Err(
                                        "orphan original ResumeWake receipt identity mismatch"
                                            .into(),
                                    );
                                }
                            }
                            _ => {
                                return Err("orphan original dispatch operation unsupported".into());
                            }
                        }
                    } else if receipt.get("failed").and_then(serde_json::Value::as_bool)
                        != Some(true)
                        || receipt
                            .get("request_digest")
                            .and_then(serde_json::Value::as_str)
                            != Some(request.request_digest.as_str())
                    {
                        return Err(
                            "orphan original terminal receipt identity mismatch; retained".into(),
                        );
                    }
                    phase.stage = if phase.stage == "dispatch" {
                        "failure"
                    } else {
                        "failure_view"
                    }
                    .into();
                }
                phase.commit = Some(*commit);
                phase.submit = false;
            }
            IntentOutcome::Unknown if phase.stage != "dispatch" => phase.submit = true,
            IntentOutcome::Rejected { .. } => {
                self.llm_sidecar
                    .provider_recovery_pending
                    .get_mut(&phase.agent)
                    .ok_or("orphan recovery missing")?
                    .reason = format!("{ORPHAN_REASON};canonical_rejected:{id}");
                self.llm_sidecar.persist_provider_lineage()?;
                return Err(
                    "orphan original canonical operation rejected; immutable original retained"
                        .into(),
                );
            }
            IntentOutcome::Pending | IntentOutcome::Received { .. } | IntentOutcome::Unknown => {}
            _ => return Err("orphan original outcome cannot establish canonical recovery".into()),
        }
        self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Orphan(phase));
        Ok(())
    }
}

#[cfg(test)]
impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn test_orphan_phase(
        &self,
        agent: &str,
        stage: &str,
        original_id: Option<String>,
    ) -> OrphanPhase {
        OrphanPhase {
            agent: agent.into(),
            stage: stage.into(),
            submit: false,
            commit: None,
            original_id,
        }
    }
    pub(in crate::viewer::runtime_live) fn test_original_orphan_dispatch(
        &self,
        agent: &str,
    ) -> Result<String, String> {
        self.original_orphan_dispatch(agent)
    }
    pub(in crate::viewer::runtime_live) fn test_orphan_settled_readback(
        &mut self,
        agent: &str,
        original: &str,
        receipt: serde_json::Value,
    ) -> Result<(), String> {
        let context = self.orphan_context(agent)?;
        Self::validate_hosted_settlement_receipt(
            &context.request_context,
            self.provider_cognition_leases
                .get(agent)
                .ok_or("orphan lease missing")?,
            receipt,
        )?;
        self.retire_canonical_orphan(agent, original)
    }
}
