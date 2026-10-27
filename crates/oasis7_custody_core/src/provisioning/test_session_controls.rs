use super::*;
impl AccountProvisionApprovalState {
    pub fn charge_test_session_control_http_attempt(
        &mut self,
        id: &str,
        proof: &HttpPossessionReceipt,
        now: UnixMillis,
    ) -> Result<()> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        if matches!(
            proof.profile(),
            HttpPopProfileV1::ClaimChallenge | HttpPopProfileV1::CredentialClaim
        ) {
            roots::public_receipt(s, proof, now)?;
            if s.approved_at.is_none()
                || !s.roots.iter().any(|(p, r)| {
                    *p != SessionPurposeV1::TerminationPlan
                        && !r.revoked
                        && now.get() < r.deadline.get()
                })
            {
                return Err(invalid());
            }
        } else if roots::authenticated_receipt(s, SessionPurposeV1::Execution, proof, now).is_err()
        {
            roots::authenticated_receipt(s, SessionPurposeV1::ReconcileRead, proof, now)?;
        }
        self.charge_session_control_attempt(
            id,
            &proof.transport_attempt_digest(),
            SessionControlHopV1::Host,
            now,
        )
    }
    pub fn charge_test_session_internal_control_transport(
        &mut self,
        id: &str,
        attempt_id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        if s.input.request.context != current.approval.authority
            || current.approval.frozen
            || current
                .approval
                .fence
                .check_epoch(current.approval.authority.service_epoch.0)
                .is_err()
        {
            return Err(invalid());
        }
        self.charge_session_control_attempt(id, attempt_id, SessionControlHopV1::Identity, now)
    }
    pub fn control_test_session(
        &mut self,
        proof: SessionProofRequest<'_>,
        current: Option<&TestSessionLiveCurrent<'_, '_>>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionControlAckV1> {
        let request: SessionControlRequestV1 = original(&proof)?;
        self.require_session_http_paid(&request.session_id, proof.possession, None)?;
        let s = self
            .test_sessions
            .get(&request.session_id)
            .ok_or_else(invalid)?;
        roots::public_receipt(s, proof.possession, now)?;
        if request.api_version != 1
            || !id(&request.request_id)
            || proof.possession.request_id() != request.request_id
            || !matches!(
                proof.possession.profile(),
                HttpPopProfileV1::Custody | HttpPopProfileV1::Mcp
            )
        {
            return Err(invalid());
        }
        if let Some(old) = s.controls.get(&request.request_id) {
            return if old.kind == request.kind {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let purpose =
            if roots::authenticated_receipt(s, SessionPurposeV1::Execution, proof.possession, now)
                .is_ok()
            {
                SessionPurposeV1::Execution
            } else {
                roots::authenticated_receipt(
                    s,
                    SessionPurposeV1::ReconcileRead,
                    proof.possession,
                    now,
                )?;
                SessionPurposeV1::ReconcileRead
            };
        let nonce = proof.possession.nonce().ok_or_else(invalid)?;
        let body = digest(&("oasis7.custody.test-session-control-request.v1", &request))?;
        let challenge = s
            .challenges
            .values()
            .find(|c| c.nonce == nonce)
            .ok_or_else(invalid)?;
        if challenge.purpose != purpose
            || challenge.kind != SessionChallengeKindV1::Control(request.kind)
            || challenge.logical_request_id != request.request_id
            || challenge.credential_generation
                != s.roots
                    .get(&purpose)
                    .and_then(|r| r.credential.as_ref())
                    .ok_or_else(invalid)?
                    .generation
            || challenge.body_digest != body
            || challenge.consumed_by.is_some()
            || now.get() >= challenge.expires_at.get()
        {
            return Err(invalid());
        }
        if matches!(
            request.kind,
            SessionControlKindV1::Initialize
                | SessionControlKindV1::ToolsList
                | SessionControlKindV1::Heartbeat
        ) {
            let c = current.ok_or_else(invalid)?;
            self.require_session_source_funding(
                &request.session_id,
                c.source_operation_id,
                SessionSourceFundingV1::Control,
            )?;
        }
        if matches!(
            request.kind,
            SessionControlKindV1::Initialize | SessionControlKindV1::ToolsList
        ) {
            let current = current.ok_or_else(invalid)?;
            verifier.verify(&self.test_session_live_checks(
                &request.session_id,
                purpose,
                current,
                now,
            )?)?;
        }
        if request.kind == SessionControlKindV1::Heartbeat {
            let current = current.ok_or_else(invalid)?;
            verifier.verify(&self.test_session_live_checks(
                &request.session_id,
                SessionPurposeV1::Execution,
                current,
                now,
            )?)?;
            if s.last_heartbeat
                .is_some_and(|t| now.get() < t.get().saturating_add(30_000))
            {
                return Err(invalid());
            }
        }
        let mut next = self.clone();
        let s = next
            .test_sessions
            .get_mut(&request.session_id)
            .ok_or_else(invalid)?;
        reserve_control_logical(s, &request.request_id, &body)?;
        s.challenges
            .get_mut(&challenge.request_id)
            .ok_or_else(invalid)?
            .consumed_by = Some(request.request_id.clone());
        if request.kind == SessionControlKindV1::Heartbeat {
            let deadline = s
                .roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?
                .deadline;
            s.lease_until = Some(UnixMillis::new(
                now.get()
                    .checked_add(90_000)
                    .ok_or_else(invalid)?
                    .min(deadline.get()),
            ));
            s.last_heartbeat = Some(now)
        } else if matches!(
            request.kind,
            SessionControlKindV1::Finish | SessionControlKindV1::Abort
        ) && s.closing.is_none()
        {
            s.closing = Some((
                if request.kind == SessionControlKindV1::Finish {
                    TestSessionClosingReasonV1::Finished
                } else {
                    TestSessionClosingReasonV1::Aborted
                },
                now,
            ));
        }
        let ack = SessionControlAckV1 {
            request_id: request.request_id.clone(),
            kind: request.kind,
            acknowledged_at: now,
            lease_until: s.lease_until.ok_or_else(invalid)?,
        };
        s.controls.insert(request.request_id, ack.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(ack)
    }
    pub fn expire_test_sessions(&mut self, now: UnixMillis) -> Result<Vec<String>> {
        let mut next = self.clone();
        let mut closed = Vec::new();
        for (id, s) in &mut next.test_sessions {
            if s.closing.is_some() {
                continue;
            }
            if s.approved_at.is_none() {
                let (_, _, write, _, _) = admission::pairing(&s.input.request.intent)?;
                if session_control_exhausted(s)
                    || now.get() >= write
                    || s.consumed_at.is_none() && now.get() >= s.input.approval.proof.expires_at.0
                {
                    s.closing = Some((TestSessionClosingReasonV1::Deadline, now));
                    closed.push(id.clone())
                }
                continue;
            }
            let root = s
                .roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?;
            let reason = if session_control_exhausted(s) {
                Some(TestSessionClosingReasonV1::BudgetExhausted)
            } else if now.get() >= root.deadline.get() {
                Some(TestSessionClosingReasonV1::Deadline)
            } else if s.lease_until.is_some_and(|v| now.get() >= v.get()) {
                Some(TestSessionClosingReasonV1::LeaseExpired)
            } else {
                None
            };
            if let Some(reason) = reason {
                s.closing = Some((reason, now));
                closed.push(id.clone())
            }
        }
        next.validate_persistence()?;
        *self = next;
        Ok(closed)
    }
    pub fn revoke_test_session(&mut self, id: &str, now: UnixMillis) -> Result<()> {
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        for r in s.roots.values_mut() {
            r.revoked = true
        }
        if s.closing.is_none() {
            s.closing = Some((TestSessionClosingReasonV1::Revoked, now))
        }
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
fn id_nonempty(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}

pub(in crate::provisioning) fn reserve_control_logical(
    s: &mut TestSessionAuthorityV1,
    request_id: &str,
    body: &str,
) -> Result<()> {
    if !id_nonempty(request_id) || !hex64(body) {
        return Err(invalid());
    }
    if let Some(old) = s.control_logicals.get(request_id) {
        return if old == body {
            Ok(())
        } else {
            Err(ProvisionApprovalError::Conflict)
        };
    }
    s.control
        .as_mut()
        .ok_or_else(invalid)?
        .requests
        .reserve(request_id, 1)
        .map_err(|_| invalid())?;
    s.control_logicals.insert(request_id.into(), body.into());
    Ok(())
}
impl SessionToolRequestV1 {
    pub fn digest(&self) -> Result<String> {
        digest(&("oasis7.custody.test-session-tool-request.v1", self))
    }
}
impl SessionControlRequestV1 {
    pub fn digest(&self) -> Result<String> {
        digest(&("oasis7.custody.test-session-control-request.v1", self))
    }
}
