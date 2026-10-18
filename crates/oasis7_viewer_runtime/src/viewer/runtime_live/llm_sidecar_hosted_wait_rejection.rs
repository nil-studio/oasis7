//! Authenticated first-Admit rejection and fixed-unit compensation.
use super::*;

impl HostedWait {
    pub(super) fn validate_rejection(
        &self,
        pending: Option<&lineage_persistence::PendingProviderSchedulerIntent>,
    ) -> Result<(), String> {
        if let Some(response) = &self.rejection {
            let pending = pending.ok_or("rejected Wait original checkpoint missing")?;
            response
                .validate(&pending.correlation)
                .map_err(|error| error.to_string())?;
            if !matches!(response.outcome, IntentOutcome::Rejected { .. })
                || self.admitted.is_some()
            {
                return Err("rejected Wait outcome or admission conflict".into());
            }
        }
        Ok(())
    }
    pub(super) fn accept_rejection(
        &mut self,
        response: IntentResponse<serde_json::Value>,
        pending: &lineage_persistence::PendingProviderSchedulerIntent,
    ) -> Result<(), String> {
        response
            .validate(&pending.correlation)
            .map_err(|error| error.to_string())?;
        if !matches!(response.outcome, IntentOutcome::Rejected { .. }) || self.admitted.is_some() {
            return Err("Wait rejection requires never-admitted original".into());
        }
        if self
            .rejection
            .as_ref()
            .is_some_and(|original| original != &response)
        {
            return Err("Wait original rejection changed; fenced".into());
        }
        self.rejection = Some(response);
        Ok(())
    }
}

