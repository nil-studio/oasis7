use super::admission::pairing;
use super::*;
impl AccountProvisionApprovalState {
    pub fn begin_test_session_checks(
        &self,
        admission: &TestSessionAdmissionV1,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<Vec<ExactSignatureChecks>> {
        self.check_owner_setup_proof_unique(&admission.input)?;
        self.test_session_scope(&admission.input, &admission.termination, current, now)?;
        admission
            .input
            .request
            .validate(now.get())
            .map_err(|_| invalid())?;
        let (id, _, _, _, _) = pairing(&admission.input.request.intent)?;
        if self
            .test_sessions
            .iter()
            .any(|(other, s)| other != id && s.closing.is_none())
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let reserved = self
            .test_sessions
            .iter()
            .filter(|(other, _)| other.as_str() != id)
            .map(|(_, s)| {
                if s.approved_at.is_none() && s.closing.is_none() {
                    3
                } else {
                    s.roots
                        .values()
                        .filter(|r| !r.revoked && now.get() < r.deadline.get())
                        .count()
                }
            })
            .sum::<usize>();
        if reserved.checked_add(3).ok_or_else(invalid)? > 4 {
            return Err(invalid());
        }
        let empty = ApprovalAdmissionBook::default();
        let book = self.test_sessions.get(id).map_or(&empty, |s| &s.book);
        let checks = book.begin_checks(
            &admission.input,
            &admission.verify_nonce,
            current.provision.approval,
            now.get(),
        )?;
        Ok(vec![
            checks,
            current.enrollment.fact.checks().map_err(|_| invalid())?,
        ])
    }
    pub fn begin_test_session(
        &mut self,
        admission: TestSessionAdmissionV1,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        for c in self.begin_test_session_checks(&admission, current, now)? {
            verifier.verify(&c)?
        }
        let mut next = self.clone();
        let id = admission.input.operation_id.clone();
        let (session, _) = self.participant_session(current.native_scope)?;
        let s = next
            .test_sessions
            .entry(id.clone())
            .or_insert_with(|| TestSessionAuthorityV1 {
                input: admission.input.clone(),
                book: ApprovalAdmissionBook::default(),
                termination: admission.termination.clone(),
                enrollment: current.enrollment.fact.clone(),
                session,
                native_lifetime: None,
                approved_at: None,
                budget_authorized_at: now,
                approval_verification_claimed_at: None,
                consumed_at: None,
                preflight_source_nonce: None,
                preflight_claimed_at: None,
                preflight_steps: BTreeMap::new(),
                preflight_oracle_reads: 0,
                preflight_access: None,
                preflight_lifetime: None,
                preflight_read: None,
                sources: BTreeMap::new(),
                source_oracle_reads: BTreeMap::new(),
                source_funding: BTreeMap::new(),
                purpose_transports: BTreeMap::new(),
                purpose_http_admissions: BTreeMap::new(),
                roots: BTreeMap::new(),
                control: None,
                identity_transport: None,
                connector_transport: None,
                control_transports: BTreeMap::new(),
                client_http_admissions: BTreeMap::new(),
                lease_until: None,
                last_heartbeat: None,
                closing: None,
                controls: BTreeMap::new(),
                control_logicals: BTreeMap::new(),
                challenges: BTreeMap::new(),
                actions: BTreeMap::new(),
                termination_slots: BTreeMap::new(),
            });
        if s.input != admission.input
            || s.termination != admission.termination
            || s.enrollment != *current.enrollment.fact
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        if s.control.is_none() {
            initialize_control_budget(s)?;
            super::lifecycle::reserve_control_logical(
                s,
                &digest(&("oasis7.custody.test-session-preflight-control-id.v1", &id))?,
                &digest(&(
                    "oasis7.custody.test-session-preflight-control-body.v1",
                    &s.input,
                ))?,
            )?;
        }
        let record = s.book.begin(
            admission.input,
            admission.verify_nonce,
            current.provision.approval,
            now.get(),
            verifier,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(record)
    }
    pub fn claim_test_session_approval_verification(
        &mut self,
        id: &str,
        now: UnixMillis,
    ) -> Result<crate::approval::ApprovalVerifyRequestV1> {
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        if self.frozen()
            || s.closing.is_some()
            || s.approval_verification_claimed_at.is_some()
            || s.consumed_at.is_some()
            || now.get() >= s.input.approval.proof.expires_at.0
        {
            return Err(invalid());
        }
        s.identity_transport
            .as_mut()
            .ok_or_else(invalid)?
            .charge(1)
            .map_err(|_| invalid())?;
        s.approval_verification_claimed_at = Some(now);
        let request = s
            .book
            .original(id)
            .ok_or_else(invalid)?
            .native_verify_request()?;
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn consume_test_session_approval_checks(
        &self,
        commit: &CommitTestSessionV1,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<Vec<ExactSignatureChecks>> {
        let s = self
            .test_sessions
            .get(&commit.session_id)
            .ok_or_else(invalid)?;
        self.test_session_scope(&s.input, &s.termination, current, now)?;
        if s.closing.is_some() || s.approval_verification_claimed_at.is_none() {
            return Err(invalid());
        }
        Ok(vec![
            s.book.commit_checks(
                &commit.session_id,
                &commit.signed,
                current.provision.approval,
                now.get(),
            )?,
            current.enrollment.fact.checks().map_err(|_| invalid())?,
        ])
    }
    pub fn consume_test_session_approval(
        &mut self,
        commit: CommitTestSessionV1,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionAuthorityV1> {
        for checks in self.consume_test_session_approval_checks(&commit, current, now)? {
            verifier.verify(&checks)?
        }
        let mut next = self.clone();
        let s = next
            .test_sessions
            .get_mut(&commit.session_id)
            .ok_or_else(invalid)?;
        if s.consumed_at.is_some() {
            return Ok(s.clone());
        }
        s.book.commit(
            &commit.session_id,
            commit.signed,
            current.provision.approval,
            now.get(),
            verifier,
        )?;
        if !hex64(&commit.source_nonce) {
            return Err(invalid());
        }
        s.consumed_at = Some(now);
        s.preflight_source_nonce = Some(commit.source_nonce);
        let result = s.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub fn activate_test_session_checks(
        &self,
        id: &str,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<Vec<ExactSignatureChecks>> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        if s.closing.is_some() || s.consumed_at.is_none() {
            return Err(invalid());
        }
        self.test_session_activation_scope(&s.input, &s.termination, current, now)?;
        let source = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
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
    pub fn activate_test_session(
        &mut self,
        id: &str,
        current: &TestSessionCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionAuthorityV1> {
        for checks in self.activate_test_session_checks(id, current, now)? {
            verifier.verify(&checks)?
        }
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        if s.approved_at.is_some() {
            return Ok(s.clone());
        }
        let CustodyIntentV1::TestSession {
            write_deadline,
            read_until,
            cleanup_until,
            request_limit,
            transport_limit,
            business_limit,
            read_budget,
            cleanup_budget,
            ..
        } = &s.input.request.intent
        else {
            return Err(invalid());
        };
        for (purpose, deadline, requests, transport, business) in [
            (
                SessionPurposeV1::Execution,
                *write_deadline,
                *request_limit,
                *transport_limit,
                *business_limit,
            ),
            (
                SessionPurposeV1::ReconcileRead,
                *read_until,
                read_budget.requests,
                read_budget.transport,
                read_budget.business,
            ),
            (
                SessionPurposeV1::TerminationPlan,
                *cleanup_until,
                cleanup_budget.requests,
                cleanup_budget.transport,
                cleanup_budget.business,
            ),
        ] {
            s.roots.insert(
                purpose,
                SessionRootV1 {
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
        let result = next.test_sessions.get(id).ok_or_else(invalid)?.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
}
fn initialize_control_budget(s: &mut TestSessionAuthorityV1) -> Result<()> {
    let CustodyIntentV1::TestSession { control_budget, .. } = &s.input.request.intent else {
        return Err(invalid());
    };
    s.control = Some(OperationBudgets {
        requests: BudgetLedger::new(control_budget.requests.0),
        transport: BudgetLedger::new(control_budget.transport_per_hop.0),
        business: BudgetLedger::new(0),
    });
    s.identity_transport = Some(BudgetLedger::new(control_budget.identity_transport.0));
    s.connector_transport = (control_budget.transport_hops == 2)
        .then(|| BudgetLedger::new(control_budget.transport_per_hop.0));
    Ok(())
}
