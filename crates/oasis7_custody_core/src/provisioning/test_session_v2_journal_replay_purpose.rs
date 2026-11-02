//! Exact fee/nonce chronology, separate from any context/action grant.
use super::*;
impl Replay<'_> {
    pub(super) fn apply_purpose(
        &mut self,
        event: &SessionEventKindV2,
        at: UnixMillis,
    ) -> Result<()> {
        match event {
            SessionEventKindV2::PurposeHttpCharged { ingress_attempt_id } => {
                let fee = self
                    .s
                    .purpose_http_admissions
                    .get(ingress_attempt_id)
                    .ok_or_else(invalid)?;
                if fee.charged_at != at || fee.hop != SessionControlHopV1::Host {
                    return Err(invalid());
                }
                self.live(fee.purpose, at)?;
                let amount =
                    match &fee.class {
                        SessionPurposeHttpAdmissionClassV1::GenericOne => 1,
                        SessionPurposeHttpAdmissionClassV1::ExecutionStartFiveV2 { .. } => {
                            let original = self
                                .s
                                .execution_pending_ingresses
                                .get(ingress_attempt_id)
                                .ok_or_else(invalid)?;
                            if original.charged_at != at
                                || fee.purpose != SessionPurposeV1::Execution
                                || self.generations.get(&fee.purpose)
                                    != Some(&original.credential_generation.0)
                            {
                                return Err(invalid());
                            }
                            5
                        }
                        SessionPurposeHttpAdmissionClassV1::ToolNonceThree { .. }
                        | SessionPurposeHttpAdmissionClassV1::ExecutionNonceFiveV2 { .. } => {
                            let original = self
                                .s
                                .tool_nonce_ingresses
                                .get(ingress_attempt_id)
                                .ok_or_else(invalid)?;
                            if original.charged_at != at
                                || original.request.purpose != fee.purpose
                                || self.generations.get(&fee.purpose)
                                    != Some(&original.credential_generation.0)
                            {
                                return Err(invalid());
                            }
                            match fee.class {
                                SessionPurposeHttpAdmissionClassV1::ToolNonceThree { .. } => 3,
                                SessionPurposeHttpAdmissionClassV1::ExecutionNonceFiveV2 {
                                    ..
                                } if fee.purpose == SessionPurposeV1::Execution => 5,
                                _ => return Err(invalid()),
                            }
                        }
                    };
                Self::unique(&mut self.purpose_fees, ingress_attempt_id)?;
                self.root_transport
                    .get_mut(&fee.purpose)
                    .ok_or_else(invalid)?
                    .charge(amount)
                    .map_err(|_| invalid())?;
            }
            SessionEventKindV2::ExecutionPendingReserved { request_id } => {
                self.live(SessionPurposeV1::Execution, at)?;
                let p = self
                    .s
                    .execution_pending_start
                    .as_ref()
                    .ok_or_else(invalid)?;
                let original = self
                    .s
                    .execution_pending_ingresses
                    .get(&p.ingress_attempt_id)
                    .ok_or_else(invalid)?;
                if self.execution_pending_reserved
                    || self.execution_started
                    || !self.purpose_fees.contains(&p.ingress_attempt_id)
                    || original.request.request_id != *request_id
                    || p.accepted_at != at
                    || self.lease != Some(p.accepted_lease_until)
                    || !self.issued.contains(&original.challenge_request_id)
                    || self.consumed.contains(&original.challenge_request_id)
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&original.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.root_transport
                    .get_mut(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .charge(134)
                    .map_err(|_| invalid())?;
                self.execution_pending_reserved = true;
            }
            SessionEventKindV2::ExecutionPendingLeafClaimed { request_id, leaf } => {
                self.pending_local(at)?;
                let p = self
                    .s
                    .execution_pending_start
                    .as_ref()
                    .ok_or_else(invalid)?;
                let original = self
                    .s
                    .execution_pending_ingresses
                    .get(&p.ingress_attempt_id)
                    .ok_or_else(invalid)?;
                if p.expected_first_rf_leaf(self.execution_pending_claims) != Some(*leaf)
                    || (self.execution_pending_claims >= 1
                        && !self.execution_pending_directory_signed)
                    || (self.execution_pending_claims >= 2 && !self.execution_pending_fence_signed)
                    || self.execution_pending_fence_read_signed
                    || original.request.request_id != *request_id
                    || p.claimed_leaf_ordinal(*leaf).is_none()
                    || p.claimed_leaf_at(*leaf) != Some(at)
                {
                    return Err(invalid());
                }
                self.execution_pending_claims += 1;
            }
            SessionEventKindV2::ExecutionPendingFenceDispatchRecorded { request_id } => {
                let p = self
                    .s
                    .execution_pending_start
                    .as_ref()
                    .ok_or_else(invalid)?;
                if self.execution_pending_fence_signed
                    || !self.execution_pending_directory_signed
                    || self.execution_pending_claims != 2
                    || p.fence_dispatch_recorded_at() != Some(at)
                    || self
                        .s
                        .execution_pending_ingresses
                        .get(&p.ingress_attempt_id)
                        .ok_or_else(invalid)?
                        .request
                        .request_id
                        != *request_id
                {
                    return Err(invalid());
                }
                self.execution_pending_fence_signed = true;
            }
            SessionEventKindV2::ExecutionPendingFenceReadRecorded { request_id } => {
                let p = self
                    .s
                    .execution_pending_start
                    .as_ref()
                    .ok_or_else(invalid)?;
                if self.execution_pending_fence_read_signed
                    || !self.execution_pending_fence_signed
                    || self.execution_pending_claims != 5
                    || p.fence_read_recorded_at() != Some(at)
                    || self
                        .s
                        .execution_pending_ingresses
                        .get(&p.ingress_attempt_id)
                        .ok_or_else(invalid)?
                        .request
                        .request_id
                        != *request_id
                {
                    return Err(invalid());
                }
                self.execution_pending_fence_read_signed = true;
            }
            SessionEventKindV2::ExecutionPendingDirectorySignatureRecorded { request_id } => {
                let p = self
                    .s
                    .execution_pending_start
                    .as_ref()
                    .ok_or_else(invalid)?;
                if self.execution_pending_directory_signed
                    || self.execution_pending_claims != 1
                    || p.directory_signature_recorded_at() != Some(at)
                    || self
                        .s
                        .execution_pending_ingresses
                        .get(&p.ingress_attempt_id)
                        .ok_or_else(invalid)?
                        .request
                        .request_id
                        != *request_id
                {
                    return Err(invalid());
                }
                // The leaf was already claimed; this only archives its exact
                // signing result and does not bypass Closing for any new leaf.
                self.execution_pending_directory_signed = true;
            }
            SessionEventKindV2::ToolMetadataNonceIssued { request_id } => {
                let link = self
                    .s
                    .tool_nonce_links
                    .get(request_id)
                    .ok_or_else(invalid)?;
                let c = self.s.challenges.get(request_id).ok_or_else(invalid)?;
                if c.kind != SessionChallengeKindV1::ToolMetadata
                    || link.created_at != at
                    || !self.purpose_fees.contains(&link.first_ingress)
                    || self
                        .root_transport
                        .get(&c.purpose)
                        .ok_or_else(invalid)?
                        .remaining()
                        .map_err(|_| invalid())?
                        == 0
                {
                    return Err(invalid());
                }
                self.issue_challenge(request_id, at)?;
            }
            // In particular, future execution stage events remain unsupported.
            _ => return self.apply_execution(event, at),
        }
        Ok(())
    }
}

