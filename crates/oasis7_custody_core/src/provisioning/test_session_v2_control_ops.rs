use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
impl AccountProvisionApprovalState {
    pub fn test_session_v2_control_history(
        &self,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<Option<SessionControlAckV1>> {
        let q: SessionControlRequestV1 = original(&proof)?;
        let s = self.test_session_v2_record(&q.session_id)?;
        http::paid(
            self,
            &q.session_id,
            proof.possession,
            proof.ingress_attempt_id,
        )?;
        if q.api_version != 1
            || q.request_id != proof.possession.request_id()
            || proof.possession.profile() != HttpPopProfileV1::Custody
        {
            return Err(invalid());
        }
        let Some(ack) = s.controls.get(&q.request_id) else {
            return Ok(None);
        };
        if super::super::authenticated_receipt_v2(
            s,
            SessionPurposeV1::Execution,
            proof.possession,
            now,
        )
        .is_err()
        {
            super::super::authenticated_receipt_v2(
                s,
                SessionPurposeV1::ReconcileRead,
                proof.possession,
                now,
            )?;
        }
        let Some(SessionControlRecordV2::Control {
            approved_input_digest,
            request,
            ..
        }) = s.control_records.get(&q.request_id)
        else {
            return Err(invalid());
        };
        if request != &q || ack.kind != q.kind || approved_input_digest != &s.input_digest()? {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(Some(ack.clone()))
    }
    pub fn issue_test_session_v2_control_challenge(
        &mut self,
        proof: SessionProofRequest<'_>,
        nonce: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionChallengeV1> {
        let q: SessionChallengeRequestV1 = original(&proof)?;
        let s = self.test_session_v2_record(&q.session_id)?;
        http::paid(
            self,
            &q.session_id,
            proof.possession,
            proof.ingress_attempt_id,
        )?;
        let SessionChallengeKindV1::Control(kind) = q.kind else {
            return Err(invalid());
        };
        if q.api_version != 1
            || q.request_id != proof.possession.request_id()
            || proof.possession.profile() != HttpPopProfileV1::CustodyChallenge
            || !valid_id(&q.request_id)
            || !valid_id(&q.logical_request_id)
            || q.request_id == q.logical_request_id
            || !hex64(&q.operation_request_digest)
            || q.purpose == SessionPurposeV1::TerminationPlan
            || kind == SessionControlKindV1::Heartbeat && q.purpose != SessionPurposeV1::Execution
        {
            return Err(invalid());
        }
        let generation =
            super::super::authenticated_receipt_v2(s, q.purpose, proof.possession, now)?.generation;
        let obs = observation(s, current, now)?;
        verifier.verify(&self.test_session_v2_live_scope(
            &q.session_id,
            q.purpose,
            current,
            now,
        )?)?;
        if let Some(old) = s.challenges.get(&q.request_id) {
            let Some(SessionControlRecordV2::Challenge { request, .. }) =
                s.control_records.get(&q.request_id)
            else {
                return Err(invalid());
            };
            return if request == &q {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.test_session_v2_transport_id_available(&q.session_id, &q.request_id)?;
        if !s.control_logicals.contains_key(&q.logical_request_id) {
            self.test_session_v2_transport_id_available(&q.session_id, &q.logical_request_id)?;
        }
        self.test_session_v2_transport_id_available(&q.session_id, nonce)?;
        let raw = URL_SAFE_NO_PAD.decode(nonce).map_err(|_| invalid())?;
        if raw.len() != 32
            || URL_SAFE_NO_PAD.encode(raw) != nonce
            || nonce == q.request_id
            || nonce == q.logical_request_id
            || s.challenges.len() >= 1024
            || s.challenges
                .values()
                .filter(|v| v.consumed_by.is_none() && now.get() < v.expires_at.get())
                .count()
                >= 8
            || s.challenges
                .values()
                .filter(|v| v.created_at.get() / 1000 == now.get() / 1000)
                .count()
                >= 2
        {
            return Err(invalid());
        }
        let root = s.roots.get(&q.purpose).ok_or_else(invalid)?;
        let challenge = SessionChallengeV1 {
            request_id: q.request_id.clone(),
            purpose: q.purpose,
            kind: q.kind,
            logical_request_id: q.logical_request_id.clone(),
            body_digest: q.operation_request_digest.clone(),
            nonce: nonce.into(),
            credential_generation: generation,
            created_at: now,
            expires_at: UnixMillis::new(
                now.get()
                    .checked_add(30_000)
                    .ok_or_else(invalid)?
                    .min(root.deadline.get()),
            ),
            consumed_by: None,
        };
        let input_digest = s.input_digest()?;
        let sequence = next_sequence(s, now)?;
        let mut next = self.clone();
        let s = next
            .test_sessions_v2
            .get_mut(&q.session_id)
            .ok_or_else(invalid)?;
        reserve_logical(s, &q.logical_request_id, &q.operation_request_digest)?;
        s.control_records.insert(
            q.request_id.clone(),
            SessionControlRecordV2::Challenge {
                sequence,
                approved_input_digest: input_digest,
                request: q.clone(),
                ingress_attempt_id: proof.ingress_attempt_id.into(),
                signed_attempt_digest: proof.possession.transport_attempt_digest(),
                observation: obs,
            },
        );
        s.challenges.insert(q.request_id, challenge.clone());
        super::super::v2_credential_http::close_if_control_exhausted(s, now);
        if append_session_event_v2(
            s,
            SessionEventKindV2::ChallengeIssued {
                request_id: challenge.request_id.clone(),
                source_operation_id: current.source_operation_id.into(),
                ingress_attempt_id: proof.ingress_attempt_id.into(),
                profile: SessionJournalProfileV2::Control,
            },
            now,
        )? != sequence
        {
            return Err(invalid());
        }
        next.validate_persistence()?;
        *self = next;
        Ok(challenge)
    }
    pub fn control_test_session_v2(
        &mut self,
        proof: SessionProofRequest<'_>,
        current: Option<&TestSessionLiveCurrent<'_, '_>>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionControlAckV1> {
        if let Some(old) = self.test_session_v2_control_history(
            SessionProofRequest {
                ingress_attempt_id: proof.ingress_attempt_id,
                original_bytes: proof.original_bytes,
                possession: proof.possession,
            },
            now,
        )? {
            return Ok(old);
        }
        let q: SessionControlRequestV1 = original(&proof)?;
        let s = self.test_session_v2_record(&q.session_id)?;
        let nonce = proof.possession.nonce().ok_or_else(invalid)?;
        let c = s
            .challenges
            .values()
            .find(|c| c.nonce == nonce)
            .ok_or_else(invalid)?;
        let credential =
            super::super::authenticated_receipt_v2(s, c.purpose, proof.possession, now)?;
        if c.kind != SessionChallengeKindV1::Control(q.kind)
            || c.logical_request_id != q.request_id
            || c.body_digest != q.digest()?
            || c.credential_generation != credential.generation
            || c.consumed_by.is_some()
            || now.get() >= c.expires_at.get()
            || q.kind == SessionControlKindV1::Heartbeat && c.purpose != SessionPurposeV1::Execution
        {
            return Err(invalid());
        }
        let obs = if closing_kind(q.kind) {
            if current.is_some() {
                return Err(invalid());
            }
            None
        } else {
            let current = current.ok_or_else(invalid)?;
            let obs = observation(s, current, now)?;
            verifier.verify(&self.test_session_v2_live_scope(
                &q.session_id,
                c.purpose,
                current,
                now,
            )?)?;
            Some(obs)
        };
        if q.kind == SessionControlKindV1::Heartbeat
            && s.last_heartbeat
                .is_some_and(|v| now.get() < v.get().saturating_add(30_000))
        {
            return Err(invalid());
        }
        let challenge_id = c.request_id.clone();
        let input_digest = s.input_digest()?;
        let sequence = next_sequence(s, now)?;
        let mut next = self.clone();
        let s = next
            .test_sessions_v2
            .get_mut(&q.session_id)
            .ok_or_else(invalid)?;
        reserve_logical(s, &q.request_id, &q.digest()?)?;
        s.challenges
            .get_mut(&challenge_id)
            .ok_or_else(invalid)?
            .consumed_by = Some(q.request_id.clone());
        if q.kind == SessionControlKindV1::Heartbeat {
            s.lease_until = Some(UnixMillis::new(
                now.get()
                    .checked_add(90_000)
                    .ok_or_else(invalid)?
                    .min(s.header()?.0.write_deadline.0),
            ));
            s.last_heartbeat = Some(now);
        } else if closing_kind(q.kind) && s.closing.is_none() {
            s.closing = Some((
                if q.kind == SessionControlKindV1::Finish {
                    TestSessionClosingReasonV1::Finished
                } else {
                    TestSessionClosingReasonV1::Aborted
                },
                now,
            ));
        }
        let ack = SessionControlAckV1 {
            request_id: q.request_id.clone(),
            kind: q.kind,
            acknowledged_at: now,
            lease_until: s.lease_until.ok_or_else(invalid)?,
        };
        s.controls.insert(q.request_id.clone(), ack.clone());
        s.control_records.insert(
            q.request_id.clone(),
            SessionControlRecordV2::Control {
                sequence,
                approved_input_digest: input_digest,
                request: q,
                ingress_attempt_id: proof.ingress_attempt_id.into(),
                signed_attempt_digest: proof.possession.transport_attempt_digest(),
                challenge_request_id: challenge_id,
                observation: obs,
            },
        );
        super::super::v2_credential_http::close_if_control_exhausted(s, now);
        if append_session_event_v2(
            s,
            SessionEventKindV2::ControlAcknowledged {
                request_id: ack.request_id.clone(),
            },
            now,
        )? != sequence
        {
            return Err(invalid());
        }
        next.validate_persistence()?;
        *self = next;
        Ok(ack)
    }
}
