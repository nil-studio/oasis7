//! First actual execution reservation. Its only result is a paid LF challenge.
//! No SDK write original, dispatch descriptor, or private body is returned.
use super::super::test_session::{
    SessionEventKindV2, append_session_event_v2, authenticated_receipt_v2,
};
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
pub(super) fn lifetime_transport(
    s: &TestSessionAuthorityV2,
    request: &str,
    source: &str,
) -> Result<String> {
    Ok(format!(
        "execution-v2-lifetime-{}",
        digest(&(
            "oasis7.custody.execution-lifetime-transport.v2",
            s.input_digest()?,
            request,
            source,
        ))?
    ))
}
impl AccountProvisionApprovalState {
    pub fn claim_test_session_v2_execution_lifetime(
        &mut self,
        session_id: &str,
        source_id: &str,
        proof: SessionProofRequest<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<String> {
        let q = super::super::test_session::action_original_v2(&proof)?;
        if q.session_id != session_id || q.purpose() != SessionPurposeV1::Execution {
            return Err(invalid());
        }
        let s = self.test_session_v2_local(session_id, SessionPurposeV1::Execution, now)?;
        authenticated_receipt_v2(s, SessionPurposeV1::Execution, proof.possession, now)?;
        let paid = s
            .purpose_http_admissions
            .get(proof.ingress_attempt_id)
            .ok_or_else(invalid)?;
        if paid.class != SessionPurposeHttpAdmissionClassV1::GenericOne
            || paid.purpose != SessionPurposeV1::Execution
            || paid.hop != SessionControlHopV1::Host
            || paid.signed_attempt_digest != proof.possession.transport_attempt_digest()
            || paid.charged_at.get() > now.get()
            || s.execution_book.is_some()
            || s.source_oracle_reads.get(source_id) != Some(&3)
        {
            return Err(invalid());
        }
        let (plan, scope) = s.header()?;
        let position = match (&q.action, plan.actions.as_slice()) {
            (
                SessionToolActionV1::Execute(crate::approval::ApprovedActionV1::Move {
                    position_cm: a,
                }),
                [crate::approval::ApprovedActionV1::Move { position_cm: b }],
            ) if a == b => a.map(|v| v.0),
            _ => return Err(invalid()),
        };
        if !plan.selection.is_exact_move0()
            || plan.business_limit.0 != 0
            || !plan.allowed_resource_refs.is_empty()
        {
            return Err(invalid());
        }
        let nonce = proof.possession.nonce().ok_or_else(invalid)?;
        let (challenge_id, challenge) = s
            .challenges
            .iter()
            .find(|(_, c)| c.nonce == nonce)
            .ok_or_else(invalid)?;
        if challenge.kind != SessionChallengeKindV1::ToolMetadata
            || challenge.logical_request_id != q.request_id
            || challenge.purpose != SessionPurposeV1::Execution
            || challenge.body_digest != q.digest()?
            || challenge.consumed_by.is_some()
            || now.get() >= challenge.expires_at.get()
        {
            return Err(invalid());
        }
        source_binding::check(
            s,
            source_id,
            proof.ingress_attempt_id,
            &proof.possession.transport_attempt_digest(),
            &q.digest()?,
        )?;
        let source = s.sources.get(source_id).ok_or_else(invalid)?;
        verifier.verify(&self.test_session_v2_source_checks(
            session_id,
            source_id,
            source.source.verification.as_ref().ok_or_else(invalid)?,
            current,
            now,
        )?)?;
        let attempt = lifetime_transport(s, &q.request_id, source_id)?;
        self.test_session_v2_transport_id_available(session_id, &attempt)?;
        let lf_nonce = self.test_session_v2_lifetime_nonce(session_id, source_id)?;
        let book = SessionExecutionBookV2 {
            api_version: 2,
            session_id: session_id.into(),
            plan_digest: plan.digest().map_err(|_| invalid())?,
            approved_input_digest: s.input_digest()?,
            session_intent_digest: s.input.request.intent_digest().map_err(|_| invalid())?,
            world: scope.world_identity.clone(),
            identity_scope: s.session.scope.role_scope.clone(),
            native_binding_digest: s
                .native_lifetime
                .as_ref()
                .ok_or_else(invalid)?
                .lifetime
                .status
                .receipt
                .digest()
                .map_err(|_| invalid())?,
            execution_deadline: s
                .roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?
                .deadline,
            action_request_id: q.request_id.clone(),
            credential_generation: s
                .roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?
                .credential
                .as_ref()
                .ok_or_else(invalid)?
                .generation,
            approved_position_cm: position,
            started_at: now,
            challenge_request_id: challenge_id.clone(),
            http_ingress_attempt_id: proof.ingress_attempt_id.into(),
            source_operation_id: source_id.into(),
            lifetime_transport_id: attempt.clone(),
            lifetime_claimed_at: now,
            archived_lifetime: None,
            lifetime_archived_at: None,
            lifetime_admitted_at: None,
            last_allocation_sequence: DecimalU64(0),
            stages: BTreeMap::new(),
            context_slots: BTreeMap::new(),
        };
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(session_id)?;
        let root = saved
            .roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        root.budgets.requests.charge(1).map_err(|_| invalid())?;
        root.budgets.transport.charge(1).map_err(|_| invalid())?;
        saved.purpose_transports.insert(
            attempt,
            SessionPurposeTransportV1::ExecutionNativeLifetimeV2(Box::new(
                ExecutionNativeLifetimeTransportV2 {
                    kind: ExecutionNativeLifetimeTransportKindV2::ExecutionNativeLifetimeV2,
                    request_id: q.request_id.clone(),
                    source_operation_id: source_id.into(),
                    approved_input_digest: s.input_digest()?,
                    claimed_at: now,
                },
            )),
        );
        saved
            .challenges
            .get_mut(challenge_id)
            .ok_or_else(invalid)?
            .consumed_by = Some(q.request_id.clone());
        saved.execution_book = Some(book);
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionBookStarted {
                request_id: q.request_id.clone(),
            },
            now,
        )?;
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionLifetimeClaimed {
                request_id: q.request_id,
                source_operation_id: source_id.into(),
            },
            now,
        )?;
        if saved
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
            && saved.closing.is_none()
        {
            saved.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(lf_nonce)
    }
}