impl crate::viewer::ViewerRuntimeLiveServer {
    pub(super) fn prepare_rejected_wait_operation(
        &mut self,
        wait: &HostedWait,
        client: &crate::world_service::client::RemoteWorldServiceClient,
    ) -> Result<Option<AgentServiceIoOperation>, String> {
        if !wait.stage.starts_with("rejection_") && !wait.stage.starts_with("compensate_") {
            return Ok(None);
        }
        let cognition = wait
            .decision
            .cognition
            .as_ref()
            .ok_or("rejected Wait cognition missing")?;
        let request = &cognition.request.request_context;
        let original = self
            .llm_sidecar
            .provider_scheduler_pending
            .get(&format!("{}:admit_wait", request.provider_invocation_key()))
            .ok_or("rejected Wait original checkpoint missing")?;
        wait.validate_rejection(Some(original))?;
        if wait.rejection.is_none() {
            return Err("Wait compensation requires authenticated rejection".into());
        }
        if wait.stage.ends_with("_view") {
            return Ok(Some(AgentServiceIoOperation::View(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: client.config().expected_world.clone(),
                scope_id: client.config().scope_id.clone(),
                min_commit: wait.commit.clone(),
                fixed_commit: None,
                deadline_unix_ms: None,
            })));
        }
        if wait.stage != "compensate_settle" {
            return Err("Wait compensation stage invalid".into());
        }
        let lease = cognition
            .cognition_lease
            .as_ref()
            .ok_or("rejected Wait lease missing")?;
        lineage_generation_recovery::validate_provider_lease_identity(
            &request.agent_subject,
            request,
            lease,
        )?;
        let (checkpoint, existed) = self.llm_sidecar.prepare_service_scheduler_checkpoint(
            request,
            "compensate_wait_settle",
            SchedulerOperationV1::SettleLease {
                lease_id: lease.lease_id.clone(),
                consumed_amount: lease.reserved_amount,
            },
            None,
        )?;
        Ok(Some(
            crate::viewer::runtime_live::agent_service_phase::original_intent_io(
                &checkpoint.correlation,
                &checkpoint.payload,
                !existed,
            ),
        ))
    }

    pub(super) fn apply_rejected_wait_response(
        &mut self,
        wait: &mut HostedWait,
        response: AgentServiceIoResponse,
    ) -> Result<(), String> {
        let cognition = wait
            .decision
            .cognition
            .as_ref()
            .ok_or("rejected Wait cognition missing")?;
        let request = &cognition.request.request_context;
        let lease = cognition
            .cognition_lease
            .as_ref()
            .ok_or("rejected Wait lease missing")?;
        if wait.stage.ends_with("_view") {
            let AgentServiceIoResponse::View(view) = response else {
                return Err("Wait compensation authorized view missing".into());
            };
            if let Some(commit) = wait.commit.as_ref() {
                if !view
                    .version()
                    .commit
                    .satisfies_minimum(commit)
                    .map_err(|error| error.to_string())?
                {
                    return Err("Wait compensation view precedes settlement".into());
                }
            } else if wait.stage != "rejection_view" {
                return Err("Wait compensation settlement commit missing".into());
            }
            self.apply_hosted_verified_view(*view)?;
            let projection = self
                .llm_sidecar
                .provider_service_projection
                .as_ref()
                .ok_or("Wait compensation canonical projection missing")?;
            wait.validate_rejected_projection(projection, false)?;
            if wait.stage == "rejection_view" {
                wait.stage = "compensate_settle".into();
            } else {
                wait.validate_rejected_projection(projection, true)?;
                wait.stage = "rejected_finalize".into();
            }
            wait.commit = None;
            return Ok(());
        }
        let Some(response) =
            crate::viewer::runtime_live::agent_service_phase::original_response(response)?
        else {
            return Ok(());
        };
        let pending = self
            .llm_sidecar
            .provider_scheduler_pending
            .get(&format!(
                "{}:compensate_wait_settle",
                request.provider_invocation_key()
            ))
            .ok_or("Wait compensation original checkpoint missing")?;
        response
            .validate(&pending.correlation)
            .map_err(|error| error.to_string())?;
        match response.outcome {
            IntentOutcome::Committed { commit, receipt } => {
                RuntimeLlmSidecar::validate_hosted_settlement_receipt(request, lease, receipt)?;
                wait.stage = "compensate_settle_view".into();
                wait.commit = Some(*commit);
                Ok(())
            }
            IntentOutcome::Received { .. } | IntentOutcome::Pending | IntentOutcome::Unknown => {
                Ok(())
            }
            IntentOutcome::Rejected { .. } => {
                Err("Wait compensation rejected; original state retained".into())
            }
            _ => Err("Wait compensation unsupported outcome; fenced".into()),
        }
    }
}

