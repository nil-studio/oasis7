use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn context_attempt_occupied(
        s: &TestSessionAuthorityV1,
        attempt: &str,
    ) -> bool {
        s.context.as_ref().is_some_and(|c| {
            c.original.request.client_operation_id == attempt
                || c.attempts.contains_key(attempt)
                || c.live_reads.contains_key(attempt)
                || s.purpose_transports
                    .get(attempt)
                    .is_some_and(|v| matches!(v, SessionPurposeTransportV1::LiveContextRead(_)))
        })
    }
    pub(in crate::provisioning) fn validate_session_context_persistence(
        &self,
        id: &str,
        s: &TestSessionAuthorityV1,
    ) -> Result<()> {
        if s.purpose_transports.values().any(|v| {
            matches!(
                v,
                SessionPurposeTransportV1::ExecutionNativeLifetimeV2(_)
                    | SessionPurposeTransportV1::ExecutionFactoryCallbackV2(_)
                    | SessionPurposeTransportV1::ExecutionPrepareCurrentV2(_)
                    | SessionPurposeTransportV1::ExecutionPrepareRkvV2(_)
            )
        }) {
            return Err(invalid());
        }
        let Some(c) = &s.context else {
            return if s.purpose_transports.values().any(|v| {
                matches!(
                    v,
                    SessionPurposeTransportV1::ContextBody(_)
                        | SessionPurposeTransportV1::LiveContextRead(_)
                )
            }) {
                Err(invalid())
            } else {
                Ok(())
            };
        };
        tool_nonce::native_registration(s)?;
        verify_prepare(&c.original).map_err(|_| invalid())?;
        let approved = s.approved_at.ok_or_else(invalid)?;
        let deadline = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .deadline;
        let op = &c.original.request.client_operation_id;
        if c.original.request.fence != s.termination.initial_fence
            || c.original.subject_public_key
                != s.termination
                    .initial_fence
                    .executor_binding
                    .as_ref()
                    .ok_or_else(invalid)?
                    .executor_public_key
            || c.original.request.authority_mode
                != Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1)
            || c.registered_at.get() < approved.get()
            || c.registered_at.get() >= deadline.get()
            || s.source_funding.get(&c.prepared_source) != Some(&SessionSourceFundingV1::Execution)
            || s.sources
                .get(&c.prepared_source)
                .is_none_or(|v| v.verification.is_none())
            || s.sources.contains_key(op)
            || s.purpose_transports.contains_key(op)
            || s.purpose_http_admissions.contains_key(op)
            || s.control_transports.contains_key(op)
            || s.client_http_admissions.contains_key(op)
            || s.actions.contains_key(op)
            || s.actions
                .values()
                .any(|a| a.transport_attempts.contains(op))
            || Self::termination_attempt_occupied(s, op)
            || c.slots.len() > 2
            || c.attempts.len() > 700
        {
            return Err(invalid());
        }
        match (&c.get, &c.prefix, &c.captured_source) {
            (None, None, None) => {}
            (Some(get), Some(prefix), Some(source)) => {
                verify_get(get).map_err(|_| invalid())?;
                if get.request.prepare_operation_id != *op
                    || get.subject_public_key != c.original.subject_public_key
                    || get.request.context.fence != c.original.request.fence
                    || source == &c.prepared_source
                    || s.source_funding.get(source) != Some(&SessionSourceFundingV1::Execution)
                    || s.sources
                        .get(source)
                        .is_none_or(|v| v.verification.is_none())
                    || !c.slots.contains_key(&ContextBodySlotKindV1::Prepared)
                {
                    return Err(invalid());
                }
                prefix
                    .decode()
                    .map_err(body_error)?
                    .validate(
                        &self.context_scope(id, ContextBodySlotKindV1::Captured)?,
                        &get.request.context,
                    )
                    .map_err(body_error)?;
            }
            _ => return Err(invalid()),
        }
        let mut positions = BTreeSet::new();
        for (attempt, a) in &c.attempts {
            let r = c.slots.get(&a.slot).ok_or_else(invalid)?;
            let expected_purpose = if a.port == ContextBodyPortV1::RecoverMetadata {
                SessionPurposeV1::ReconcileRead
            } else {
                SessionPurposeV1::Execution
            };
            let root = s.roots.get(&expected_purpose).ok_or_else(invalid)?;
            if !context_attempt_id(attempt)
                || attempt == op
                || (matches!(
                    a.port,
                    ContextBodyPortV1::Write
                        | ContextBodyPortV1::ReadBefore
                        | ContextBodyPortV1::ReadAfter
                ) && !positions.insert((a.slot, a.port)))
                || a.purpose != expected_purpose
                || s.purpose_transports.get(attempt) != Some(&context_transport(c, a, r)?)
                || s.source_funding
                    .get(&a.source_operation_id)
                    .and_then(|f| f.purpose())
                    != Some(expected_purpose)
                || s.sources
                    .get(&a.source_operation_id)
                    .is_none_or(|v| v.verification.is_none())
                || a.claimed_at.get() < c.registered_at.get()
                || a.claimed_at.get() >= root.deadline.get()
                || s.sources.contains_key(attempt)
                || s.source_funding.contains_key(attempt)
                || s.purpose_http_admissions.contains_key(attempt)
                || s.control_transports.contains_key(attempt)
                || s.client_http_admissions.contains_key(attempt)
                || Self::termination_attempt_occupied(s, attempt)
                || s.tool_nonce_ingresses
                    .values()
                    .any(|v| v.owner_attempt == *attempt || v.account_attempt == *attempt)
                || s.actions
                    .values()
                    .any(|v| v.transport_attempts.contains(attempt))
            {
                return Err(invalid());
            }
            if a.port == ContextBodyPortV1::Write && &r.write_attempt != attempt {
                return Err(invalid());
            }
            if let Some(f) = &a.observation {
                f.validate_for(&r.scope).map_err(body_error)?;
                if f.stored.as_ref().is_some_and(|v| {
                    v.envelope_digest != r.envelope_digest
                        || v.record_wire_bytes != r.record_wire_bytes
                }) {
                    return Err(invalid());
                }
            }
        }
        for (attempt, charge) in &s.purpose_transports {
            if let SessionPurposeTransportV1::ContextBody(_) = charge {
                let a = c.attempts.get(attempt).ok_or_else(invalid)?;
                let r = c.slots.get(&a.slot).ok_or_else(invalid)?;
                if charge != &context_transport(c, a, r)? {
                    return Err(invalid());
                }
            }
        }
        for (slot, r) in &c.slots {
            r.evidence.validate_metadata(&r.scope).map_err(body_error)?;
            if self.context_scope(id, *slot)? != r.scope
                || r.evidence.prepare() != &c.original
                || r.evidence.slot() != *slot
                || !hex64(&r.plaintext_digest)
                || !hex64(&r.envelope_digest)
                || r.plaintext_bytes.0 == 0
                || r.plaintext_bytes.0 > CONTEXT_BODY_MAX_BYTES as u64
                || r.record_wire_bytes.0 == 0
                || r.record_wire_bytes.0 > CONTEXT_BODY_MAX_WIRE_BYTES as u64
                || !c
                    .attempts
                    .get(&r.write_attempt)
                    .is_some_and(|a| a.slot == *slot && a.port == ContextBodyPortV1::Write)
            {
                return Err(invalid());
            }
            if let ContextBodyEvidenceV1::Captured {
                original, prefix, ..
            } = &r.evidence
            {
                if c.get.as_ref() != Some(original.as_ref()) || c.prefix.as_ref() != Some(prefix) {
                    return Err(invalid());
                }
            }
        }
        self.validate_live_context_persistence(id, s)?;
        Ok(())
    }
}
