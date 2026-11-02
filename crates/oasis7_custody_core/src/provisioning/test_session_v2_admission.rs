//! Actual V2 enrollment and Owner-prepared binding before new MFA consumption.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionAdmissionV2 {
    pub input: ApprovalAdmissionInput,
    pub verify_nonce: String,
}
impl core::fmt::Debug for TestSessionAdmissionV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionAdmissionV2(<private>)")
    }
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn test_session_v2_scope(
        &self,
        input: &ApprovalAdmissionInput,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<()> {
        let CustodyIntentV1::TestSessionV2 { plan, scope } = &input.request.intent else {
            return Err(invalid());
        };
        plan.validate_scope(scope, now.get())
            .map_err(|_| invalid())?;
        if !plan.selection.is_exact_move0() {
            return Err(invalid());
        }
        current.enrollment.validate_physical()?;
        let c = current.provision.approval;
        let fact = current.enrollment.fact;
        let r = &fact.registration;
        let descriptor = &r.request.descriptor;
        let pair = &scope.client_pairing;
        let prepared = self.owner_setup_prepared_v2(&plan.termination.setup_id)?;
        let expected_fence = GameTaskFenceV1::from_control(prepared.control);
        let expected_basis = self.test_session_v2_basis_digest(scope, &c.authority)?;

        let term = &plan.termination;
        if self.session_inventory != SessionInventoryV1::Known
            || self.frozen()
            || self.provision.stage() != ProvisionStage::Prepared
            || input.operation_id != scope.session_id
            || self.test_sessions.contains_key(&scope.session_id)
            || input.request.context != c.authority
            || c.authority.security_version != self.provision.security_epoch()
            || c.authority.security_version.0 == 0
            || c.frozen
            || c.fence.check_epoch(c.authority.service_epoch.0).is_err()
            || current.observed_at.get() > now.get()
            || now.get() - current.observed_at.get() > 5000
            || current.provision.world_identity != &self.draft.world_identity
            || current.provision.native_identity_namespace.is_empty()
            || c.authority.custody_account_id != self.draft.entry.custody_account_id
            || c.authority.environment != self.draft.entry.environment
            || current.provision.policy.version != self.draft.entry.bootstrap_config_version.0
            || !current.provision.policy.managers.iter().any(|m| {
                m.principal == self.draft.entry.manager
                    && m.environments.contains(&self.draft.entry.environment)
            })
            || c.source.manager_account_id != self.draft.entry.manager.native_account_id
            || c.identity_namespace != self.draft.entry.manager.identity_namespace
            || plan.basis_digest != expected_basis
            || prepared.plan.digest().map_err(|_| invalid())? != plan.owner_setup_plan_digest
            || prepared.plan.selection != plan.selection
            || !plan.allowed_resource_refs.is_empty()
            || plan.business_limit.0 != 0
            || term.custody_account_id != self.draft.entry.custody_account_id
            || term.provision_id != self.draft.entry.provision_id
            || term.owner_key_ref != prepared.plan.owner_key_ref
            || term.executor_key_ref != prepared.plan.executor_key_ref
            || term.world != prepared.plan.world_identity
            || term.initial_fence != expected_fence
            || term.original_delegation != *prepared.delegation
            || term.prepared_result_digest != prepared.result_digest
            || pair.target != r.request.target
            || pair.enrollment_id != r.enrollment_id
            || pair.enrollment_digest != fact.digest().map_err(|_| invalid())?
            || pair.client_key_id != r.key_id
            || pair.client_public_key != descriptor.client_public_key
            || pair.host_instance_id != descriptor.host_instance_id
            || pair.descriptor_digest != r.descriptor_digest
            || descriptor.environment != c.authority.environment
            || now.get() >= r.expires_at.get()
            || scope.runtime_ref != descriptor.restricted_runtime_identity
            || scope.skill_ref != descriptor.skill_manifest_digest
            || scope.source_ref != descriptor.build_digest
            || current
                .deployments
                .configured_metadata(descriptor)
                .map_err(|_| invalid())?
                .is_none()
        {
            return Err(invalid());
        }
        let matched = match (&scope.client_binding, descriptor.transport_profile) {
            (
                crate::approval::ApprovedClientBindingV1::NativeBrokerCustodyV1 { client_key },
                HostTransportProfileV1::NativeBrokerCustodyV1,
            ) => client_key == &pair.client_public_key && pair.connector_public_key.is_none(),
            // This profile has no qualified Dot inference/Control consumer.
            _ => false,
        };
        if !matched {
            return Err(invalid());
        }
        let invite = self.pairing_invite(&r.request.target)?;
        if now.get() >= invite.original.expires_at.get()
            || invite.original.service_epoch != c.authority.service_epoch
            || invite.original.transport_profile != descriptor.transport_profile
            || invite.original.tool_profile != descriptor.tool_profile
            || invite
                .consumed_session
                .as_ref()
                .is_some_and(|old| old != &scope.session_id)
        {
            return Err(invalid());
        }
        let (_, status) = self.participant_session(current.native_scope)?;
        if status.status.receipt.native_account_id != scope.native_account_id {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn begin_test_session_v2_checks(
        &self,
        admission: &TestSessionAdmissionV2,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<Vec<ExactSignatureChecks>> {
        self.check_owner_setup_proof_unique(&admission.input)?;
        self.test_session_v2_scope(&admission.input, current, now)?;
        admission
            .input
            .request
            .validate(now.get())
            .map_err(|_| invalid())?;
        let CustodyIntentV1::TestSessionV2 { scope, .. } = &admission.input.request.intent else {
            return Err(invalid());
        };
        let id = scope.session_id.as_str();
        if self
            .test_sessions_v2
            .iter()
            .any(|(other, s)| other != id && s.closing.is_none())
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let reserved = self
            .test_sessions_v2
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
        if self.test_sessions.values().any(|v| v.closing.is_none()) {
            return Err(ProvisionApprovalError::Conflict);
        }
        let old_reserved = self
            .test_sessions
            .values()
            .map(|v| {
                v.roots
                    .values()
                    .filter(|r| !r.revoked && now.get() < r.deadline.get())
                    .count()
            })
            .sum::<usize>();
        if reserved
            .checked_add(old_reserved)
            .and_then(|v| v.checked_add(3))
            .is_none_or(|v| v > 4)
        {
            return Err(invalid());
        }
        let empty = ApprovalAdmissionBook::default();
        let book = self.test_sessions_v2.get(id).map_or(&empty, |s| &s.book);
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
    pub fn begin_test_session_v2(
        &mut self,
        admission: TestSessionAdmissionV2,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        for c in self.begin_test_session_v2_checks(&admission, current, now)? {
            verifier.verify(&c)?
        }
        let mut next = self.clone();
        let id = admission.input.operation_id.clone();
        let (session, _) = self.participant_session(current.native_scope)?;
        let termination_state = TestSessionTerminationStateV2::armed(&admission.input)?;
        let s = next
            .test_sessions_v2
            .entry(id.clone())
            .or_insert_with(|| TestSessionAuthorityV2 {
                input: admission.input.clone(),
                book: ApprovalAdmissionBook::default(),
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
                tool_nonce_ingresses: BTreeMap::new(),
                tool_nonce_links: BTreeMap::new(),
                context: None,
                roots: BTreeMap::new(),
                credential_claims: BTreeMap::new(),
                control: None,
                identity_transport: None,
                connector_transport: None,
                control_transports: BTreeMap::new(),
                client_http_admissions: BTreeMap::new(),
                lease_until: None,
                last_heartbeat: None,
                closing: None,
                controls: BTreeMap::new(),
                control_records: BTreeMap::new(),
                events: Vec::new(),
                control_logicals: BTreeMap::new(),
                challenges: BTreeMap::new(),
                actions: BTreeMap::new(),
                termination_state: termination_state.clone(),
                execution_book: None,
                preflight_world_access: None,
                native_model_budget: None,
                native_machine_registration: None,
            });
        if s.input != admission.input || s.enrollment != *current.enrollment.fact {
            return Err(ProvisionApprovalError::Conflict);
        }
        if s.control.is_none() {
            let budget = s.header()?.0.control_budget.clone();
            let mut requests = BudgetLedger::new(budget.requests.0);
            let request_id = digest(&(
                "oasis7.custody.test-session-preflight-control-id.v2",
                &id,
                s.input.request.intent_digest().map_err(|_| invalid())?,
            ))?;
            let body = digest(&(
                "oasis7.custody.test-session-preflight-control-body.v2",
                &s.input,
            ))?;
            requests.reserve(&request_id, 1).map_err(|_| invalid())?;
            s.control = Some(OperationBudgets {
                requests,
                transport: BudgetLedger::new(budget.transport_per_hop.0),
                business: BudgetLedger::new(0),
            });
            s.identity_transport = Some(BudgetLedger::new(budget.identity_transport.0));
            s.connector_transport =
                (budget.transport_hops == 2).then(|| BudgetLedger::new(budget.transport_per_hop.0));
            s.control_logicals.insert(request_id, body);
        }
        let record = s.book.begin(
            admission.input,
            admission.verify_nonce,
            current.provision.approval,
            now.get(),
            verifier,
        )?;
        if s.events.is_empty() {
            append_session_event_v2(s, SessionEventKindV2::PreflightReservation, now)?;
        }
        next.validate_persistence()?;
        *self = next;
        Ok(record)
    }
    pub fn claim_test_session_v2_approval_verification(
        &mut self,
        id: &str,
        now: UnixMillis,
    ) -> Result<crate::approval::ApprovalVerifyRequestV1> {
        let mut next = self.clone();
        let s = next.test_sessions_v2.get_mut(id).ok_or_else(invalid)?;
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
        append_session_event_v2(s, SessionEventKindV2::ApprovalVerificationClaim, now)?;
        let request = s
            .book
            .original(id)
            .ok_or_else(invalid)?
            .native_verify_request()?;
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn consume_test_session_v2_approval_checks(
        &self,
        commit: &CommitTestSessionV2,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<Vec<ExactSignatureChecks>> {
        let s = self
            .test_sessions_v2
            .get(&commit.session_id)
            .ok_or_else(invalid)?;
        self.test_session_v2_scope(&s.input, current, now)?;
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
    pub fn consume_test_session_v2_approval(
        &mut self,
        commit: CommitTestSessionV2,
        current: &TestSessionApprovalCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TestSessionAuthorityV2> {
        for checks in self.consume_test_session_v2_approval_checks(&commit, current, now)? {
            verifier.verify(&checks)?
        }
        let mut next = self.clone();
        let s = next
            .test_sessions_v2
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
        s.native_model_budget = SessionNativeModelBudgetV2::from_consumed_input(&s.input, now)?;
        s.consumed_at = Some(now);
        s.preflight_source_nonce = Some(commit.source_nonce);
        append_session_event_v2(s, SessionEventKindV2::ApprovalConsumed, now)?;
        let result = s.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
}

pub(super) fn pairing_v2(
    intent: &CustodyIntentV1,
) -> Result<(&str, &ClientPairingBindingV1, u64, u64, u64)> {
    let CustodyIntentV1::TestSessionV2 { plan, scope } = intent else {
        return Err(invalid());
    };
    scope.client_pairing.validate().map_err(|_| invalid())?;
    Ok((
        &scope.session_id,
        &scope.client_pairing,
        plan.write_deadline.0,
        plan.read_until.0,
        plan.cleanup_until.0,
    ))
}

impl AccountProvisionApprovalState {
    /// Stable approved metadata only; real management ACL/current and physical enrollment
    /// checks are still required by prepare/begin. This function grants no authority.
    pub fn test_session_v2_basis_digest(
        &self,
        scope: &ResourceTestSessionScopeV2,
        authority: &crate::approval::ApprovalContextV1,
    ) -> Result<String> {
        let plan = self.owner_setup_v2_plan().ok_or_else(invalid)?;
        let prepared = self.owner_setup_prepared_v2(&plan.setup_id)?;
        if scope.client_pairing.target.custody_account_id != self.draft.entry.custody_account_id
            || scope.client_pairing.target.provision_id != self.draft.entry.provision_id
            || scope.world_identity != self.draft.world_identity
            || scope.task_fence != GameTaskFenceV1::from_control(prepared.control)
            || scope.executor_key_ref != prepared.plan.executor_key_ref
        {
            return Err(invalid());
        }
        digest(&(
            "oasis7.custody.test-session-basis.v2",
            &self.draft.entry.environment,
            &self.draft.entry.custody_account_id,
            &self.draft.entry.provision_id,
            prepared.plan.digest().map_err(|_| invalid())?,
            &prepared.result_digest,
            scope,
            authority,
        ))
    }
}
