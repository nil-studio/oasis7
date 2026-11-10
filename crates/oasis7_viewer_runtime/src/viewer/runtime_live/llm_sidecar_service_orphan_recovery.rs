//! Recover an original hosted dispatch without repeating an external model call.
use super::*;
use crate::viewer::runtime_live::{
    agent_service_io::{
        AgentServiceBlocker, AgentServiceIoOperation, AgentServiceIoResponse, AgentServiceProgress,
    },
    agent_service_phase::{HostedServicePhase, original_intent_io, original_response},
};
use oasis7_client_api::world_service::*;

const KNOWN_FAILURE_REASON: &str = "executor_failed_after_dispatch:known_provider_failure";
const ORPHAN_REASON: &str = "executor_lost_after_dispatch:model_outcome_unknown";

pub(super) fn same_provider_context(
    active: &cognition_context::ProviderContextState,
    candidate: &cognition_context::ProviderContextState,
) -> bool {
    serde_json::to_value(active).is_ok_and(|original| {
        serde_json::to_value(candidate).is_ok_and(|retained| original == retained)
    })
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(in crate::viewer::runtime_live) struct OrphanPhase {
    pub(in crate::viewer::runtime_live) agent: String,
    stage: String,
    pub(in crate::viewer::runtime_live) submit: bool,
    commit: Option<CommitRef>,
    original_id: Option<String>,
    terminal_authority: Option<Box<TerminalRecoveryAuthorityV1>>,
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
            let retained = self.provider_raw_outcomes.iter().any(|outcome| {
                outcome.agent_id == agent
                    && outcome
                        .prepared_request_context
                        .as_ref()
                        .is_none_or(|request| request == &active.request_context)
                    && outcome
                        .prepared_context
                        .as_ref()
                        .is_none_or(|context| context == &active.turn_context)
            }) || self
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
        if let Some((agent, _)) = self.provider_recovery_pending.iter().find(|(_, p)| {
            p.reason.starts_with(ORPHAN_REASON) || p.reason.starts_with(KNOWN_FAILURE_REASON)
        }) {
            self.hosted_service_phase = Some(HostedServicePhase::Orphan(OrphanPhase {
                agent: agent.clone(),
                stage: "view".into(),
                submit: false,
                commit: None,
                original_id: None,
                terminal_authority: None,
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
        {
            return Err("orphan current Runtime binding changed; model_outcome_unknown".into());
        }
        if let Some(authority) = &view.agent_context {
            if authority.agent_id != agent
                || invocation.subject != authority.capability_invocation_context.subject
                || invocation.grant_id != authority.capability_invocation_context.grant_id
            {
                return Err("orphan current Agent authority changed".into());
            }
        } else {
            let Some(HostedServicePhase::Orphan(phase)) = &self.hosted_service_phase else {
                return Err("orphan current Agent authority missing".into());
            };
            let proof = phase
                .terminal_authority
                .as_ref()
                .ok_or("orphan terminal authority missing")?;
            self.validate_orphan_terminal_identity(agent, proof)?;
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

    fn validate_orphan_terminal_identity(
        &self,
        agent: &str,
        proof: &TerminalRecoveryAuthorityV1,
    ) -> Result<(), String> {
        let context = self.orphan_context(agent)?;
        let request = &context.request_context;
        let view = self
            .provider_service_projection
            .as_ref()
            .ok_or("orphan current View missing")?;
        let binding = view
            .runtime_binding
            .as_ref()
            .ok_or("orphan current Runtime missing")?;
        let original_id = self.original_orphan_dispatch(agent)?;
        let pending = self
            .provider_scheduler_pending
            .get(&original_id)
            .ok_or("orphan original missing")?;
        proof
            .current_runtime_binding
            .validate()
            .map_err(|e| format!("{e:?}"))?;
        proof
            .validate(&pending.correlation)
            .map_err(|e| e.to_string())?;
        let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) = &pending.payload
        else {
            return Err("terminal recovery requires original signed Prefix".into());
        };
        let crate::world_service::wire::SchedulerOperationV1::ProviderPrefix {
            request: original,
            ..
        } = &signed.request.operation
        else {
            return Err("terminal recovery supports only original ProviderPrefix".into());
        };
        let invocation = request
            .base_decision_request
            .capability_invocation_context
            .as_ref()
            .ok_or("orphan invocation missing")?;
        let subject = oasis7_wasm_abi::CapabilitySubject::Agent {
            agent_id: agent.into(),
            owner_binding: proof.owner_binding.clone(),
            generation: proof.identity_generation,
        };
        let lease = self
            .provider_cognition_leases
            .get(agent)
            .ok_or("orphan original lease missing")?;
        lineage_generation_recovery::validate_provider_lease_identity(agent, request, lease)?;
        let expected_lease = TerminalLeaseIdentityV1 {
            lease_id: lease.lease_id.clone(),
            idempotency_key: lease.idempotency_key.clone(),
            account_id: lease.account_id.clone(),
            quote_digest: lease.quote.quote_digest.clone(),
            reserved_amount: lease.reserved_amount,
            reserved_at_tick: lease.reserved_at_tick,
        };
        let current = &proof.current_runtime_binding;
        if original != &crate::runtime_host::feedback::request_context_to_api(request)
            || proof.agent_id != agent
            || invocation.subject != subject
            || proof.grant_id != invocation.grant_id
            || proof.agent_session_id != request.agent_session_id
            || proof.agent_turn_id != request.agent_turn_id
            || proof.decision_request_id != request.decision_request_id
            || proof.request_digest != request.request_digest.to_string()
            || proof.retry_seq != request.retry_seq
            || proof.transport_attempt != request.transport_attempt
            || proof.writer_public_key != signed.subject_public_key
            || proof.delegation_generation != signed.request.delegation_generation
            || proof.lease != expected_lease
            || proof.prefix_deadline_unix_ms.is_some()
            || current.world_id != binding.world_id
            || current.branch_id != binding.branch_id
            || current.reorg_epoch != binding.reorg_epoch
            || current.finality_epoch != binding.finality_epoch
            || current.finality_block_hash != binding.finality_block_hash
            || current.finality_status != binding.finality_status
            || current.runtime_manifest_hash != binding.runtime_manifest_hash
            || proof.current_task_fence
                != view
                    .game_task_control
                    .as_ref()
                    .map(oasis7_agent_api::game::GameTaskFenceV1::from_control)
        {
            return Err(
                "orphan terminal assertion differs from original or current authority".into(),
            );
        }
        if let Some(owner) = &view.canonical_agent_owner
            && (owner.agent_id != agent
                || owner.player_id != proof.owner_binding
                || owner.canonical_authority.agent_identity_generation != proof.identity_generation
                || owner.world_id != binding.world_id
                || owner.reorg_epoch != binding.reorg_epoch
                || owner.canonical_authority.branch_id != binding.branch_id)
        {
            return Err("orphan current owner or identity generation changed".into());
        }
        Ok(())
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
        if phase.stage != "dispatch" {
            self.validate_orphan_authority(&phase.agent)?;
        }
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
        if phase.stage == "dispatch"
            && self
                .provider_service_projection
                .as_ref()
                .is_some_and(|view| view.agent_context.is_none())
        {
            let minimum = phase
                .commit
                .clone()
                .ok_or("orphan authenticated View watermark missing")?;
            return Ok(AgentServiceIoOperation::TerminalLookup {
                request: LookupIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    key: pending.correlation.key.clone(),
                },
                original: pending.payload,
                minimum,
            });
        }
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
    ) -> Result<AgentServiceProgress, String> {
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
            if phase.stage != "view" {
                self.llm_sidecar.validate_orphan_authority(&phase.agent)?;
            }
            match phase.stage.as_str() {
                "view" => {
                    phase.original_id =
                        Some(self.llm_sidecar.original_orphan_dispatch(&phase.agent)?);
                    phase.commit = Some(
                        self.verified_world_view
                            .as_ref()
                            .ok_or("orphan verified View missing")?
                            .version()
                            .commit
                            .clone(),
                    );
                    phase.stage = "dispatch".into();
                }
                "failure_view" => phase.stage = "settle".into(),
                "settled_view" => {
                    let known_failure = self
                        .llm_sidecar
                        .provider_recovery_pending
                        .get(&phase.agent)
                        .is_some_and(|pending| pending.reason.starts_with(KNOWN_FAILURE_REASON));
                    if known_failure {
                        // The authenticated settled view above still precedes any
                        // actor release. First capture its exact failed feedback;
                        // a later loop under the publication ACK consumes it.
                        let lease = self.llm_sidecar.validate_orphan_authority(&phase.agent)?;
                        if lease.status != crate::runtime::CognitionLeaseStatusV1::Settled
                            || lease.settled_amount != 1
                            || lease.net_amount != 1
                            || lease.released_amount != 0
                            || lease.refunded_amount != 0
                            || lease.compensated_amount != 0
                        {
                            return Err(
                                "known provider failure lacks canonical settled readback".into()
                            );
                        }
                        self.llm_sidecar
                            .fail_provider_turn_with_feedback_after_publication(&phase.agent)?;
                        if self
                            .llm_sidecar
                            .provider_active_turns
                            .contains_key(&phase.agent)
                        {
                            self.llm_sidecar.hosted_service_phase =
                                Some(HostedServicePhase::Orphan(phase));
                            return Ok(AgentServiceProgress::Advanced);
                        }
                    } else {
                        self.llm_sidecar.retire_canonical_orphan(
                            &phase.agent,
                            phase
                                .original_id
                                .as_deref()
                                .ok_or("orphan original ID missing")?,
                        )?;
                    }
                    self.llm_sidecar.hosted_service_phase = None;
                    self.llm_sidecar.hosted_service_config_binding = None;
                    return Ok(AgentServiceProgress::Advanced);
                }
                _ => return Err("unknown orphan view stage".into()),
            }
            phase.submit = false;
            self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Orphan(phase));
            return Ok(AgentServiceProgress::Advanced);
        }
        let Some(response) = original_response(response)? else {
            return Ok(AgentServiceProgress::Idle);
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
        if phase.stage == "dispatch"
            && (matches!(&response.outcome, IntentOutcome::Committed { .. })
                || (matches!(&response.outcome, IntentOutcome::HistoryUnavailable)
                    && response
                        .terminal_recovery_authority
                        .as_ref()
                        .is_some_and(|proof| proof.schema_version == 2)))
        {
            if let Some(proof) = &response.terminal_recovery_authority {
                let minimum = phase
                    .commit
                    .as_ref()
                    .ok_or("orphan View watermark missing")?;
                if !proof
                    .read_commit
                    .satisfies_minimum(minimum)
                    .map_err(|e| e.to_string())?
                {
                    return Err("orphan terminal proof precedes current View".into());
                }
                let current_binding = self
                    .llm_sidecar
                    .provider_service_projection
                    .as_ref()
                    .and_then(|view| view.runtime_binding.as_ref())
                    .ok_or("orphan current binding missing")?;
                if (proof.read_commit == *minimum
                    && proof.current_runtime_binding != *current_binding)
                    || proof.current_runtime_binding.base_tick < current_binding.base_tick
                {
                    return Err("orphan terminal proof Runtime precedes authenticated View".into());
                }
                self.llm_sidecar
                    .validate_orphan_terminal_identity(&phase.agent, proof)?;
                phase.terminal_authority = Some(proof.clone());
            }
            // Install only after all comparisons; absence retains the full live path.
            self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Orphan(phase.clone()));
            self.llm_sidecar.validate_orphan_authority(&phase.agent)?;
        }
        if phase.stage == "dispatch"
            && let Some(proof) = response
                .terminal_recovery_authority
                .as_ref()
                .filter(|proof| proof.schema_version == 2)
        {
            // Current Journal authority closes only this original dispatch. It
            // does not manufacture an old Committed receipt or historical cut.
            phase.stage = "failure".into();
            phase.commit = Some(proof.read_commit.clone());
            phase.submit = false;
            self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Orphan(phase));
            return Ok(AgentServiceProgress::Advanced);
        }
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
                    .reason
                    .push_str(&format!(";canonical_rejected:{id}"));
                self.llm_sidecar.persist_provider_lineage()?;
                return Err(
                    "orphan original canonical operation rejected; immutable original retained"
                        .into(),
                );
            }
            IntentOutcome::HistoryUnavailable | IntentOutcome::Expired => {
                let blocker = if matches!(response.outcome, IntentOutcome::HistoryUnavailable) {
                    AgentServiceBlocker::HistoricalEvidenceUnavailable
                } else {
                    AgentServiceBlocker::OriginalIntentExpired
                };
                // These are authenticated outcomes, never evidence of absence.
                // Retain the original Lookup; neither permits a new Submit.
                phase.submit = false;
                self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Orphan(phase));
                return Ok(AgentServiceProgress::Blocked(blocker));
            }
            IntentOutcome::Pending | IntentOutcome::Received { .. } | IntentOutcome::Unknown => {
                self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Orphan(phase));
                return Ok(AgentServiceProgress::Idle);
            }
        }
        self.llm_sidecar.hosted_service_phase = Some(HostedServicePhase::Orphan(phase));
        Ok(AgentServiceProgress::Advanced)
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
            terminal_authority: None,
        }
    }
    pub(in crate::viewer::runtime_live) fn test_orphan_lookup_at(
        &self,
        agent: &str,
        original_id: String,
        minimum: CommitRef,
    ) -> OrphanPhase {
        let mut phase = self.test_orphan_phase(agent, "dispatch", Some(original_id));
        phase.commit = Some(minimum);
        phase
    }
    pub(in crate::viewer::runtime_live) fn test_validate_orphan_terminal_identity(
        &self,
        agent: &str,
        proof: &TerminalRecoveryAuthorityV1,
    ) -> Result<(), String> {
        self.validate_orphan_terminal_identity(agent, proof)
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
