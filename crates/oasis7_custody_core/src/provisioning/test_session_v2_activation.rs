//! Exact V2 final preflight predicates; root creation remains one Account transaction.
use super::*;
impl AccountProvisionApprovalState {
    pub fn test_session_v2_world_read(
        &self,
        input: &ApprovalAdmissionInput,
    ) -> Result<ws::ReadWorldViewRequest> {
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &input.request.intent else {
            return Err(invalid());
        };
        let p = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let mut request = p.descriptor.owner_read.clone();
        request.observation_nonce = Some(digest(&(
            "oasis7.custody.test-session-preflight.v2",
            &scope.session_id,
            input.request.intent_digest().map_err(|_| invalid())?,
            input.approval.digest().map_err(|_| invalid())?,
        ))?);
        request.deadline_unix_ms = Some(plan.read_until.0.max(plan.cleanup_until.0));
        request.fixed_commit = None;
        request.formal_anchor = None;
        Ok(request)
    }
    pub(super) fn test_session_v2_activation_scope(
        &self,
        input: &ApprovalAdmissionInput,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<()> {
        self.test_session_v2_scope(input, &current.approval, now)?;
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &input.request.intent else {
            return Err(invalid());
        };
        let id = scope.session_id.as_str();
        let (write, read, cleanup) = (
            plan.write_deadline.0,
            plan.read_until.0,
            plan.cleanup_until.0,
        );
        let termination = &plan.termination;
        let c = current.approval.provision.approval;
        let (session, baseline) = self.participant_session(current.approval.native_scope)?;
        let native_account_id = &scope.native_account_id;
        let lifetime = &current.lifetime.lifetime;
        let receipt = &lifetime.status.receipt;
        let nonce = digest(&(
            "oasis7.custody.test-session-native-lifetime.v2",
            id,
            input.request.intent_digest().map_err(|_| invalid())?,
            input.approval.digest().map_err(|_| invalid())?,
        ))?;
        lifetime
            .validate_for(
                &baseline.status.receipt,
                &nonce,
                session.native_binding_generation,
                c.authority.service_epoch,
                now.get(),
            )
            .map_err(|_| invalid())?;
        lifetime
            .covers(UnixMillis::new(write.max(read).max(cleanup)))
            .map_err(|_| invalid())?;
        if receipt.native_account_id != *native_account_id
            || receipt.custody_account_id != self.draft.entry.custody_account_id
            || receipt.provision_id != self.draft.entry.provision_id
            || lifetime.status.session_ref != session.session_ref
            || current.private_sid_generation != session.private_sid_generation
            || receipt.identity_namespace != current.approval.provision.native_identity_namespace
        {
            return Err(invalid());
        }
        let w = current.world;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        if w.request.owner != participant.bundle.as_ref().ok_or_else(invalid)?.owner {
            return Err(invalid());
        }

        w.response
            .payload
            .validate(&w.request)
            .map_err(|_| invalid())?;
        let expected = self.test_session_v2_world_read(input)?;
        let saved = self.test_sessions_v2.get(id).ok_or_else(invalid)?;
        if saved.preflight_lifetime.as_ref() != Some(current.lifetime) {
            return Err(invalid());
        }

        if saved.preflight_read.as_ref().map(|r| r.as_ref()) != Some(&w.request)
            || !saved
                .preflight_steps
                .contains_key(&SessionPreflightStepV1::OwnerRead)
        {
            return Err(invalid());
        }
        if w.request.read.request != expected
            || w.request.read.subject_public_key != termination.initial_fence.owner_public_key
            || w.response.request_digest
                != ws::request_digest(
                    ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                    &w.request,
                )
                .map_err(|_| invalid())?
            || w.response
                .payload
                .control
                .view
                .game_task_control
                .as_ref()
                .map(GameTaskFenceV1::from_control)
                .as_ref()
                != Some(&termination.initial_fence)
            || w.response.payload.signer_delegation.as_ref()
                != Some(&termination.original_delegation)
        {
            return Err(invalid());
        }
        let state = self.test_sessions_v2.get(id).ok_or_else(invalid)?;
        let wrapper = state
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        if wrapper.api_version != 2 || wrapper.approved_input_digest != state.input_digest()? {
            return Err(invalid());
        }
        let source = &wrapper.source;
        super::source::validate_inspection_record(
            state.book.original(id).ok_or_else(invalid)?,
            source,
            source.verification.as_ref().ok_or_else(invalid)?,
            c,
            now.get(),
        )?;
        Ok(())
    }
    pub fn activate_test_session_v2_checks(
        &self,
        id: &str,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<Vec<ExactSignatureChecks>> {
        let s = self.test_sessions_v2.get(id).ok_or_else(invalid)?;
        if s.closing.is_some() || s.consumed_at.is_none() {
            return Err(invalid());
        }
        self.test_session_v2_activation_scope(&s.input, current, now)?;
        let wrapper = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        if wrapper.api_version != 2 || wrapper.approved_input_digest != s.input_digest()? {
            return Err(invalid());
        }
        let source = &wrapper.source;
        let checks = super::source::validate_inspection_record(
            s.book.original(id).ok_or_else(invalid)?,
            source,
            source.verification.as_ref().ok_or_else(invalid)?,
            current.approval.provision.approval,
            now.get(),
        )?;
        let checks = append_native_history_checks(
            checks,
            current.approval.provision.approval,
            NativeFact::Lifetime(current.lifetime),
            current.approval.native_scope.native_root_key_id,
            current.approval.native_scope.native_root_public_key,
            &current.lifetime.lifetime.digest().map_err(|_| invalid())?,
        )?;
        Ok(vec![
            append_owner_setup_checks(
                checks,
                OwnerSetupSignatureFact::World(
                    current.world,
                    &self.draft.world_identity.service_public_key,
                ),
            )?,
            current
                .approval
                .enrollment
                .fact
                .checks()
                .map_err(|_| invalid())?,
        ])
    }
    pub fn activate_test_session_v2(
        &mut self,
        id: &str,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionAuthorityV2> {
        for checks in self.activate_test_session_v2_checks(id, current, now)? {
            verifier.verify(&checks)?
        }
        let mut next = self.clone();
        let s = next.test_sessions_v2.get_mut(id).ok_or_else(invalid)?;
        if s.approved_at.is_some() {
            return Ok(s.clone());
        }
        let (plan, _) = s.header()?;
        let (
            write_deadline,
            read_until,
            cleanup_until,
            request_limit,
            transport_limit,
            business_limit,
            read_budget,
            cleanup_budget,
        ) = (
            plan.write_deadline,
            plan.read_until,
            plan.cleanup_until,
            plan.request_limit,
            plan.transport_limit,
            plan.business_limit,
            plan.read_budget.clone(),
            plan.cleanup_budget.clone(),
        );
        let approved_input_digest = s.input_digest()?;
        for (purpose, deadline, requests, transport, business) in [
            (
                SessionPurposeV1::Execution,
                write_deadline,
                request_limit,
                transport_limit,
                business_limit,
            ),
            (
                SessionPurposeV1::ReconcileRead,
                read_until,
                read_budget.requests,
                read_budget.transport,
                read_budget.business,
            ),
            (
                SessionPurposeV1::TerminationPlan,
                cleanup_until,
                cleanup_budget.requests,
                cleanup_budget.transport,
                cleanup_budget.business,
            ),
        ] {
            s.roots.insert(
                purpose,
                SessionRootV2 {
                    approved_input_digest: approved_input_digest.clone(),
                    root: SessionRootV1 {
                        purpose,
                        deadline: UnixMillis::new(deadline.0),
                        budgets: OperationBudgets {
                            requests: BudgetLedger::new(requests.0),
                            transport: BudgetLedger::new(transport.0),
                            business: BudgetLedger::new(business.0),
                        },
                        credential: None,
                        credential_history: Vec::new(),
                        revoked: false,
                    },
                },
            );
        }
        s.approved_at = Some(now);
        s.lease_until = Some(UnixMillis::new(
            now.get()
                .checked_add(90_000)
                .ok_or_else(invalid)?
                .min(write_deadline.0),
        ));
        s.native_lifetime = Some(current.lifetime.clone());
        let invite = s.enrollment.registration.request.target.invite_id.clone();
        for record in next
            .pairing_invites
            .values_mut()
            .filter(|r| r.original.invite_id == invite)
        {
            record.consumed_session = Some(id.into())
        }
        let result = next.test_sessions_v2.get(id).ok_or_else(invalid)?.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
}
