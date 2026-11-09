//! Full body, fresh response and exact paid observation bindings.
use super::*;
impl AccountProvisionApprovalState {
    fn live_context_payload(
        &self,
        id: &str,
        attempt: &str,
        evidence: &LiveContextReadEvidence<'_>,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let c = s.context.as_ref().ok_or_else(invalid)?;
        let record = c.live_reads.get(attempt).ok_or_else(invalid)?;
        let reference = c
            .slots
            .get(&ContextBodySlotKindV1::Captured)
            .ok_or_else(invalid)?;
        if evidence.historical_body.scope().slot != ContextBodySlotKindV1::Captured
            || reference_digest(reference)? != record.reference_digest
            || source_digest(s, &record.source_operation_id)? != record.source_digest
        {
            return Err(invalid());
        }
        self.context_body_matches(id, evidence.historical_body)?;
        verify_live_context_reply(
            &record.request,
            evidence.response,
            &reference.scope.world.service_public_key,
        )
        .map_err(|_| invalid())?;
        let old = evidence
            .historical_body
            .captured_response()
            .map_err(body_error)?;
        let immutable = |v: &CapturedTaskContextV1| {
            oasis7_wasm_abi::encode_canonical_cbor(&(
                &v.prepared,
                &v.capture_commit,
                &v.capture_proof,
            ))
            .map_err(|_| invalid())
        };
        if immutable(&old.payload)? != immutable(&evidence.response.payload)? {
            return Err(ProvisionApprovalError::Conflict);
        }
        let bytes = zeroize::Zeroizing::new(
            oasis7_wasm_abi::encode_canonical_cbor(&evidence.response.payload)
                .map_err(|_| invalid())?,
        );
        if bytes.len() > CONTEXT_BODY_MAX_BYTES {
            return Err(ProvisionApprovalError::ContextTooLarge);
        }
        Ok(bytes)
    }
    pub(super) fn live_context_fact(
        &self,
        id: &str,
        attempt: &str,
        evidence: &LiveContextReadEvidence<'_>,
        archived_at: UnixMillis,
    ) -> Result<LiveContextReadFactV1> {
        let bytes = self.live_context_payload(id, attempt, evidence)?;
        Ok(LiveContextReadFactV1 {
            request_digest: evidence.response.request_digest.clone(),
            signature_hex: evidence.response.signature_hex.clone(),
            payload_digest: crate::approval::bytes_digest(&bytes),
            payload_bytes: DecimalU64(bytes.len() as u64),
            read_commit: evidence
                .response
                .payload
                .current_public_context
                .read_commit
                .clone(),
            capture_commit: evidence.response.payload.capture_commit.clone(),
            archived_at,
        })
    }
    /// Whole historical body and whole challenge-bound response, without Current.
    /// The trusted adapter must execute every returned job, including nested ones.
    pub fn test_session_live_context_archive_checks(
        &self,
        id: &str,
        attempt: &str,
        evidence: &LiveContextReadEvidence<'_>,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        self.validate_live_context_persistence(id, s)?;
        let record = self.live_context_record(id, attempt)?;
        let bytes = self.live_context_payload(id, attempt, evidence)?;
        let fact = self.live_context_fact(
            id,
            attempt,
            evidence,
            record
                .archive
                .as_ref()
                .map_or(record.claimed_at, |v| v.archived_at),
        )?;
        let checks = self.context_historical_body_checks(id, evidence.historical_body)?;
        let checks = append_context_signature_checks(
            checks,
            ContextSignatureFact::LiveCaptured {
                request: &record.request,
                response: evidence.response,
                world_key: &evidence.historical_body.scope().world.service_public_key,
            },
        )?;
        Ok(bind_live_context_checks(
            checks, record, &fact, &bytes, None,
        )?)
    }
    /// Complete post-reply Current plus exact historical archive. A previous
    /// observation/ticket or another Source cannot refresh this claim.
    pub fn test_session_live_context_checks(
        &self,
        id: &str,
        attempt: &str,
        evidence: &LiveContextReadEvidence<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        self.validate_live_context_persistence(
            id,
            self.test_sessions.get(id).ok_or_else(invalid)?,
        )?;
        let record = self.live_context_record(id, attempt)?;
        let archive = record.archive.as_ref().ok_or_else(invalid)?;
        if record.admitted_at.is_some()
            || record.admission_observed_at.is_some()
            || record.admission_source_ticket.is_some()
            || current.source_operation_id != record.source_operation_id
            || current.source_ticket <= record.source_ticket
            || current.observed_at.get() < archive.archived_at.get()
            || now.get() < archive.archived_at.get()
            || now.get() >= record.deadline.get()
            || current.lifetime.lifetime.digest().map_err(|_| invalid())? != record.lifetime_digest
        {
            return Err(invalid());
        }
        let fact = self.live_context_fact(id, attempt, evidence, archive.archived_at)?;
        if &fact != archive {
            return Err(ProvisionApprovalError::Conflict);
        }
        let bytes = self.live_context_payload(id, attempt, evidence)?;
        let checks = self.context_complete_checks(id, evidence.historical_body, current, now)?;
        let checks = append_context_signature_checks(
            checks,
            ContextSignatureFact::LiveCaptured {
                request: &record.request,
                response: evidence.response,
                world_key: &evidence.historical_body.scope().world.service_public_key,
            },
        )?;
        let lifetime_digest = current.lifetime.lifetime.digest().map_err(|_| invalid())?;
        let binding = crate::approval_admission::LiveContextCurrentBinding {
            source: current.source_operation_id,
            ticket: current.source_ticket,
            observed_at: current.observed_at,
            private_generation: current.private_sid_generation,
            lifetime_digest: &lifetime_digest,
        };
        Ok(bind_live_context_checks(
            checks,
            record,
            &fact,
            &bytes,
            Some(&binding),
        )?)
    }
}
