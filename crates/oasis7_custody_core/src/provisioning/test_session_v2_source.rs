//! Versioned Source reservations use only the actual V2 Book and original bounds.
use super::*;
fn id_nonempty(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn test_session_v2_local(
        &self,
        id: &str,
        purpose: SessionPurposeV1,
        now: UnixMillis,
    ) -> Result<&TestSessionAuthorityV2> {
        let s = self.test_session_v2_record(id)?;
        if s.native_machine_registration
            .as_ref()
            .is_some_and(|m| !m.ready())
        {
            return Err(invalid());
        }
        let root = s.roots.get(&purpose).ok_or_else(invalid)?;
        if self.session_inventory != SessionInventoryV1::Known
            || self.frozen()
            || s.approved_at.is_none()
            || root.revoked
            || now.get() >= root.deadline.get()
            || s.input.request.context.security_version != self.provision.security_epoch()
            || (purpose == SessionPurposeV1::Execution
                && (s.closing.is_some()
                    || v2_control_exhausted(s)
                    || v2_purpose_exhausted(s)
                    || s.lease_until.is_none_or(|t| now.get() >= t.get())))
        {
            return Err(invalid());
        }
        Ok(s)
    }
    /// Existing dispatch only: no new Source, grant or token is constructed.
    pub(in crate::provisioning) fn test_session_v2_prepaid_prepare_local(
        &self,
        id: &str,
        record: &super::super::execution_book_v2::ExecutionPrepareServiceRecordV2,
        now: UnixMillis,
    ) -> Result<&TestSessionAuthorityV2> {
        let s = self.test_session_v2_record(id)?;
        let root = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        let event = SessionEventKindV2::ExecutionPreparePermitsRecorded {
            request_id: s
                .execution_book
                .as_ref()
                .ok_or_else(invalid)?
                .journal_record()
                .request_id
                .into(),
        };
        if self.session_inventory != SessionInventoryV1::Known
            || self.frozen()
            || s.approved_at.is_none()
            || root.revoked
            || now.get() >= root.deadline.get()
            || s.lease_until.is_none_or(|t| now.get() >= t.get())
            || s.input.request.context.security_version != self.provision.security_epoch()
            || s.native_machine_registration
                .as_ref()
                .is_some_and(|m| !m.ready())
            || v2_control_exhausted(s)
            || s.roots
                .get(&SessionPurposeV1::ReconcileRead)
                .is_some_and(|r| r.budgets.transport.remaining() == Ok(0))
            || session_event_ordinal_v2(s, &event)? != Some(record.accepted_ordinal)
            || s.execution_book
                .as_ref()
                .and_then(|b| b.prepare_service_record())
                != Some(record)
        {
            return Err(invalid());
        }
        if let Some((reason, at)) = s.closing {
            if reason != TestSessionClosingReasonV1::BudgetExhausted
                || at != record.recorded_at
                || root.budgets.transport.remaining().map_err(|_| invalid())? != 0
            {
                return Err(invalid());
            }
        } else if root.budgets.transport.remaining().map_err(|_| invalid())? == 0 {
            return Err(invalid());
        }
        for event in s
            .events
            .iter()
            .skip(usize::try_from(record.accepted_ordinal).map_err(|_| invalid())?)
        {
            match &event.event {
                SessionEventKindV2::Expired { .. } => return Err(invalid()),
                SessionEventKindV2::ControlAcknowledged { request_id } => {
                    if matches!(s.control_records.get(request_id),Some(SessionControlRecordV2::Control{request,..}) if matches!(request.kind,SessionControlKindV1::Finish|SessionControlKindV1::Abort))
                    {
                        return Err(invalid());
                    }
                }
                _ => (),
            }
        }
        Ok(s)
    }
    pub(in crate::provisioning) fn test_session_v2_transport_id_available(
        &self,
        id: &str,
        attempt: &str,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        if transport_id_in_non_machine_state_v2(s, attempt)
            || s.native_machine_registration
                .as_ref()
                .is_some_and(|m| m.transport_id_in_use(attempt))
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(())
    }
    pub fn reserve_test_session_v2_source(
        &mut self,
        id: &str,
        reservation: SessionSourceReservation<'_>,
        now: UnixMillis,
    ) -> Result<TestSessionSourceV2> {
        if matches!(
            reservation.funding,
            SessionSourceFundingV1::ExecutionFactoryV2 { .. }
        ) {
            return Err(invalid());
        }
        self.reserve_test_session_v2_source_inner(id, reservation, now)
    }
    pub(in crate::provisioning) fn reserve_test_session_v2_source_inner(
        &mut self,
        id: &str,
        reservation: SessionSourceReservation<'_>,
        now: UnixMillis,
    ) -> Result<TestSessionSourceV2> {
        let SessionSourceReservation {
            operation_id,
            nonce,
            funding,
        } = reservation;
        let s = self.test_session_v2_record(id)?;
        if let Some(old) = s.sources.get(operation_id) {
            return if old.api_version == 2
                && old.approved_input_digest == s.input_digest()?
                && old.source.verify_nonce == nonce
                && s.source_funding.get(operation_id) == Some(&funding)
            {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.test_session_v2_transport_id_available(id, operation_id)?;
        self.test_session_v2_transport_id_available(id, nonce)?;
        if operation_id == nonce {
            return Err(ProvisionApprovalError::Conflict);
        }
        if s.sources
            .values()
            .any(|saved| saved.source.verify_nonce == nonce)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        if self.frozen() || !hex64(nonce) || s.consumed_at.is_none() || s.sources.len() >= 160 {
            return Err(invalid());
        }
        let (plan, _) = s.header()?;
        let deadline = if let Some(purpose) = funding.purpose() {
            self.test_session_v2_local(id, purpose, now)?
                .roots
                .get(&purpose)
                .ok_or_else(invalid)?
                .deadline
                .get()
        } else {
            plan.read_until.0.max(plan.cleanup_until.0)
        };
        if now.get() >= deadline {
            return Err(invalid());
        }
        let wrapper = TestSessionSourceV2 {
            api_version: 2,
            approved_input_digest: s.input_digest()?,
            source: ProvisionOriginInspectionV1 {
                operation_id: operation_id.into(),
                verify_nonce: nonce.into(),
                created_at: DecimalU64(now.get()),
                expires_at: DecimalU64(
                    now.get()
                        .checked_add(30_000)
                        .ok_or_else(invalid)?
                        .min(deadline),
                ),
                claimed_at: None,
                request_bytes: None,
                verification: None,
                verified_at: None,
            },
        };
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        if let Some(purpose) = funding.purpose() {
            s.roots
                .get_mut(&purpose)
                .ok_or_else(invalid)?
                .budgets
                .transport
                .charge(1)
                .map_err(|_| invalid())?;
        } else {
            s.identity_transport
                .as_mut()
                .ok_or_else(invalid)?
                .charge(1)
                .map_err(|_| invalid())?;
        }
        s.sources.insert(operation_id.into(), wrapper.clone());
        s.source_oracle_reads.insert(operation_id.into(), 0);
        s.source_funding.insert(operation_id.into(), funding);
        append_session_event_v2(
            s,
            SessionEventKindV2::SourceReserved {
                source_operation_id: operation_id.into(),
            },
            now,
        )?;
        if s.approved_at.is_some() {
            super::v2_credential_http::close_if_control_exhausted(s, now);
        }
        next.validate_persistence()?;
        *self = next;
        Ok(wrapper)
    }
    pub fn reserve_test_session_v2_source_read(
        &mut self,
        id: &str,
        source_id: &str,
        now: UnixMillis,
    ) -> Result<u8> {
        if self.frozen() {
            return Err(invalid());
        }
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let wrapper = s.sources.get(source_id).ok_or_else(invalid)?;
        if wrapper.api_version != 2
            || wrapper.approved_input_digest != s.input_digest()?
            || now.get() >= wrapper.source.expires_at.0
        {
            return Err(invalid());
        }
        let reads = s
            .source_oracle_reads
            .get_mut(source_id)
            .ok_or_else(invalid)?;
        if *reads >= 8 {
            return Err(invalid());
        }
        *reads += 1;
        let result = *reads;
        append_session_event_v2(
            s,
            SessionEventKindV2::SourceReadReserved {
                source_operation_id: source_id.into(),
                ticket: result,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub fn claim_test_session_v2_source(
        &mut self,
        id: &str,
        source_id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
    ) -> Result<crate::approval::ApprovalVerifyRequestV1> {
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let r = s.book.original(id).ok_or_else(invalid)?;
        r.check_recorded_context(current.approval)?;
        let expected = s.input_digest()?;
        let wrapper = s.sources.get_mut(source_id).ok_or_else(invalid)?;
        if self.frozen()
            || current.approval.frozen
            || current
                .approval
                .fence
                .check_epoch(current.approval.authority.service_epoch.0)
                .is_err()
            || current.approval.authority != s.input.request.context
            || wrapper.api_version != 2
            || wrapper.approved_input_digest != expected
            || wrapper.source.claimed_at.is_some()
            || now.get() >= wrapper.source.expires_at.0
        {
            return Err(invalid());
        }
        let mut request = r.native_verify_request()?;
        request.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = wrapper.source.verify_nonce.clone();
        wrapper.source.claimed_at = Some(DecimalU64(now.get()));
        wrapper.source.request_bytes =
            Some(serde_json::to_string(&request).map_err(|_| invalid())?);
        append_session_event_v2(
            s,
            SessionEventKindV2::SourceVerificationClaimed {
                source_operation_id: source_id.into(),
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn test_session_v2_source_checks(
        &self,
        id: &str,
        source_id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_v2_record(id)?;
        let r = s.book.original(id).ok_or_else(invalid)?;
        r.check_recorded_context(current.approval)?;
        let wrapper = s.sources.get(source_id).ok_or_else(invalid)?;
        if self.frozen()
            || current.approval.authority != s.input.request.context
            || current.approval.frozen
            || current
                .approval
                .fence
                .check_epoch(current.approval.authority.service_epoch.0)
                .is_err()
            || wrapper.api_version != 2
            || wrapper.approved_input_digest != s.input_digest()?
        {
            return Err(invalid());
        }
        super::source::validate_inspection_record(
            r,
            &wrapper.source,
            signed,
            current.approval,
            now.get(),
        )
    }
    pub fn complete_test_session_v2_source(
        &mut self,
        id: &str,
        source_id: &str,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        if let Some(saved) = self
            .test_session_v2_record(id)?
            .sources
            .get(source_id)
            .and_then(|w| w.source.verification.as_ref())
        {
            return if saved == &signed {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        verifier
            .verify(&self.test_session_v2_source_checks(id, source_id, &signed, current, now)?)?;
        let mut next = self.clone();
        let wrapper = next
            .test_session_v2_record_mut(id)?
            .sources
            .get_mut(source_id)
            .ok_or_else(invalid)?;
        wrapper.source.verification = Some(signed);
        wrapper.source.verified_at = Some(DecimalU64(now.get()));
        append_session_event_v2(
            next.test_session_v2_record_mut(id)?,
            SessionEventKindV2::SourceVerified {
                source_operation_id: source_id.into(),
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}

fn v2_control_exhausted(s: &TestSessionAuthorityV2) -> bool {
    s.control
        .as_ref()
        .is_some_and(|b| b.requests.remaining() == Ok(0) || b.transport.remaining() == Ok(0))
        || s.identity_transport
            .as_ref()
            .is_some_and(|b| b.remaining() == Ok(0))
        || s.connector_transport
            .as_ref()
            .is_some_and(|b| b.remaining() == Ok(0))
}
pub(super) fn v2_purpose_exhausted(s: &TestSessionAuthorityV2) -> bool {
    [SessionPurposeV1::Execution, SessionPurposeV1::ReconcileRead]
        .iter()
        .any(|p| {
            s.roots
                .get(p)
                .is_some_and(|r| r.budgets.transport.remaining() == Ok(0))
        })
}

pub(in crate::provisioning) fn transport_id_in_non_machine_state_v2(
    s: &TestSessionAuthorityV2,
    attempt: &str,
) -> bool {
    s.preflight_world_access
        .as_ref()
        .is_some_and(|w| w.transport_id_in_use(attempt))
        || transport_id_in_non_world2_state_v2(s, attempt)
}
pub(in crate::provisioning) fn transport_id_in_non_world2_state_v2(
    s: &TestSessionAuthorityV2,
    attempt: &str,
) -> bool {
    super::super::execution_book_v2::prepare_paid_id_in_use_v2(s, attempt)
            || !id_nonempty(attempt) || s.sources.contains_key(attempt) || s.source_funding.contains_key(attempt)
            || s.purpose_transports.contains_key(attempt) || s.purpose_http_admissions.contains_key(attempt)
            || s.control_transports.contains_key(attempt) || s.client_http_admissions.contains_key(attempt)
            || super::v2_credentials::credential_claim_id_in_use_v2(s, attempt)
            || super::v2_control::id_in_use_v2(s, attempt)
            || s.sources.values().any(|source| source.source.verify_nonce == attempt)
            || s.tool_nonce_ingresses.contains_key(attempt) || s.termination_state.transport_id_in_use(attempt)
            // ExecutionBook supplies its exact occupancy method before any execution is mounted.
            || s.execution_book.as_ref().is_some_and(|b|b.transport_id_in_use(attempt))
}
