use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_test_session_persistence(&self) -> Result<()> {
        if self.session_inventory == SessionInventoryV1::UnknownLegacy {
            return if self.test_sessions.is_empty() {
                Ok(())
            } else {
                Err(ProvisionApprovalError::InvalidPersistence)
            };
        }
        if self.test_sessions.len() > 256 {
            return Err(invalid());
        }
        let mut enrollments = BTreeSet::new();
        let mut invites = BTreeSet::new();
        let mut proofs: Vec<ApprovalAdmissionInput> = self
            .book
            .records()
            .chain(
                self.managed_logins
                    .keys()
                    .filter_map(|id| self.managed_login_original(id)),
            )
            .chain(self.owner_setups.values().flat_map(|v| v.book.records()))
            .chain(
                self.owner_setup_reconciles
                    .values()
                    .flat_map(|v| v.book.records()),
            )
            .map(|r| r.input().clone())
            .collect();
        for (id, s) in &self.test_sessions {
            let (intent_id, pair, write, read, cleanup) =
                admission::pairing(&s.input.request.intent)?;
            s.termination.validate()?;
            if id != intent_id
                || id != &s.input.operation_id
                || pair.target.custody_account_id != self.draft.entry.custody_account_id
                || pair.target.provision_id != self.draft.entry.provision_id
                || s.termination.custody_account_id != self.draft.entry.custody_account_id
                || s.termination.provision_id != self.draft.entry.provision_id
                || s.termination.session_id != *id
                || s.termination.cleanup_until.get() != cleanup
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
                || !enrollments.insert(pair.enrollment_id.clone())
                || !invites.insert(pair.target.invite_id.clone())
            {
                return Err(invalid());
            }
            s.enrollment.checks().map_err(|_| invalid())?;
            self.validate_session_purpose_http_persistence(s)?;
            for record in s.book.records() {
                if record.input() != &s.input
                    || proofs.iter().any(|old| same_proof(old, record.input()))
                {
                    return Err(invalid());
                }
                proofs.push(record.input().clone());
            }
            self.validate_session_preflight_persistence(id, s)?;
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
            if s.approved_at.is_none() {
                if !s.roots.is_empty()
                    || s.native_lifetime.is_some()
                    || s.lease_until.is_some()
                    || !s.challenges.is_empty()
                    || !s.actions.is_empty()
                    || !s.controls.is_empty()
                    || !s.termination_slots.is_empty()
                {
                    return Err(invalid());
                }
            } else {
                let approved = s.approved_at.ok_or_else(invalid)?;
                if s.consumed_at.is_none_or(|t| approved.get() < t.get())
                    || s.roots.len() != 3
                    || s.lease_until
                        .is_none_or(|l| l.get() > write || l.get() < approved.get())
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
                let CustodyIntentV1::TestSession {
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
                        write,
                        *request_limit,
                        *transport_limit,
                        *business_limit,
                    ),
                    (
                        SessionPurposeV1::ReconcileRead,
                        read,
                        read_budget.requests,
                        read_budget.transport,
                        read_budget.business,
                    ),
                    (
                        SessionPurposeV1::TerminationPlan,
                        cleanup,
                        cleanup_budget.requests,
                        cleanup_budget.transport,
                        cleanup_budget.business,
                    ),
                ] {
                    let root = s.roots.get(&purpose).ok_or_else(invalid)?;
                    if root.purpose != purpose
                        || root.deadline.get() != deadline
                        || (purpose == SessionPurposeV1::TerminationPlan
                            && root.credential.is_some())
                    {
                        return Err(invalid());
                    }
                    credential_history::validate(s, root, purpose, approved, deadline, id)?;
                    let mut expected = OperationBudgets {
                        requests: BudgetLedger::new(requests.0),
                        transport: BudgetLedger::new(transport.0),
                        business: BudgetLedger::new(business.0),
                    };
                    for (action_id, a) in s.actions.iter().filter(|(_, a)| a.purpose == purpose) {
                        if action_id != &a.request_id
                            || a.request.request_id != *action_id
                            || a.request.session_id != *id
                            || a.request.digest()? != a.body_digest
                            || !hex64(&a.body_digest)
                            || a.admitted_at.get() < approved.get()
                            || a.admitted_at.get() >= deadline
                            || a.transport_attempts.iter().any(|a| !id_nonempty(a))
                            || a.transport_attempts.iter().collect::<BTreeSet<_>>().len()
                                != a.transport_attempts.len()
                        {
                            return Err(invalid());
                        }
                        expected.requests.charge(1).map_err(|_| invalid())?;
                        expected
                            .business
                            .reserve(action_id, a.business.0)
                            .map_err(|_| invalid())?;
                        expected
                            .transport
                            .charge(a.transport_attempts.len() as u64)
                            .map_err(|_| invalid())?;
                    }
                    let sources = s
                        .source_funding
                        .values()
                        .filter(|f| f.purpose() == Some(purpose))
                        .count() as u64;
                    let extra = s
                        .purpose_transports
                        .values()
                        .filter(|p| **p == purpose)
                        .count() as u64;
                    expected
                        .transport
                        .charge(
                            sources
                                .checked_add(extra)
                                .and_then(|total| {
                                    total.checked_add(
                                        s.purpose_http_admissions
                                            .values()
                                            .filter(|a| a.purpose == purpose)
                                            .count() as u64,
                                    )
                                })
                                .ok_or_else(invalid)?,
                        )
                        .map_err(|_| invalid())?;
                    if purpose == SessionPurposeV1::TerminationPlan {
                        for (kind, o) in &s.termination_slots {
                            if o.allocated_at.get() < approved.get()
                                || o.allocated_at.get() >= cleanup
                                || o.operation_id
                                    != if *kind == TerminationOperationV1::Stop {
                                        s.termination.stop_operation_id.clone()
                                    } else {
                                        s.termination.revoke_operation_id.clone()
                                    }
                                || o.transport_attempts.iter().collect::<BTreeSet<_>>().len()
                                    != o.transport_attempts.len()
                                || o.claimed_at.is_some() && o.signed.is_none()
                                || o.lookup_claimed_at.is_some() && o.claimed_at.is_none()
                            {
                                return Err(invalid());
                            }
                            match (&o.payload, kind) {
                                (TerminationPayloadV1::Stop(r), TerminationOperationV1::Stop) => {
                                    r.validate().map_err(|_| invalid())?;
                                    if r.client_operation_id != o.operation_id
                                        || r.operation != GameExecutorOperationV1::Stop
                                        || r.fence != s.termination.initial_fence
                                    {
                                        return Err(invalid());
                                    }
                                }
                                (
                                    TerminationPayloadV1::Revoke(r),
                                    TerminationOperationV1::Revoke,
                                ) => {
                                    let mut original = s.termination.original_delegation.clone();
                                    original.generation = r.generation;
                                    original.nonce = r.nonce;
                                    original.revoked = true;
                                    if r != &original
                                        || r.generation
                                            <= s.termination.original_delegation.generation
                                        || r.nonce <= s.termination.original_delegation.nonce
                                    {
                                        return Err(invalid());
                                    }
                                }
                                _ => return Err(invalid()),
                            }
                            if let Some(original) = &o.signed {
                                self.validate_termination_signed(s, o, original)?;
                                validate_owner_setup_signature_fact(
                                    OwnerSetupSignatureFact::Original(
                                        original,
                                        &s.termination.initial_fence.owner_public_key,
                                    ),
                                )?;
                            }
                            for (source, result) in &o.observations {
                                if result.canonical_fact().is_none_or(|f| f.source != *source) {
                                    return Err(invalid());
                                }
                                self.validate_termination_result(s, o, result)?;
                            }
                            if let Some(result) = &o.result {
                                if !result.terminal()
                                    || !o.observations.values().any(|v| v == result.as_ref())
                                {
                                    return Err(invalid());
                                }
                            }
                            expected.requests.charge(1).map_err(|_| invalid())?;
                            expected
                                .transport
                                .charge(o.transport_attempts.len() as u64)
                                .map_err(|_| invalid())?;
                        }
                    }
                    if expected != root.budgets {
                        return Err(invalid());
                    }
                }
            }
            self.validate_session_control_persistence(s)?;
            if s.source_funding.len() != s.sources.len()
                || s.source_funding.keys().any(|k| !s.sources.contains_key(k))
                || s.purpose_transports.iter().any(|(id, _)| !id_nonempty(id))
            {
                return Err(invalid());
            }
            if s.source_oracle_reads.len() != s.sources.len()
                || s.source_oracle_reads
                    .iter()
                    .any(|(id, n)| *n > 8 || !s.sources.contains_key(id))
            {
                return Err(invalid());
            }
            if s.sources.len() > 160 {
                return Err(invalid());
            }
            for (key, source) in &s.sources {
                if s.consumed_at.is_none()
                    || key != &source.operation_id
                    || !id_nonempty(key)
                    || !hex64(&source.verify_nonce)
                    || source.created_at.0 < s.consumed_at.ok_or_else(invalid)?.get()
                    || source.expires_at.0
                        > source
                            .created_at
                            .0
                            .checked_add(30_000)
                            .ok_or_else(invalid)?
                    || source.expires_at.0 > read.max(cleanup)
                    || source.expires_at.0 <= source.created_at.0
                    || source.claimed_at.is_some() != source.request_bytes.is_some()
                    || source.verification.is_some() != source.verified_at.is_some()
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
            if s.challenges.len() > 1024 {
                return Err(invalid());
            }
            let mut nonces = BTreeSet::new();
            for (key, c) in &s.challenges {
                if key != &c.request_id
                    || !hex64(&c.body_digest)
                    || c.purpose == SessionPurposeV1::TerminationPlan
                    || !nonces.insert(c.nonce.clone())
                    || c.kind != SessionChallengeKindV1::CredentialClaim
                        && c.credential_generation.0 == 0
                    || c.created_at.get() < s.approved_at.ok_or_else(invalid)?.get()
                    || c.expires_at.get() <= c.created_at.get()
                    || c.expires_at.get()
                        > c.created_at.get().checked_add(30_000).ok_or_else(invalid)?
                    || c.expires_at.get()
                        > s.roots.get(&c.purpose).ok_or_else(invalid)?.deadline.get()
                {
                    return Err(invalid());
                }
            }
            for (key, c) in &s.controls {
                if key != &c.request_id
                    || !id_nonempty(key)
                    || c.acknowledged_at.get() < s.approved_at.ok_or_else(invalid)?.get()
                    || c.lease_until.get() > write
                {
                    return Err(invalid());
                }
            }
        }
        for invite in self.pairing_invites.values() {
            if let Some(id) = &invite.consumed_session {
                if !self.test_sessions.get(id).is_some_and(|s| {
                    s.approved_at.is_some()
                        && s.enrollment.registration.request.target.invite_id
                            == invite.original.invite_id
                }) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
    fn validate_session_control_persistence(&self, s: &TestSessionAuthorityV1) -> Result<()> {
        let CustodyIntentV1::TestSession { control_budget, .. } = &s.input.request.intent else {
            return Err(invalid());
        };
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
                    >= admission::pairing(&s.input.request.intent)?
                        .3
                        .max(admission::pairing(&s.input.request.intent)?.4)
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
fn same_proof(a: &ApprovalAdmissionInput, b: &ApprovalAdmissionInput) -> bool {
    let (a, b) = (&a.approval.proof, &b.approval.proof);
    a.nonce == b.nonce
        || (a.identity_namespace == b.identity_namespace
            && a.identity_instance_epoch == b.identity_instance_epoch
            && (a.approval_id == b.approval_id || a.approval_request_id == b.approval_request_id))
}
fn id_nonempty(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}

#[path = "test_session_credential_restore.rs"]
mod credential_history;
