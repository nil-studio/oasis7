use super::*;
impl AccountProvisionApprovalState {
    pub fn owner_setup_reconcile_archive_checks(
        &self,
        id: &str,
        attempt: &str,
        fact: &OwnerSetupCanonicalFactV1,
        context: &OwnerSetupArchiveContext<'_>,
    ) -> Result<ExactSignatureChecks> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let a = v.attempts.get(attempt).ok_or_else(invalid)?;
        if fact.source != OwnerSetupCanonicalSourceV1::Lookup
            || a.lookup_claimed_at.is_none()
            || context.world_public_key != v.plan.world_identity.service_public_key
            || context.world_public_key != self.draft.world_identity.service_public_key
            || a.fact.as_ref().is_some_and(|old| old != fact)
        {
            return Err(invalid());
        }
        self.validate_owner_setup_canonical_observation(&v.plan.setup_id, v.plan.stage, fact)?;
        Ok(owner_setup_archive_signature_checks(
            &digest(&(id, attempt))?,
            v.plan.stage,
            fact,
            context.world_public_key,
            self.owner_setup_outbox(&v.plan.setup_id, v.plan.stage)
                .and_then(|o| o.original.as_ref())
                .ok_or_else(invalid)?,
            &v.plan.setup_plan_digest,
        )?)
    }
    pub fn archive_owner_setup_reconcile_fact(
        &mut self,
        id: &str,
        attempt: &str,
        fact: OwnerSetupCanonicalFactV1,
        context: &OwnerSetupArchiveContext<'_>,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupCanonicalFactV1> {
        verifier
            .verify(&self.owner_setup_reconcile_archive_checks(id, attempt, &fact, context)?)?;
        let mut next = self.clone();
        let a = next
            .owner_setup_reconciles
            .get_mut(id)
            .and_then(|v| v.attempts.get_mut(attempt))
            .ok_or_else(invalid)?;
        a.fact = Some(fact.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(fact)
    }
    pub fn owner_setup_reconcile_result_checks(
        &self,
        id: &str,
        attempt: &str,
        e: &OwnerSetupReconcileCompletionV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let a = v.attempts.get(attempt).ok_or_else(invalid)?;
        let mut checks = self.owner_setup_reconcile_access_checks(
            id,
            attempt,
            a.access.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )?;
        if a.completion_reserved_at.is_none_or(|t| t.0 > now)
            || a.admitted_at.is_none_or(|t| t.0 > now || now - t.0 > 5000)
        {
            return Err(invalid());
        }
        self.validate_reconcile_world(id, attempt, &e.world)?;
        self.validate_reconcile_result(id, attempt, e)?;
        match &e.result {
            OwnerSetupResultV1::Canonical { .. } => {
                let lookup = e.lookup_session.as_ref().ok_or_else(invalid)?;
                if lookup.status.status.observed_at.0 < a.lookup_claimed_at.ok_or_else(invalid)?.0 {
                    return Err(invalid());
                }
                self.reconcile_session(
                    id,
                    &runtime::reconcile_nonce(id, attempt, &a.nonce, "lookup")?,
                    lookup,
                    now,
                    current.provision.approval.authority.service_epoch,
                )?;
                checks = append_native_history_checks(
                    checks,
                    current.provision.approval,
                    NativeFact::Status(&lookup.status),
                    current.native_scope.native_root_key_id,
                    current.native_scope.native_root_public_key,
                    &digest(&(id, attempt, "lookup-session"))?,
                )?;
            }
            _ => {
                if e.lookup_session.is_some() {
                    return Err(invalid());
                }
            }
        }
        if e.session.status.status.observed_at.0 < a.completion_reserved_at.ok_or_else(invalid)?.0 {
            return Err(invalid());
        }
        self.reconcile_session(
            id,
            &runtime::reconcile_nonce(id, attempt, &a.nonce, "poststate")?,
            &e.session,
            now,
            current.provision.approval.authority.service_epoch,
        )?;
        checks = append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::World(&e.world, &v.plan.world_identity.service_public_key),
        )?;
        checks = append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::Result(&e.result, &v.plan.world_identity.service_public_key),
        )?;
        Ok(append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&e.session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&(id, attempt, "result"))?,
        )?)
    }
    pub fn complete_owner_setup_reconcile(
        &mut self,
        id: &str,
        attempt: &str,
        e: OwnerSetupReconcileCompletionV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupOutboxV1> {
        verifier
            .verify(&self.owner_setup_reconcile_result_checks(id, attempt, &e, current, now)?)?;
        let mut next = self.clone();
        let v = next
            .owner_setup_reconciles
            .get_mut(id)
            .ok_or_else(invalid)?;
        let plan = v.plan.clone();
        let a = v.attempts.get_mut(attempt).ok_or_else(invalid)?;
        if let Some(old) = &a.result {
            return if old == &e.result
                && a.result_session.as_ref() == Some(&e.session)
                && a.result_lookup_session == e.lookup_session
                && a.final_world.as_ref() == Some(&e.world)
            {
                Ok(next
                    .owner_setup_outbox(&plan.setup_id, plan.stage)
                    .ok_or_else(invalid)?
                    .clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        a.result = Some(e.result.clone());
        a.result_session = Some(e.session.clone());
        a.result_lookup_session = e.lookup_session;
        a.final_world = Some(e.world);
        a.completed_at = Some(DecimalU64(now));
        let o = next
            .owner_setups
            .get_mut(&plan.setup_id)
            .and_then(|v| v.stages.get_mut(&plan.stage))
            .ok_or_else(invalid)?;
        if e.result.terminal() {
            if o.result.as_ref().is_some_and(|old| old != &e.result) {
                return Err(ProvisionApprovalError::Conflict);
            }
            o.operation
                .confirm(digest(&e.result)?)
                .map_err(|_| invalid())?;
            o.result = Some(e.result);
            o.session = Some(e.session);
            o.completed_at = Some(DecimalU64(now));
            o.result_provenance = Some(OwnerSetupResultProvenanceV1::Reconcile {
                reconcile_id: id.into(),
                attempt_id: attempt.into(),
            });
        }
        let result = o.clone();
        if plan.stage == OwnerSetupStageV1::FinalObservation
            && result.result.as_ref().is_some_and(|r| r.succeeded())
        {
            let phase = PhaseOriginal {
                operation_id: plan.setup_id.clone(),
                body_digest: digest(
                    &next
                        .owner_setups
                        .get(&plan.setup_id)
                        .ok_or_else(invalid)?
                        .plan,
                )?,
                receipt_digest: Some(digest(result.result.as_ref().ok_or_else(invalid)?)?),
            };
            next.provision.confirm_owner_setup(phase)?;
        }
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub(super) fn validate_reconcile_result(
        &self,
        id: &str,
        attempt: &str,
        e: &OwnerSetupReconcileCompletionV1,
    ) -> Result<()> {
        let v = self.owner_setup_reconciles.get(id).ok_or_else(invalid)?;
        let a = v.attempts.get(attempt).ok_or_else(invalid)?;
        match &e.result {
            OwnerSetupResultV1::Canonical {
                response: _,
                from_lookup,
            } => {
                if !from_lookup
                    || a.lookup_claimed_at.is_none()
                    || a.fact.as_ref() != e.result.canonical_fact().as_ref()
                {
                    return Err(invalid());
                }
                self.validate_owner_setup_result(&v.plan.setup_id, v.plan.stage, &e.result)?;
                if e.result.terminal() {
                    self.validate_reconcile_poststate(&v.plan, &e.result, &e.world)?;
                }
            }
            OwnerSetupResultV1::PreparedGoal(_) => {
                if v.plan.stage != OwnerSetupStageV1::PrepareGoal {
                    return Err(invalid());
                }
                self.validate_owner_setup_result(&v.plan.setup_id, v.plan.stage, &e.result)?;
                self.validate_reconcile_poststate(&v.plan, &e.result, &e.world)?;
            }
            OwnerSetupResultV1::FinalObservation { owner, goal } => {
                if v.plan.stage != OwnerSetupStageV1::FinalObservation
                    || owner.as_ref() != &e.world.response
                {
                    return Err(invalid());
                }
                let request = a.goal_request.as_ref().ok_or_else(invalid)?;
                ws::private_goal_read::verify_current_response(
                    request,
                    goal,
                    &v.plan.world_identity.service_public_key,
                )
                .map_err(|_| invalid())?;
                self.validate_reconcile_poststate(&v.plan, &e.result, &e.world)?;
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
                self.owner_setup_active_goal(&v.plan.setup_id, task)?;
                let setup = self
                    .owner_setups
                    .get(&v.plan.setup_id)
                    .ok_or_else(invalid)?;
                let Some(OwnerSetupResultV1::PreparedGoal(prepared)) = setup
                    .stages
                    .get(&OwnerSetupStageV1::PrepareGoal)
                    .and_then(|o| o.result.as_ref())
                else {
                    return Err(invalid());
                };
                if goal.payload.active_goal.as_ref() != Some(&prepared.payload)
                    || !setup.plan.resource_refs.is_empty()
                {
                    return Err(invalid());
                }
            }
        }
        validate_owner_setup_signature_fact(OwnerSetupSignatureFact::Result(
            &e.result,
            &v.plan.world_identity.service_public_key,
        ))?;
        Ok(())
    }
    fn validate_reconcile_poststate(
        &self,
        p: &OwnerSetupReconcilePlanV1,
        result: &OwnerSetupResultV1,
        w: &OwnerSetupWorldEvidenceV1,
    ) -> Result<()> {
        let mut expected = self.owner_setup_expected_control(&p.setup_id, p.stage)?;
        let mut delegated = matches!(
            p.stage,
            OwnerSetupStageV1::Bind | OwnerSetupStageV1::FinalObservation
        );
        if let OwnerSetupResultV1::Canonical { response, .. } = result {
            if let ws::IntentOutcome::Committed { receipt, commit } = &response.payload.outcome {
                if !w
                    .response
                    .payload
                    .control
                    .version
                    .commit
                    .satisfies_minimum(commit)
                    .map_err(|_| invalid())?
                {
                    return Err(invalid());
                }
                match p.stage {
                    OwnerSetupStageV1::CreateTask => {
                        expected =
                            Some(serde_json::from_value(receipt.clone()).map_err(|_| invalid())?)
                    }
                    OwnerSetupStageV1::PublishGoal => {
                        let published: oasis7_client_api::legacy_task_results::PublishReceipt =
                            serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
                        super::super::requests::apply_published_control(
                            expected.as_mut().ok_or_else(invalid)?,
                            published,
                        );
                    }
                    OwnerSetupStageV1::Delegation => delegated = true,
                    OwnerSetupStageV1::Bind => {
                        let bound: oasis7_client_api::legacy_task_results::ExecutorReceipt =
                            serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
                        let task = expected.as_mut().ok_or_else(invalid)?;
                        task.task_revision = bound.task_revision;
                        task.task_status = bound.task_status;
                        task.executor_epoch = bound.executor_epoch;
                        task.executor_id = bound
                            .executor_binding
                            .as_ref()
                            .map(|v| v.executor_id.clone());
                        task.executor_binding = bound.executor_binding;
                    }
                    _ => return Err(invalid()),
                }
            } else {
                delegated = matches!(
                    p.stage,
                    OwnerSetupStageV1::Bind | OwnerSetupStageV1::FinalObservation
                );
            }
        }
        if w.response.payload.control.view.game_task_control != expected {
            return Err(invalid());
        }
        let actual = w.response.payload.signer_delegation.as_ref();
        if delegated {
            let Some(OwnerSetupOriginalV1::Canonical { submit, .. }) = self
                .owner_setup_outbox(&p.setup_id, OwnerSetupStageV1::Delegation)
                .and_then(|o| o.original.as_ref())
            else {
                return Err(invalid());
            };
            let WorldServicePayloadV1::Delegation(signed) = &submit.signed_payload else {
                return Err(invalid());
            };
            if actual != Some(&signed.request) {
                return Err(invalid());
            }
        } else if actual.is_some() {
            return Err(invalid());
        }
        Ok(())
    }
}
