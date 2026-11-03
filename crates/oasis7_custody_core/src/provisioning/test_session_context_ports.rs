use super::*;
impl AccountProvisionApprovalState {
    /// Persists the exact candidate digest and paid once-claim before a fixed actor PUT.
    /// No envelope is retained in Account and no body-use authority is returned.
    pub fn claim_test_session_context_write(
        &mut self,
        id: &str,
        input: ContextBodyWrite<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ContextBodyReferenceV1> {
        let ContextBodyWrite {
            attempt,
            body,
            candidate,
        } = input;
        verifier.verify(&self.context_complete_checks(id, body, current, now)?)?;
        candidate.validate_for(body.scope()).map_err(body_error)?;
        if candidate.envelope.ciphertext.len() / 2
            != body
                .canonical_bytes()
                .len()
                .checked_add(crate::envelope::AES_GCM_TAG_BYTES)
                .ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let c = s.context.as_ref().ok_or_else(invalid)?;
        if c.slots.contains_key(&body.scope().slot) {
            return Err(ProvisionApprovalError::Conflict);
        }
        if match body.scope().slot {
            ContextBodySlotKindV1::Prepared => c.prepared_source.as_str(),
            ContextBodySlotKindV1::Captured => c.captured_source.as_deref().ok_or_else(invalid)?,
        } != current.source_operation_id
        {
            return Err(invalid());
        }
        let reference = ContextBodyReferenceV1 {
            scope: body.scope().clone(),
            evidence: body.evidence().clone(),
            plaintext_digest: crate::approval::bytes_digest(body.canonical_bytes()),
            plaintext_bytes: DecimalU64(body.canonical_bytes().len() as u64),
            envelope_digest: candidate.envelope_digest().map_err(body_error)?,
            record_wire_bytes: DecimalU64(candidate.wire_bytes().map_err(body_error)?.len() as u64),
            write_attempt: attempt.into(),
        };
        let mut next = self.clone();
        next.context_port(
            id,
            attempt,
            ContextBodyAttemptV1 {
                slot: body.scope().slot,
                port: ContextBodyPortV1::Write,
                purpose: SessionPurposeV1::Execution,
                source_operation_id: current.source_operation_id.into(),
                claimed_at: now,
                observation: None,
            },
            &reference,
        )?;
        next.test_sessions
            .get_mut(id)
            .and_then(|s| s.context.as_mut())
            .ok_or_else(invalid)?
            .slots
            .insert(body.scope().slot, reference.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(reference)
    }
    /// Fixed full-body reads are Execution-only. Expired contexts never yield bytes.
    pub fn claim_test_session_context_read(
        &mut self,
        id: &str,
        input: ContextBodyRead<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ContextBodyReferenceV1> {
        let ContextBodyRead {
            slot,
            port,
            attempt,
        } = input;
        let purpose = if port == ContextBodyPortV1::RecoverMetadata {
            SessionPurposeV1::ReconcileRead
        } else {
            SessionPurposeV1::Execution
        };
        if port == ContextBodyPortV1::Write {
            return Err(invalid());
        }
        verifier.verify(&self.test_session_context_read_checks(id, slot, port, current, now)?)?;
        let reference = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .and_then(|c| c.slots.get(&slot))
            .ok_or_else(invalid)?
            .clone();
        let mut next = self.clone();
        next.context_port(
            id,
            attempt,
            ContextBodyAttemptV1 {
                slot,
                port,
                purpose,
                source_operation_id: current.source_operation_id.into(),
                claimed_at: now,
                observation: None,
            },
            &reference,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(reference)
    }
    fn context_port(
        &mut self,
        id: &str,
        attempt: &str,
        record: ContextBodyAttemptV1,
        reference: &ContextBodyReferenceV1,
    ) -> Result<()> {
        let ContextBodyAttemptV1 {
            slot,
            port,
            purpose,
            claimed_at: now,
            ..
        } = record;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let c = s.context.as_ref().ok_or_else(invalid)?;
        if !context_attempt_id(attempt)
            || Self::session_transport_attempt_occupied(s, attempt)
            || c.original.request.client_operation_id == attempt
            || c.attempts.values().any(|a| {
                a.slot == slot
                    && a.port == port
                    && matches!(
                        port,
                        ContextBodyPortV1::Write
                            | ContextBodyPortV1::ReadBefore
                            | ContextBodyPortV1::ReadAfter
                    )
            })
            || c.attempts.len() >= 700
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let transport = context_transport(c, &record, reference)?;
        let s = self.test_sessions.get_mut(id).ok_or_else(invalid)?;
        s.roots
            .get_mut(&purpose)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        s.purpose_transports.insert(attempt.into(), transport);
        s.context
            .as_mut()
            .ok_or_else(invalid)?
            .attempts
            .insert(attempt.into(), record);
        if s.closing.is_none() && session_purpose_transport_exhausted(s) {
            s.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        Ok(())
    }
    /// Historical opaque storage observation only. A caller echo cannot create a
    /// context-use grant: later dependencies must reopen and verify the full body.
    pub fn record_test_session_context_storage_fact(
        &mut self,
        id: &str,
        attempt: &str,
        fact: ContextBodyStorageFactV1,
    ) -> Result<ContextBodyStorageFactV1> {
        fact.validate().map_err(body_error)?;
        let c = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .ok_or_else(invalid)?;
        let a = c.attempts.get(attempt).ok_or_else(invalid)?;
        let r = c.slots.get(&a.slot).ok_or_else(invalid)?;
        if fact.scope != r.scope
            || fact.stored.as_ref().is_some_and(|v| {
                v.envelope_digest != r.envelope_digest || v.record_wire_bytes != r.record_wire_bytes
            })
        {
            return Err(invalid());
        }
        if let Some(old) = &a.observation {
            return if old == &fact {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        next.test_sessions
            .get_mut(id)
            .and_then(|s| s.context.as_mut())
            .and_then(|c| c.attempts.get_mut(attempt))
            .ok_or_else(invalid)?
            .observation = Some(fact.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(fact)
    }
    /// Must be called after an actual private body read. The returned opaque
    /// object is evidence for storage only, not permission to expose/use context.
    pub fn reopen_test_session_context_body(
        &self,
        id: &str,
        attempt: &str,
        bytes: &[u8],
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ValidatedContextBody> {
        let c = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .ok_or_else(invalid)?;
        let a = c.attempts.get(attempt).ok_or_else(invalid)?;
        let r = c.slots.get(&a.slot).ok_or_else(invalid)?;
        if a.port == ContextBodyPortV1::Write
            || a.port == ContextBodyPortV1::RecoverMetadata
            || a.purpose != SessionPurposeV1::Execution
            || a.source_operation_id != current.source_operation_id
            || bytes.len() as u64 != r.plaintext_bytes.0
            || crate::approval::bytes_digest(bytes) != r.plaintext_digest
        {
            return Err(invalid());
        }
        let body = ValidatedContextBody::revalidate(r.scope.clone(), &r.evidence, bytes)
            .map_err(body_error)?;
        verifier.verify(&self.context_complete_checks(id, &body, current, now)?)?;
        Ok(body)
    }
}