impl HostedWait {
    fn validate_rejected_projection(
        &self,
        projection: &crate::world_service::projection::WorldServiceProjection,
        settled: bool,
    ) -> Result<(), String> {
        let cognition = self
            .decision
            .cognition
            .as_ref()
            .ok_or("rejected Wait cognition missing")?;
        let request = &cognition.request.request_context;
        let lease = cognition
            .cognition_lease
            .as_ref()
            .ok_or("rejected Wait lease missing")?;
        lineage_generation_recovery::validate_provider_lease_identity(
            &request.agent_subject,
            request,
            lease,
        )?;
        let binding = projection
            .runtime_binding
            .as_ref()
            .ok_or("Wait compensation current binding missing")?;
        let captured = &request.runtime_binding;
        let agent = projection
            .agent_context
            .as_ref()
            .ok_or("Wait compensation authorized Agent context missing")?;
        let old_catalog = request
            .base_decision_request
            .capability_catalog
            .as_ref()
            .ok_or("Wait compensation original capability catalog missing")?;
        let catalog = &agent.capability_catalog;
        if binding.world_id != captured.world_id
            || binding.branch_id != captured.branch_id
            || binding.reorg_epoch != captured.reorg_epoch
            || binding.finality_epoch != captured.finality_epoch
            || binding.runtime_manifest_hash != captured.runtime_manifest_hash
            || agent.agent_id != request.agent_subject
            || catalog.revocation_epoch != old_catalog.revocation_epoch
            || catalog.policy_hash != old_catalog.policy_hash
            || catalog.module_registry_hash != old_catalog.module_registry_hash
            || catalog.subject != old_catalog.subject
            || catalog.presenter != old_catalog.presenter
            || catalog.audience != old_catalog.audience
        {
            return Err("Wait compensation authority or branch changed; blocked".into());
        }
        let actual = projection
            .cognition_leases
            .iter()
            .find(|entry| entry.lease_id == lease.lease_id)
            .ok_or("Wait compensation canonical lease missing")?;
        if actual.idempotency_key != lease.idempotency_key
            || actual.account_id != lease.account_id
            || actual.agent_id != lease.agent_id
            || actual.agent_session_id != lease.agent_session_id
            || actual.agent_turn_id != lease.agent_turn_id
            || actual.decision_request_id != lease.decision_request_id
            || actual.request_digest != lease.request_digest
            || actual.quote != lease.quote
            || actual.reserved_amount != lease.reserved_amount
            || (settled
                && (actual.status != crate::runtime::CognitionLeaseStatusV1::Settled
                    || actual.settled_amount != lease.reserved_amount))
            || (!settled
                && !matches!(
                    actual.status,
                    crate::runtime::CognitionLeaseStatusV1::Reserved
                        | crate::runtime::CognitionLeaseStatusV1::Settled
                ))
        {
            return Err("Wait compensation canonical lease identity or charge mismatch".into());
        }
        if projection.continuations.iter().any(|entry| {
            entry.agent_id == request.agent_subject
                && entry.origin_request_digest == request.request_digest.to_string()
                && entry.continuation_proposal_id == self.proposal.continuation_proposal_id
        }) {
            return Err("rejected Wait unexpectedly admitted; fenced".into());
        }
        Ok(())
    }
}

