use super::*;
use oasis7_agent_api::game::*;
use ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH;
use ws::private_goal::verify_owner_signature_strict;
impl AccountProvisionApprovalState {
    pub fn owner_setup_result_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        result: &OwnerSetupResultV1,
        session: &ParticipantSessionEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.owner_setup_stored_admission_checks(id, stage, current, now)?;
        let o = self.owner_setup_dispatched(id, stage)?;
        if o.callback_claimed_at.is_none() {
            return Err(invalid());
        }
        self.validate_owner_setup_result(id, stage, result)?;
        if let Some(fact) = result.canonical_fact() {
            if o.canonical_history.get(&fact.source) != Some(&fact) {
                return Err(invalid());
            }
        }
        let participant = self
            .participant_provisions
            .values()
            .find(|v| v.result.is_some())
            .ok_or_else(invalid)?;
        let d = &participant.descriptor;
        let setup_session = self
            .owner_setups
            .get(id)
            .and_then(|v| v.session.as_ref())
            .ok_or_else(invalid)?;
        let nonce = match result {
            OwnerSetupResultV1::Canonical {
                from_lookup: true, ..
            } => {
                if o.lookup_claimed_at.is_none() {
                    return Err(invalid());
                }
                ledger::lookup_nonce(id, stage, &o.session_nonce)?
            }
            _ => o.session_nonce.clone(),
        };
        session
            .status
            .status
            .validate_for(
                &d.native_binding,
                &nonce,
                setup_session.native_binding_generation,
                current.provision.approval.authority.service_epoch,
                now,
            )
            .map_err(|_| invalid())?;
        if !session.status.status.active
            || session.status.status.frozen
            || session.status.status.pending
            || session.private_sid_generation != setup_session.private_sid_generation
            || session.status.status.session_ref != setup_session.session_ref
        {
            return Err(invalid());
        }
        let checks = append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&(id, stage))?,
        )?;
        Ok(append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::Result(result, &d.world.service_public_key),
        )?)
    }
    pub fn complete_owner_setup_stage(
        &mut self,
        id: &str,
        completion: OwnerSetupCompletionV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupOutboxV1> {
        let OwnerSetupCompletionV1 {
            stage,
            result,
            session,
        } = completion;
        if let Some(old) = self
            .owner_setup_outbox(id, stage)
            .filter(|o| o.result.is_some())
        {
            return if old.result.as_ref() == Some(&result) && old.session.as_ref() == Some(&session)
            {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if let Some(old) = self
            .owner_setup_outbox(id, stage)
            .filter(|o| o.last_observation.is_some())
        {
            return if old.last_observation.as_ref() == Some(&result)
                && old.last_observation_session.as_ref() == Some(&session)
            {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        verifier
            .verify(&self.owner_setup_result_checks(id, stage, &result, &session, current, now)?)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if result.terminal() {
            o.operation
                .confirm(digest(&result)?)
                .map_err(|_| invalid())?;
            o.result = Some(result);
            o.session = Some(session);
            o.completed_at = Some(DecimalU64(now));
        } else {
            o.operation.mark_unknown().map_err(|_| invalid())?;
            o.last_observation = Some(result);
            o.last_observation_session = Some(session);
            o.last_observed_at = Some(DecimalU64(now));
        }
        let result = o.clone();
        if stage == OwnerSetupStageV1::FinalObservation
            && result.result.as_ref().is_some_and(|r| r.succeeded())
        {
            let phase = PhaseOriginal {
                operation_id: id.into(),
                body_digest: digest(&next.owner_setups.get(id).ok_or_else(invalid)?.plan)?,
                receipt_digest: Some(digest(result.result.as_ref().ok_or_else(invalid)?)?),
            };
            next.provision.confirm_owner_setup(phase)?;
        }
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub(super) fn validate_owner_setup_result(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        result: &OwnerSetupResultV1,
    ) -> Result<()> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let o = v.stages.get(&stage).ok_or_else(invalid)?;
        let key = &v.plan.world_identity.service_public_key;
        match (o.original.as_ref().ok_or_else(invalid)?, result) {
            (
                OwnerSetupOriginalV1::Canonical { submit, lookup },
                OwnerSetupResultV1::Canonical {
                    response,
                    from_lookup,
                },
            ) => {
                let domain = if *from_lookup {
                    ws::LOOKUP_PATH
                } else {
                    ws::SUBMIT_PATH
                };
                let expected = if *from_lookup {
                    ws::request_digest(domain, lookup)
                } else {
                    ws::request_digest(domain, submit)
                }
                .map_err(|_| invalid())?;
                if response.request_digest != expected {
                    return Err(invalid());
                }
                verify_owner_signature_strict(
                    domain,
                    &(&response.request_digest, &response.payload),
                    key,
                    &response.signature_hex,
                )
                .map_err(|_| invalid())?;
                response
                    .payload
                    .validate(&submit.correlation)
                    .map_err(|_| invalid())?;
                let ws::IntentOutcome::Committed { commit, receipt } = &response.payload.outcome
                else {
                    return Ok(());
                };
                if commit.binding.branch_id != v.plan.world_identity.branch_id
                    || commit.binding.reorg_generation != v.plan.world_identity.reorg_epoch.0
                {
                    return Err(invalid());
                }
                let legacy: ws::SubmitIntentRequest<
                    oasis7_client_api::legacy_world_service::WorldServicePayloadV1,
                > = serde_json::from_value(serde_json::to_value(submit).map_err(|_| invalid())?)
                    .map_err(|_| invalid())?;
                let expected = match &submit.signed_payload {
                    WorldServicePayloadV1::GameTaskCreate(_) => serde_json::to_value(
                        oasis7_client_api::legacy_task_results::create_control(&legacy)
                            .map_err(|_| invalid())?,
                    ),
                    WorldServicePayloadV1::GameGoalPublish(_) => serde_json::to_value(
                        oasis7_client_api::legacy_task_results::publish_receipt(
                            &legacy,
                            commit.position,
                        )
                        .map_err(|_| invalid())?,
                    ),
                    WorldServicePayloadV1::GameExecutorControl(_) => serde_json::to_value(
                        oasis7_client_api::legacy_task_results::executor_receipt(&legacy)
                            .map_err(|_| invalid())?,
                    ),
                    WorldServicePayloadV1::Delegation(s) => {
                        Ok(serde_json::json!({"delegation":s.request}))
                    }
                    _ => return Err(invalid()),
                }
                .map_err(|_| invalid())?;
                if receipt != &expected {
                    return Err(invalid());
                }
            }
            (
                OwnerSetupOriginalV1::PrepareGoal(original),
                OwnerSetupResultV1::PreparedGoal(response),
            ) => {
                ws::private_goal_read::verify_prepare_response(original, response, key)
                    .map_err(|_| invalid())?;
                verify_owner_signature_strict(
                    ws::private_goal_read::GOAL_PREPARE_PATH,
                    &(&response.request_digest, &response.payload),
                    key,
                    &response.signature_hex,
                )
                .map_err(|_| invalid())?;
            }
            (
                OwnerSetupOriginalV1::FinalObservation {
                    owner: request,
                    goal: goal_request,
                },
                OwnerSetupResultV1::FinalObservation { owner, goal },
            ) => {
                owner.payload.validate(request).map_err(|_| invalid())?;
                if o.world.as_ref().map(|w| &w.response) != Some(owner.as_ref()) {
                    return Err(invalid());
                }
                if owner.request_digest
                    != ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, request)
                        .map_err(|_| invalid())?
                {
                    return Err(invalid());
                }
                verify_owner_signature_strict(
                    PARTICIPANT_OWNER_VIEW_PATH,
                    &(&owner.request_digest, &owner.payload),
                    key,
                    &owner.signature_hex,
                )
                .map_err(|_| invalid())?;
                ws::private_goal_read::verify_current_response(goal_request, goal, key)
                    .map_err(|_| invalid())?;
                verify_owner_signature_strict(
                    ws::private_goal_read::GOAL_CURRENT_PATH,
                    &(&goal.request_digest, &goal.payload),
                    key,
                    &goal.signature_hex,
                )
                .map_err(|_| invalid())?;
                let task = owner
                    .payload
                    .control
                    .view
                    .game_task_control
                    .as_ref()
                    .ok_or_else(invalid)?;
                if task != &goal.payload.control {
                    return Err(invalid());
                }
                self.owner_setup_active_goal(id, task)?;
                self.owner_setup_delegation(id, owner.payload.signer_delegation.as_ref())?;
                let prepared = v
                    .stages
                    .get(&OwnerSetupStageV1::PrepareGoal)
                    .and_then(|o| o.result.as_ref())
                    .ok_or_else(invalid)?;
                let OwnerSetupResultV1::PreparedGoal(prepared) = prepared else {
                    return Err(invalid());
                };
                if goal.payload.active_goal.as_ref() != Some(&prepared.payload) {
                    return Err(invalid());
                }
                let bound = v
                    .stages
                    .get(&OwnerSetupStageV1::Bind)
                    .and_then(|o| o.result.as_ref())
                    .ok_or_else(invalid)?;
                let OwnerSetupResultV1::Canonical { response, .. } = bound else {
                    return Err(invalid());
                };
                let ws::IntentOutcome::Committed { receipt, commit } = &response.payload.outcome
                else {
                    return Err(invalid());
                };
                let binding: oasis7_client_api::legacy_task_results::ExecutorReceipt =
                    serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
                let executor = task.executor_binding.as_ref().ok_or_else(invalid)?;
                if task.task_revision != binding.task_revision
                    || task.executor_epoch != binding.executor_epoch
                    || task.executor_binding != binding.executor_binding
                    || task.task_status != GameTaskStatusV1::Active
                    || executor.executor_public_key != v.plan.executor_public_key
                    || executor.executor_id != v.plan.executor_id
                    || executor.delegation_generation != v.plan.delegation_generation
                    || !owner
                        .payload
                        .control
                        .version
                        .commit
                        .satisfies_minimum(commit)
                        .map_err(|_| invalid())?
                    || !goal
                        .payload
                        .read_commit
                        .satisfies_minimum(commit)
                        .map_err(|_| invalid())?
                    || !v.plan.resource_refs.is_empty()
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
    pub(in crate::provisioning) fn owner_setup_phase_replay(
        &self,
    ) -> Result<Vec<(ProvisionStage, PhaseOriginal, Option<String>)>> {
        let mut phases = Vec::new();
        for (id, v) in &self.owner_setups {
            if v.stages
                .get(&OwnerSetupStageV1::CreateTask)
                .is_some_and(|o| o.admitted_at.is_some())
            {
                phases.push((
                    ProvisionStage::ReadyForSetup,
                    self.owner_setup_ready_phase(id)?,
                    None,
                ));
            }

            if let Some(result) = v
                .stages
                .get(&OwnerSetupStageV1::FinalObservation)
                .and_then(|o| o.result.as_ref())
            {
                self.validate_owner_setup_result(id, OwnerSetupStageV1::FinalObservation, result)?;
                phases.push((
                    ProvisionStage::Prepared,
                    PhaseOriginal {
                        operation_id: id.clone(),
                        body_digest: digest(&v.plan)?,
                        receipt_digest: Some(digest(result)?),
                    },
                    None,
                ));
            }
        }
        Ok(phases)
    }
}
