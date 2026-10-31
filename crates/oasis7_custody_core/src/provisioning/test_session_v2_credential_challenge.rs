//! Closed credential-claim challenges; no Tool or World action admission.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
impl AccountProvisionApprovalState {
    pub fn test_session_v2_credential_challenge_history(
        &self,
        proof: &SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<Option<SessionChallengeV1>> {
        let q: SessionChallengeRequestV1 = super::lifecycle::original(proof)?;
        if q.api_version != 1
            || q.kind != SessionChallengeKindV1::CredentialClaim
            || q.purpose == SessionPurposeV1::TerminationPlan
            || !id(&q.request_id)
            || !id(&q.logical_request_id)
            || !hex64(&q.operation_request_digest)
            || proof.possession.profile() != HttpPopProfileV1::ClaimChallenge
            || proof.possession.request_id() != q.request_id
        {
            return Err(invalid());
        }
        let s = self.test_session_v2_record(&q.session_id)?;
        super::v2_credential_http::public_receipt(s, proof.possession, now)?;
        self.require_test_session_v2_claim_http_paid(
            &q.session_id,
            proof.possession,
            proof.ingress_attempt_id,
        )?;
        let Some(old) = s.challenges.get(&q.request_id) else {
            return Ok(None);
        };
        if old.purpose != q.purpose
            || old.kind != q.kind
            || old.logical_request_id != q.logical_request_id
            || old.body_digest != q.operation_request_digest
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(Some(old.clone()))
    }
    pub fn issue_test_session_v2_credential_challenge(
        &mut self,
        proof: SessionProofRequest<'_>,
        nonce: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionChallengeV1> {
        let q: SessionChallengeRequestV1 = super::lifecycle::original(&proof)?;
        if let Some(old) = self.test_session_v2_credential_challenge_history(&proof, now)? {
            return Ok(old);
        }
        if q.api_version != 1
            || q.kind != SessionChallengeKindV1::CredentialClaim
            || q.purpose == SessionPurposeV1::TerminationPlan
            || !id(&q.request_id)
            || !id(&q.logical_request_id)
            || q.request_id == q.logical_request_id
            || !hex64(&q.operation_request_digest)
            || proof.possession.request_id() != q.request_id
            || nonce == q.request_id
            || nonce == q.logical_request_id
            || proof.possession.profile() != HttpPopProfileV1::ClaimChallenge
        {
            return Err(invalid());
        }
        let s = self.test_session_v2_record(&q.session_id)?;
        super::v2_credential_http::public_receipt(s, proof.possession, now)?;
        self.require_test_session_v2_claim_http_paid(
            &q.session_id,
            proof.possession,
            proof.ingress_attempt_id,
        )?;
        if s.source_funding.get(current.source_operation_id)
            != Some(&SessionSourceFundingV1::Control)
        {
            return Err(invalid());
        }
        if current.source_ticket != 4
            || s.source_oracle_reads.get(current.source_operation_id) != Some(&4)
            || current.private_sid_generation.0 == 0
            || s.control_transports
                .get(&super::v2_credential_source::lifetime_attempt_v2(
                    s,
                    current.source_operation_id,
                )?)
                != Some(&SessionControlHopV1::Identity)
        {
            return Err(invalid());
        }
        self.test_session_v2_local(&q.session_id, q.purpose, now)?;
        self.test_session_v2_transport_id_available(&q.session_id, &q.request_id)?;
        self.test_session_v2_transport_id_available(&q.session_id, nonce)?;
        if let Some(old) = s.control_logicals.get(&q.logical_request_id) {
            if old != &q.operation_request_digest
                || !s.challenges.values().any(|c| {
                    c.kind == SessionChallengeKindV1::CredentialClaim
                        && c.logical_request_id == q.logical_request_id
                        && c.body_digest == q.operation_request_digest
                })
            {
                return Err(ProvisionApprovalError::Conflict);
            }
        } else {
            self.test_session_v2_transport_id_available(&q.session_id, &q.logical_request_id)?;
        }
        let bytes = URL_SAFE_NO_PAD.decode(nonce).map_err(|_| invalid())?;
        if bytes.len() != 32
            || URL_SAFE_NO_PAD.encode(bytes) != nonce
            || s.challenges.values().any(|c| c.nonce == nonce)
            || s.challenges.len() >= 1024
            || s.challenges
                .values()
                .filter(|c| c.consumed_by.is_none() && now.get() < c.expires_at.get())
                .count()
                >= 8
            || s.challenges
                .values()
                .filter(|c| c.created_at.get() / 1000 == now.get() / 1000)
                .count()
                >= 2
        {
            return Err(invalid());
        }
        verifier.verify(&self.test_session_v2_live_scope(
            &q.session_id,
            q.purpose,
            current,
            now,
        )?)?;
        let root = s.roots.get(&q.purpose).ok_or_else(invalid)?;
        // Zero denotes *absence in a claim challenge*, never an issued credential.
        // First issuance below requires both current=None and empty history and
        // produces generation1; every replacement requires explicit expectedgen.
        let generation = root
            .credential
            .as_ref()
            .map_or(DecimalU64(0), |c| c.generation);
        let challenge = SessionChallengeV1 {
            request_id: q.request_id,
            nonce: nonce.into(),
            purpose: q.purpose,
            kind: q.kind,
            logical_request_id: q.logical_request_id.clone(),
            credential_generation: generation,
            body_digest: q.operation_request_digest.clone(),
            created_at: now,
            expires_at: UnixMillis::new(
                now.get()
                    .checked_add(30_000)
                    .ok_or_else(invalid)?
                    .min(root.deadline.get()),
            ),
            consumed_by: None,
        };
        let mut next = self.clone();
        let s = next
            .test_sessions_v2
            .get_mut(&q.session_id)
            .ok_or_else(invalid)?;
        if !s.control_logicals.contains_key(&q.logical_request_id) {
            s.control
                .as_mut()
                .ok_or_else(invalid)?
                .requests
                .reserve(&q.logical_request_id, 1)
                .map_err(|_| invalid())?;
            s.control_logicals
                .insert(q.logical_request_id, q.operation_request_digest);
        }
        s.challenges
            .insert(challenge.request_id.clone(), challenge.clone());
        super::v2_credential_http::close_if_control_exhausted(s, now);
        next.validate_persistence()?;
        *self = next;
        Ok(challenge)
    }
}
