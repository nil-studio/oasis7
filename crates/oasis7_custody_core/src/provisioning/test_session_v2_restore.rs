//! Strict restoration cannot reconstruct V2 authority from V1 snapshots or absent fields.
use super::*;
fn id_nonempty(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_test_session_v2_persistence(&self) -> Result<()> {
        if self.test_sessions_v2.len() > 256
            || (self.session_inventory != SessionInventoryV1::Known
                && !self.test_sessions_v2.is_empty())
        {
            return Err(invalid());
        }
        for (id, s) in &self.test_sessions_v2 {
            s.validate_header()?;
            let (plan, scope) = s.header()?;
            let pair = &scope.client_pairing;
            let (write, read, cleanup) = (
                plan.write_deadline.0,
                plan.read_until.0,
                plan.cleanup_until.0,
            );
            if id != &scope.session_id
                || self.test_sessions.contains_key(id)
                || scope.client_pairing.target.custody_account_id
                    != self.draft.entry.custody_account_id
                || scope.client_pairing.target.provision_id != self.draft.entry.provision_id
                || s.enrollment.registration.request.target != pair.target
                || s.enrollment.digest().map_err(|_| invalid())? != pair.enrollment_digest
                || s.enrollment.registration.enrollment_id != pair.enrollment_id
                || s.enrollment
                    .registration
                    .request
                    .descriptor
                    .digest()
                    .map_err(|_| invalid())?
                    != pair.descriptor_digest
            {
                return Err(invalid());
            }
            self.check_owner_setup_proof_unique(&s.input)?;
            s.enrollment.checks().map_err(|_| invalid())?;
            let record = s.book.original(id).ok_or_else(invalid)?;
            if s.consumed_at.map(UnixMillis::get) != record.consumed_at()
                || s.consumed_at.is_some() != (record.stage() == ApprovalAdmissionStage::Consumed)
                || s.consumed_at
                    .is_some_and(|t| t.get() < s.budget_authorized_at.get())
                || s.approval_verification_claimed_at
                    .is_some_and(|t| t.get() < s.budget_authorized_at.get())
                || s.consumed_at.is_some() && s.approval_verification_claimed_at.is_none()
            {
                return Err(invalid());
            }
            if let Some((reason, at)) = s.closing {
                let justified = match reason {
                    TestSessionClosingReasonV1::Deadline => {
                        at.get()
                            >= if s.consumed_at.is_none() {
                                write.min(s.input.approval.proof.expires_at.0)
                            } else {
                                write
                            }
                    }
                    TestSessionClosingReasonV1::LeaseExpired => {
                        s.approved_at.is_some()
                            && s.lease_until.is_some_and(|lease| at.get() >= lease.get())
                    }
                    TestSessionClosingReasonV1::BudgetExhausted => {
                        s.approved_at
                            .is_some_and(|approved| at.get() >= approved.get())
                            && (super::v2_credential_http::control_exhausted(s)
                                || super::source_v2::v2_purpose_exhausted(s))
                    }
                    TestSessionClosingReasonV1::Finished | TestSessionClosingReasonV1::Aborted => {
                        super::v2_control::validate_control_records_v2(self, s).is_ok()
                    }
                    _ => false,
                };
                if !justified || at.get() < s.budget_authorized_at.get() {
                    return Err(invalid());
                }
            }
            self.validate_session_v2_preflight_persistence(id, s)?;
            self.validate_session_v2_control_persistence(s)?;
            super::v2_credential_restore::validate(self, id, s)?;
            super::v2_control::validate_control_records_v2(self, s)?;
            super::v2_purpose_http::validate_purpose_http_v2(s)?;
            self.validate_session_v2_tool_nonce_persistence(s)?;
            self.validate_execution_book_v2(s)?;
            super::v2_journal::validate(s)?;
            // These old V1 execution/control formats never become V2 execution authority.
            if s.context.is_some()
                || !s.actions.is_empty()
                || s.purpose_transports.keys().any(|key| {
                    !s.tool_nonce_ingresses
                        .values()
                        .any(|n| n.owner_attempt == *key || n.account_attempt == *key)
                        && !s.execution_book.as_ref().is_some_and(|book| {
                            book.owns_lifetime_transport(key)
                                || book.owns_prepare_factory_transport(s, key)
                                || book.owns_prepare_current_transport(s, key)
                        })
                })
            {
                return Err(invalid());
            }
            if s.approved_at.is_none() {
                if !s.roots.is_empty()
                    || !s.credential_claims.is_empty()
                    || !s.control_records.is_empty()
                    || !s.challenges.is_empty()
                    || s.native_lifetime.is_some()
                    || s.lease_until.is_some()
                    || s.execution_book.is_some()
                    || s.termination_state.stop.is_some()
                    || s.termination_state.close.is_some()
                    || !s.termination_state.transports.is_empty()
                {
                    return Err(invalid());
                }
            } else {
                let approved = s.approved_at.ok_or_else(invalid)?;
                if s.consumed_at.is_none_or(|t| approved.get() < t.get())
                    || s.roots.len() != 3
                    || s.lease_until
                        .is_none_or(|t| t.get() > write || t.get() < approved.get())
                    || self.pairing_invite(&pair.target)?.consumed_session.as_ref() != Some(id)
                {
                    return Err(invalid());
                }
                s.native_lifetime
                    .as_ref()
                    .ok_or_else(invalid)?
                    .lifetime
                    .covers(UnixMillis::new(write.max(read).max(cleanup)))
                    .map_err(|_| invalid())?;
                for (purpose, deadline, requests, transport, business) in [
                    (
                        SessionPurposeV1::Execution,
                        write,
                        plan.request_limit,
                        plan.transport_limit,
                        plan.business_limit,
                    ),
                    (
                        SessionPurposeV1::ReconcileRead,
                        read,
                        plan.read_budget.requests,
                        plan.read_budget.transport,
                        plan.read_budget.business,
                    ),
                    (
                        SessionPurposeV1::TerminationPlan,
                        cleanup,
                        plan.cleanup_budget.requests,
                        plan.cleanup_budget.transport,
                        plan.cleanup_budget.business,
                    ),
                ] {
                    let root = s.roots.get(&purpose).ok_or_else(invalid)?;
                    if root.approved_input_digest != s.input_digest()?
                        || root.purpose != purpose
                        || root.deadline.get() != deadline
                    {
                        return Err(invalid());
                    }
                    let mut expected = OperationBudgets {
                        requests: BudgetLedger::new(requests.0),
                        transport: BudgetLedger::new(transport.0),
                        business: BudgetLedger::new(business.0),
                    };
                    let sources = s
                        .source_funding
                        .values()
                        .filter(|f| f.purpose() == Some(purpose))
                        .count() as u64;
                    expected.transport.charge(sources).map_err(|_| invalid())?;
                    expected
                        .transport
                        .charge(
                            s.purpose_transports
                                .values()
                                .filter(|a| a.purpose() == purpose)
                                .count() as u64,
                        )
                        .map_err(|_| invalid())?;
                    expected
                        .transport
                        .charge(
                            s.purpose_http_admissions
                                .values()
                                .filter(|a| a.purpose == purpose)
                                .count() as u64,
                        )
                        .map_err(|_| invalid())?;
                    if purpose == SessionPurposeV1::Execution {
                        expected
                            .requests
                            .charge(
                                s.execution_book
                                    .as_ref()
                                    .map_or(0, |book| book.execution_request_count()),
                            )
                            .map_err(|_| invalid())?;
                    }
                    if purpose == SessionPurposeV1::TerminationPlan {
                        let allocations = s.termination_state.allocated_request_count();
                        expected
                            .requests
                            .charge(allocations)
                            .map_err(|_| invalid())?;
                        expected
                            .transport
                            .charge(s.termination_state.paid_transport_count()?)
                            .map_err(|_| invalid())?;
                    }
                    if expected != root.budgets {
                        return Err(invalid());
                    }
                }
            }
            if s.sources.len() > 160
                || s.sources.len() != s.source_funding.len()
                || s.sources.len() != s.source_oracle_reads.len()
                || s.source_funding
                    .keys()
                    .any(|id| !s.sources.contains_key(id))
                || s.source_oracle_reads
                    .iter()
                    .any(|(id, n)| *n > 8 || !s.sources.contains_key(id))
            {
                return Err(invalid());
            }
            let mut source_nonces = std::collections::BTreeSet::new();
            for (key, wrapper) in &s.sources {
                if !source_nonces.insert(&wrapper.source.verify_nonce) {
                    return Err(invalid());
                }
                let source = &wrapper.source;
                if wrapper.api_version != 2
                    || wrapper.approved_input_digest != s.input_digest()?
                    || s.consumed_at.is_none()
                    || key != &source.operation_id
                    || !id_nonempty(key)
                    || !hex64(&source.verify_nonce)
                    || super::v2_control::id_in_use_v2(s, key)
                    || super::v2_control::id_in_use_v2(s, &source.verify_nonce)
                    || source.created_at.0 < s.consumed_at.ok_or_else(invalid)?.get()
                    || source.expires_at.0 <= source.created_at.0
                    || source.expires_at.0
                        > source
                            .created_at
                            .0
                            .checked_add(30_000)
                            .ok_or_else(invalid)?
                    || source.expires_at.0 > read.max(cleanup)
                    || source.claimed_at.is_some() != source.request_bytes.is_some()
                    || source.verification.is_some() != source.verified_at.is_some()
                    || s.termination_state.transport_id_in_use(key)
                {
                    return Err(invalid());
                }
                if let Some(bytes) = &source.request_bytes {
                    let mut expected = record.native_verify_request()?;
                    expected.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
                    expected.verify_nonce = source.verify_nonce.clone();
                    if &serde_json::to_string(&expected).map_err(|_| invalid())? != bytes {
                        return Err(invalid());
                    }
                }
                if let Some(v) = &source.verification {
                    if source.claimed_at.is_none()
                        || v.verification.verify_nonce != source.verify_nonce
                        || v.verification.proof_digest
                            != s.input.approval.digest().map_err(|_| invalid())?
                        || v.verification.purpose
                            != crate::approval::VerificationPurposeV1::InspectOrigin
                        || v.verification.origin_state != crate::approval::OriginStateV1::Active
                    {
                        return Err(invalid());
                    }
                }
            }
        }
        Ok(())
    }
    fn validate_session_v2_control_persistence(&self, s: &TestSessionAuthorityV2) -> Result<()> {
        let control_budget = &s.header()?.0.control_budget;
        let mut control = OperationBudgets {
            requests: BudgetLedger::new(control_budget.requests.0),
            transport: BudgetLedger::new(control_budget.transport_per_hop.0),
            business: BudgetLedger::new(0),
        };
        for (id, body) in &s.control_logicals {
            if !id_nonempty(id) || !hex64(body) {
                return Err(invalid());
            }
            control.requests.reserve(id, 1).map_err(|_| invalid())?;
        }
        let mut identity = BudgetLedger::new(control_budget.identity_transport.0);
        identity
            .charge(
                s.source_funding
                    .values()
                    .filter(|f| **f == SessionSourceFundingV1::Control)
                    .count() as u64
                    + u64::from(s.approval_verification_claimed_at.is_some())
                    // The separate machine Active HTTP is charged even while
                    // Await/unsigned/unknown; registration replaces OwnerRead.
                    + u64::from(s.native_machine_registration.as_ref().is_some_and(|m| m.active.is_some()))
                    + s.preflight_steps
                        .keys()
                        .map(|step| {
                            if *step == SessionPreflightStepV1::FreshAccessPair {
                                4
                            } else {
                                1
                            }
                        })
                        .sum::<u64>(),
            )
            .map_err(|_| invalid())?;
        let mut connector = (control_budget.transport_hops == 2)
            .then(|| BudgetLedger::new(control_budget.transport_per_hop.0));
        if s.client_http_admissions.len() > 240 {
            return Err(invalid());
        }
        for (id, a) in &s.client_http_admissions {
            if !id_nonempty(id)
                || !hex64(&a.signed_attempt_digest)
                || a.hop != SessionControlHopV1::Host
                || s.control_transports.get(id) != Some(&SessionControlHopV1::Host)
                || a.charged_at.get() < s.approved_at.ok_or_else(invalid)?.get()
                || a.charged_at.get()
                    >= admission_v2::pairing_v2(&s.input.request.intent)?
                        .3
                        .max(admission_v2::pairing_v2(&s.input.request.intent)?.4)
            {
                return Err(invalid());
            }
        }
        for (attempt, hop) in &s.control_transports {
            if !id_nonempty(attempt) {
                return Err(invalid());
            }
            match hop {
                SessionControlHopV1::Host => {
                    if !s.client_http_admissions.contains_key(attempt) {
                        return Err(invalid());
                    }
                    control.transport.charge(1)
                }
                SessionControlHopV1::Identity => identity.charge(1),
                SessionControlHopV1::Connector => connector.as_mut().ok_or_else(invalid)?.charge(1),
            }
            .map_err(|_| invalid())?;
        }
        if s.control.as_ref() != Some(&control)
            || s.identity_transport.as_ref() != Some(&identity)
            || s.connector_transport != connector
        {
            return Err(invalid());
        }
        Ok(())
    }
}