impl Replay<'_> {
    pub(super) fn pending_local(&self, at: UnixMillis) -> Result<()> {
        let p = self
            .s
            .execution_pending_start
            .as_ref()
            .ok_or_else(invalid)?;
        let original = self
            .s
            .execution_pending_ingresses
            .get(&p.ingress_attempt_id)
            .ok_or_else(invalid)?;
        let root = self
            .root_transport
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        if !self.execution_pending_reserved
            || at.get() < p.accepted_at.get()
            || at.get() >= p.absolute_until.get()
            || self.lease.is_none_or(|lease| at.get() >= lease.get())
            || self.generations.get(&SessionPurposeV1::Execution)
                != Some(&original.credential_generation.0)
            || self.control.transport.remaining().map_err(|_| invalid())? == 0
            || self.identity.remaining().map_err(|_| invalid())? == 0
            || self.acks.iter().any(|id|matches!(self.s.control_records.get(id),Some(SessionControlRecordV2::Control{request,..}) if matches!(request.kind,SessionControlKindV1::Finish|SessionControlKindV1::Abort)))
            || self
                .root_transport
                .get(&SessionPurposeV1::ReconcileRead)
                .is_some_and(|r| r.remaining() == Ok(0))
        {
            return Err(invalid());
        }
        if let Some((reason, time)) = self.closing {
            if reason != TestSessionClosingReasonV1::BudgetExhausted
                || time != p.accepted_at
                || root.remaining().map_err(|_| invalid())? != 0
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
