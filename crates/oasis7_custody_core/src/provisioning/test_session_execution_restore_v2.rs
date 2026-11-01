//! Bidirectional pending/LF book validation. Future SDK stages stay rejected.
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn execution_request_count(&self) -> u64 {
        1
    }
    pub(in crate::provisioning) fn owns_lifetime_transport(&self, id: &str) -> bool {
        self.lifetime_transport_id == id
    }
    pub(in crate::provisioning) fn consumed_nonce_is_exact(
        &self,
        request: &str,
        nonce: &str,
    ) -> bool {
        self.action_request_id == request && self.challenge_request_id == nonce
    }
    pub(in crate::provisioning) fn archived_lifetime(
        &self,
    ) -> Option<&crate::native_binding::SignedManagedNativeSessionLifetimeV1> {
        self.archived_lifetime.as_ref()
    }
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_execution_book_v2(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        let Some(b) = s.execution_book.as_ref() else {
            return if s.purpose_transports.values().any(|t| {
                matches!(
                    t,
                    SessionPurposeTransportV1::ExecutionNativeLifetimeV2(_)
                        | SessionPurposeTransportV1::ExecutionFactoryCallbackV2(_)
                        | SessionPurposeTransportV1::ExecutionPrepareCurrentV2(_)
                )
            }) {
                Err(invalid())
            } else {
                Ok(())
            };
        };
        let (plan, scope) = s.header()?;
        let position = match plan.actions.as_slice() {
            [crate::approval::ApprovedActionV1::Move { position_cm }] => position_cm.map(|v| v.0),
            _ => return Err(invalid()),
        };
        let root = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        if b.api_version != 2
            || b.session_id != s.input.operation_id
            || b.plan_digest != plan.digest().map_err(|_| invalid())?
            || b.approved_input_digest != s.input_digest()?
            || b.session_intent_digest != s.input.request.intent_digest().map_err(|_| invalid())?
            || b.world != scope.world_identity
            || b.identity_scope != s.session.scope.role_scope
            || b.native_binding_digest
                != s.native_lifetime
                    .as_ref()
                    .ok_or_else(invalid)?
                    .lifetime
                    .status
                    .receipt
                    .digest()
                    .map_err(|_| invalid())?
            || b.execution_deadline != root.deadline
            || b.approved_position_cm != position
            || !plan.selection.is_exact_move0()
            || plan.business_limit.0 != 0
            || !plan.allowed_resource_refs.is_empty()
            || s.approved_at.is_none_or(|at| at.get() > b.started_at.get())
            || b.started_at != b.lifetime_claimed_at
            || b.started_at.get() >= root.deadline.get()
            || !b.context_slots.is_empty()
        {
            return Err(invalid());
        }
        let paid = s
            .purpose_http_admissions
            .get(&b.http_ingress_attempt_id)
            .ok_or_else(invalid)?;
        if paid.purpose != SessionPurposeV1::Execution
            || paid.hop != SessionControlHopV1::Host
            || paid.class != SessionPurposeHttpAdmissionClassV1::GenericOne
            || paid.charged_at.get() > b.started_at.get()
        {
            return Err(invalid());
        }
        let c = s
            .challenges
            .get(&b.challenge_request_id)
            .ok_or_else(invalid)?;
        let original = SessionActionRequestV2 {
            api_version: 2,
            session_id: b.session_id.clone(),
            request_id: b.action_request_id.clone(),
            action: SessionToolActionV1::Execute(plan.actions[0].clone()),
        };
        if b.credential_generation.0 == 0
            || b.credential_generation.0
                > root.credential.as_ref().ok_or_else(invalid)?.generation.0
            || c.credential_generation != b.credential_generation
            || c.kind != SessionChallengeKindV1::ToolMetadata
            || c.purpose != SessionPurposeV1::Execution
            || c.logical_request_id != b.action_request_id
            || c.body_digest != original.digest()?
            || c.consumed_by.as_deref() != Some(b.action_request_id.as_str())
            || c.created_at.get() > b.started_at.get()
            || b.started_at.get() >= c.expires_at.get()
        {
            return Err(invalid());
        }
        source_binding::check(
            s,
            &b.source_operation_id,
            &b.http_ingress_attempt_id,
            &paid.signed_attempt_digest,
            &original.digest()?,
        )?;
        let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
        let v = &source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        if source.api_version != 2
            || source.approved_input_digest != s.input_digest()?
            || s.source_oracle_reads
                .get(&b.source_operation_id)
                .is_none_or(|n| *n < 3)
            || v.observed_at.0 > b.started_at.get()
            || b.started_at.get() - v.observed_at.0 > 5000
            || b.started_at.get() >= source.source.expires_at.0
            || b.lifetime_transport_id
                != start::lifetime_transport(s, &b.action_request_id, &b.source_operation_id)?
        {
            return Err(invalid());
        }
        let marker = SessionPurposeTransportV1::ExecutionNativeLifetimeV2(Box::new(
            ExecutionNativeLifetimeTransportV2 {
                kind: ExecutionNativeLifetimeTransportKindV2::ExecutionNativeLifetimeV2,
                request_id: b.action_request_id.clone(),
                source_operation_id: b.source_operation_id.clone(),
                approved_input_digest: b.approved_input_digest.clone(),
                claimed_at: b.lifetime_claimed_at,
            },
        ));
        if s.purpose_transports.get(&b.lifetime_transport_id) != Some(&marker)
            || s.purpose_transports.iter().any(|(id, t)| {
                matches!(t, SessionPurposeTransportV1::ExecutionNativeLifetimeV2(_))
                    && id != &b.lifetime_transport_id
            })
        {
            return Err(invalid());
        }
        match (&b.archived_lifetime, b.lifetime_archived_at) {
            (None, None) if b.lifetime_admitted_at.is_none() => {}
            (Some(fact), Some(at)) => {
                lifetime::fact_shape(self, s, b, fact)?;
                if let Some(admitted) = b.lifetime_admitted_at {
                    fact.lifetime
                        .validate_for(
                            &s.native_lifetime
                                .as_ref()
                                .ok_or_else(invalid)?
                                .lifetime
                                .status
                                .receipt,
                            &self.test_session_v2_lifetime_nonce(
                                &b.session_id,
                                &b.source_operation_id,
                            )?,
                            s.session.native_binding_generation,
                            s.input.request.context.service_epoch,
                            admitted.get(),
                        )
                        .map_err(|_| invalid())?;
                    fact.lifetime.covers(root.deadline).map_err(|_| invalid())?;
                    if admitted.get() < at.get()
                        || admitted.get() >= root.deadline.get()
                        || admitted.get() < fact.lifetime.status.observed_at.0
                        || admitted.get() - fact.lifetime.status.observed_at.0 > 5000
                        || admitted.get() < v.observed_at.0
                        || admitted.get() - v.observed_at.0 > 5000
                        || admitted.get() >= source.source.expires_at.0
                        || s.source_oracle_reads
                            .get(&b.source_operation_id)
                            .is_none_or(|n| *n < 4)
                    {
                        return Err(invalid());
                    }
                }
            }
            _ => return Err(invalid()),
        }
        self.validate_execution_prepare_v2(s, b)?;
        Ok(())
    }
}