impl RuntimeLlmSidecar {
    pub(super) fn complete_rejected_hosted_wait(&mut self) -> Result<(), String> {
        let wait = self
            .hosted_wait
            .clone()
            .ok_or("rejected Wait phase missing")?;
        let cognition = wait
            .decision
            .cognition
            .as_ref()
            .ok_or("rejected Wait cognition missing")?;
        let request = &cognition.request.request_context;
        let original = self
            .provider_scheduler_pending
            .get(&format!("{}:admit_wait", request.provider_invocation_key()))
            .ok_or("rejected Wait original checkpoint missing")?;
        wait.validate_rejection(Some(original))?;
        let Some(IntentResponse {
            formal_profile: None,
            outcome: IntentOutcome::Rejected { .. },
            ..
        }) = wait.rejection.as_ref()
        else {
            return Err("Wait rejected cleanup lacks original rejection".into());
        };
        if wait.stage != "rejected_finalize" {
            return Err("Wait rejected cleanup precedes settlement".into());
        }
        let projection = self
            .provider_service_projection
            .as_ref()
            .ok_or("rejected Wait verified view missing")?;
        wait.validate_rejected_projection(projection, true)?;
        for context in [
            self.provider_active_turns.get(&request.agent_subject),
            self.provider_contexts.get(&request.agent_subject),
        ]
        .into_iter()
        .flatten()
        {
            if context.request_context != *request {
                return Err("rejected Wait cleanup newer context conflict".into());
            }
        }
        if self
            .provider_completed_decisions
            .front()
            .is_none_or(|decision| {
                decision
                    .cognition
                    .as_ref()
                    .is_none_or(|cognition| cognition.request.request_context != *request)
            })
            || self
                .provider_held_decisions
                .get(&request.agent_subject)
                .is_some_and(|decision| {
                    decision
                        .cognition
                        .as_ref()
                        .is_none_or(|cognition| cognition.request.request_context != *request)
                })
        {
            return Err("rejected Wait cleanup completed identity mismatch".into());
        }
        let backup = (
            self.provider_cognition_leases.clone(),
            self.provider_completed_decisions.clone(),
            self.provider_active_turns.clone(),
            self.provider_contexts.clone(),
            self.provider_held_decisions.clone(),
            self.provider_continuation_proposals.clone(),
            self.hosted_restored_wait.clone(),
            self.hosted_restored_wait_decisions.clone(),
            self.provider_terminal_states.clone(),
        );
        #[cfg(any(test, feature = "test_tier_required"))]
        let test_persist = self
            .test_wait_rejection_root()?
            .map(|root| {
                let native = self
                    .runner
                    .as_ref()
                    .and_then(RuntimeDecisionRunner::async_runner)
                    .ok_or("test rejected Wait native runner missing")?;
                self.test_wait_rejection_state(native.rejected_wait_test_ledger_digests())
                    .map(|before| (root, before))
            })
            .transpose()?;
        #[cfg(any(test, feature = "test_tier_required"))]
        let mut test_persist_attempted = false;
        let mut runner = self.runner.take().ok_or("rejected Wait runner missing")?;
        let result = (|| {
            let native = runner
                .async_runner_mut()
                .ok_or("rejected Wait native runner missing")?;
            let restored = self.hosted_restored_wait.is_some();
            let mut persist = |_digests: Option<[String; 5]>| {
                self.stage_rejected_wait_cleanup(
                    &wait,
                    "canonical_wait_admission_rejected".into(),
                )?;
                #[cfg(any(test, feature = "test_tier_required"))]
                if let Some((root, before)) = &test_persist {
                    let staged = self.test_wait_rejection_state(
                        _digests.ok_or("test rejected Wait staged ledgers missing")?,
                    )?;
                    Self::test_write_wait_cleanup_file(
                        root,
                        "wait-rejection-before.json",
                        &serde_json::to_vec(before)
                            .map_err(|_| "test rejected Wait snapshot failed")?,
                    )?;
                    Self::test_write_wait_cleanup_file(
                        root,
                        "wait-rejection-staged.json",
                        &serde_json::to_vec(&staged)
                            .map_err(|_| "test rejected Wait snapshot failed")?,
                    )?;
                    Self::test_write_wait_cleanup_file(
                        root,
                        "wait-rejection-before-persist",
                        b"ready",
                    )?;
                    Self::test_wait_wait_cleanup_file(root, "wait-rejection-release")?;
                    test_persist_attempted = true;
                }
                self.persist_provider_lineage()
            };
            if restored {
                if !native
                    .rejected_unprojected_wait_is_absent(
                        &request.agent_subject,
                        &wait.proposal,
                        request,
                    )
                    .map_err(|error| error.to_string())?
                {
                    return Err("restored rejected Wait local state is not absent; fenced".into());
                }
                #[cfg(any(test, feature = "test_tier_required"))]
                let digests = Some(native.rejected_wait_test_ledger_digests());
                #[cfg(not(any(test, feature = "test_tier_required")))]
                let digests = None;
                persist(digests)
            } else {
                #[cfg(any(test, feature = "test_tier_required"))]
                if test_persist.is_some() {
                    return native
                        .with_rejected_unprojected_wait_cleanup_observed(
                            &request.agent_subject,
                            &wait.proposal,
                            &wait.current,
                            request,
                            |_, digests| persist(Some(digests)),
                        )
                        .map(|_| ())
                        .map_err(|error| error.to_string());
                }
                native
                    .with_rejected_unprojected_wait_cleanup(
                        &request.agent_subject,
                        &wait.proposal,
                        &wait.current,
                        request,
                        |_| persist(None),
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
        })();
        self.runner = Some(runner);
        if let Err(error) = result {
            (
                self.provider_cognition_leases,
                self.provider_completed_decisions,
                self.provider_active_turns,
                self.provider_contexts,
                self.provider_held_decisions,
                self.provider_continuation_proposals,
                self.hosted_restored_wait,
                self.hosted_restored_wait_decisions,
                self.provider_terminal_states,
            ) = backup;
            self.hosted_wait = Some(wait);
            #[cfg(any(test, feature = "test_tier_required"))]
            if test_persist_attempted && let Some((root, _)) = &test_persist {
                let recorded = self
                    .runner
                    .as_ref()
                    .and_then(RuntimeDecisionRunner::async_runner)
                    .ok_or_else(|| "test rejected Wait rollback runner missing".to_string())
                    .and_then(|native| {
                        self.test_wait_rejection_state(native.rejected_wait_test_ledger_digests())
                    })
                    .and_then(|after| {
                        Self::test_write_wait_cleanup_file(
                            root,
                            "wait-rejection-after.json",
                            &serde_json::to_vec(&after).map_err(|_| {
                                "test rejected Wait rollback snapshot failed".to_string()
                            })?,
                        )?;
                        Self::test_write_wait_cleanup_file(
                            root,
                            "wait-rejection-rollback",
                            b"ready",
                        )
                    });
                if recorded.is_ok() {
                    eprintln!("hosted_wait_rejection_persistence_failed");
                    let _ = Self::test_wait_wait_cleanup_file(root, "wait-rejection-retry-release");
                }
            }
            return Err(error);
        }
        Ok(())
    }

    fn stage_rejected_wait_cleanup(
        &mut self,
        wait: &HostedWait,
        reason: String,
    ) -> Result<(), String> {
        let cognition = wait
            .decision
            .cognition
            .as_ref()
            .ok_or("rejected Wait cognition missing")?;
        let request = &cognition.request.request_context;
        self.clear_settled_service_lease(
            request,
            cognition
                .cognition_lease
                .as_ref()
                .ok_or("rejected Wait lease missing")?,
        )?;
        self.provider_completed_decisions.pop_front();
        self.provider_active_turns.remove(&request.agent_subject);
        self.provider_contexts.remove(&request.agent_subject);
        self.provider_held_decisions.remove(&request.agent_subject);
        self.provider_continuation_proposals
            .remove(&wait.proposal.continuation_proposal_id);
        self.hosted_wait = None;
        self.hosted_restored_wait = None;
        self.hosted_restored_wait_decisions
            .remove(&request.provider_invocation_key().to_string());
        self.provider_terminal_states.insert(
            request.agent_subject.clone(),
            lineage_persistence::ProviderTerminalState {
                agent_id: request.agent_subject.clone(),
                agent_session_id: request.agent_session_id.clone(),
                agent_turn_id: request.agent_turn_id.clone(),
                decision_request_id: request.decision_request_id.clone(),
                request_digest: request.request_digest.to_string(),
                status: "rejected".into(),
                reject_reason: Some(reason),
                feedback_id: None,
                feedback: None,
            },
        );
        Ok(())
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
impl RuntimeLlmSidecar {
    fn test_wait_rejection_root(&self) -> Result<Option<std::path::PathBuf>, String> {
        let Some(root) = std::env::var_os("PRE2_WAIT_REJECTION_FS_ROOT") else {
            return Ok(None);
        };
        let root = std::path::PathBuf::from(root);
        if !root.is_absolute() || !root.is_dir() {
            return Err("test rejected Wait private directory required".into());
        }
        if root.join("wait-rejection-before-persist").exists() {
            return Ok(None);
        }
        Ok(Some(root))
    }
    fn test_wait_rejection_state(&self, digests: [String; 5]) -> Result<serde_json::Value, String> {
        let mut state = self.test_wait_cleanup_state()?;
        let fields = state
            .as_object_mut()
            .ok_or("test rejected Wait snapshot invalid")?;
        fields.insert(
            "terminal_states".into(),
            serde_json::to_value(&self.provider_terminal_states)
                .map_err(|_| "test rejected Wait terminal snapshot failed")?,
        );
        fields.insert(
            "runtime_ledgers".into(),
            serde_json::to_value(digests)
                .map_err(|_| "test rejected Wait ledger snapshot failed")?,
        );
        Ok(state)
    }
}
