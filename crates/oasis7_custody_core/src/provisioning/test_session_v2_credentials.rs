//! V2 credentials use the actual V2 Book and Root, never a V1 authority copy.
use super::*;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCredentialAllocationFactV2 {
    pub purpose: SessionPurposeV1,
    pub digest: String,
    pub generation: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCredentialClaimRecordV2 {
    pub api_version: u8,
    pub approved_input_digest: String,
    pub request: ClientCredentialClaimRequestV1,
    pub allocations: Vec<SessionCredentialAllocationFactV2>,
    pub ingress_attempt_id: String,
    pub signed_attempt_digest: String,
    pub challenge_request_id: String,
    pub source_operation_id: String,
    pub source_ticket: u8,
    pub claimed_at: UnixMillis,
    pub private_sid_generation: DecimalU64,
    pub lifetime: SignedManagedNativeSessionLifetimeV1,
}
impl core::fmt::Debug for SessionCredentialClaimRecordV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionCredentialClaimRecordV2(<private>)")
    }
}
pub(in crate::provisioning) fn credential_purpose_v2(
    p: &TestSessionPurposeV1,
) -> Result<SessionPurposeV1> {
    match p {
        TestSessionPurposeV1::Execute => Ok(SessionPurposeV1::Execution),
        TestSessionPurposeV1::Read => Ok(SessionPurposeV1::ReconcileRead),
        _ => Err(invalid()),
    }
}
pub(in crate::provisioning) fn credential_claim_id_in_use_v2(
    s: &TestSessionAuthorityV2,
    id: &str,
) -> bool {
    s.credential_claims.iter().any(|(key, claim)| {
        key == id
            || claim.challenge_request_id == id
            || claim.ingress_attempt_id == id
            || claim.source_operation_id == id
            || claim.lifetime.lifetime.status.nonce == id
    }) || s
        .challenges
        .values()
        .any(|c| c.request_id == id || c.nonce == id || c.logical_request_id == id)
        || s.roots
            .values()
            .flat_map(|r| &r.credential_history)
            .any(|c| c.claim_request_id == id)
}
impl AccountProvisionApprovalState {
    /// Authenticated paid metadata only. Call before Source/Native IO so a
    /// lost reply never creates another token or revives retired credentials.
    pub fn test_session_v2_credential_claim_history(
        &self,
        id: &str,
        proof: &SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<Option<SessionCredentialClaimOutcome>> {
        let q: ClientCredentialClaimRequestV1 = super::lifecycle::original(proof)?;
        q.validate().map_err(|_| invalid())?;
        let s = self.test_session_v2_record(id)?;
        super::v2_credential_http::public_receipt(s, proof.possession, now)?;
        self.require_test_session_v2_claim_http_paid(
            id,
            proof.possession,
            proof.ingress_attempt_id,
        )?;
        if q.test_session_id != id
            || q.enrollment_id != s.header()?.1.client_pairing.enrollment_id
            || proof.possession.profile() != HttpPopProfileV1::CredentialClaim
            || proof.possession.request_id() != q.request_id
        {
            return Err(invalid());
        }
        let Some(old) = s.credential_claims.get(&q.request_id) else {
            return Ok(None);
        };
        if old.request != q || old.approved_input_digest != s.input_digest()? {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(Some(SessionCredentialClaimOutcome {
            newly_claimed: false,
            roots: old
                .allocations
                .iter()
                .map(|a| (a.purpose, a.generation))
                .collect(),
        }))
    }
    pub fn claim_test_session_v2_credentials(
        &mut self,
        id: &str,
        claim: SessionCredentialClaim<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionCredentialClaimOutcome> {
        let request: ClientCredentialClaimRequestV1 = super::lifecycle::original(&claim.proof)?;
        if let Some(old) = self.test_session_v2_credential_claim_history(id, &claim.proof, now)? {
            return Ok(old);
        }
        request.validate().map_err(|_| invalid())?;
        let proof = claim.proof.possession;
        let s = self.test_session_v2_record(id)?;
        super::v2_credential_http::native_registration(s)?;
        super::v2_credential_http::public_receipt(s, proof, now)?;
        self.require_test_session_v2_claim_http_paid(id, proof, claim.proof.ingress_attempt_id)?;
        if request.test_session_id != id
            || request.enrollment_id != s.header()?.1.client_pairing.enrollment_id
            || proof.profile() != HttpPopProfileV1::CredentialClaim
            || proof.request_id() != request.request_id
        {
            return Err(invalid());
        }
        let claim_digest = request.test_session_digest()?;
        if s.credential_claims.len() >= 8
            || s.control_logicals.get(&request.request_id) != Some(&claim_digest)
            || s.source_funding.get(current.source_operation_id)
                != Some(&SessionSourceFundingV1::Control)
            || s.credential_claims
                .values()
                .any(|old| old.source_operation_id == current.source_operation_id)
            || current.private_sid_generation.0 == 0
            || current.source_ticket != 4
            || s.source_oracle_reads.get(current.source_operation_id) != Some(&4)
            || s.control_transports
                .get(&super::v2_credential_source::lifetime_attempt_v2(
                    s,
                    current.source_operation_id,
                )?)
                != Some(&SessionControlHopV1::Identity)
            || s.consumed_at.is_none_or(|t| now.get() < t.get())
        {
            return Err(invalid());
        }
        let challenge = s
            .challenges
            .values()
            .find(|c| Some(c.nonce.as_str()) == proof.nonce())
            .ok_or_else(invalid)?;
        if challenge.kind != SessionChallengeKindV1::CredentialClaim
            || challenge.logical_request_id != request.request_id
            || challenge.body_digest != claim_digest
            || challenge.consumed_by.is_some()
            || now.get() >= challenge.expires_at.get()
            || !request
                .purposes
                .iter()
                .any(|p| credential_purpose_v2(p).ok() == Some(challenge.purpose))
        {
            return Err(invalid());
        }
        let challenge_root = s.roots.get(&challenge.purpose).ok_or_else(invalid)?;
        if challenge.credential_generation
            != challenge_root
                .credential
                .as_ref()
                .map_or(DecimalU64(0), |c| c.generation)
        {
            return Err(invalid());
        }
        let allocations = claim.allocations;
        if allocations.len() != request.purposes.len()
            || allocations
                .iter()
                .map(|a| &a.digest)
                .collect::<BTreeSet<_>>()
                .len()
                != allocations.len()
            || allocations.iter().any(|a| {
                !hex64(&a.digest)
                    || s.roots
                        .values()
                        .any(|r| r.credential_history.iter().any(|c| c.digest == a.digest))
            })
        {
            return Err(invalid());
        }
        // Verify every exact current manifest before allocating any purpose.
        let mut seen = BTreeSet::new();
        for p in &request.purposes {
            let purpose = credential_purpose_v2(p)?;
            if !seen.insert(purpose) {
                return Err(invalid());
            }
            verifier.verify(&self.test_session_v2_live_scope(id, purpose, current, now)?)?;
        }
        let challenge_id = challenge.request_id.clone();
        let input_digest = s.input_digest()?;
        let mut next = self.clone();
        let mut facts = Vec::new();
        for p in &request.purposes {
            let purpose = credential_purpose_v2(p)?;
            let allocation = allocations
                .iter()
                .find(|a| a.purpose == purpose)
                .ok_or_else(invalid)?;
            let session = next.test_sessions_v2.get_mut(id).ok_or_else(invalid)?;
            let root = session.roots.get_mut(&purpose).ok_or_else(invalid)?;
            if root.approved_input_digest != input_digest {
                return Err(invalid());
            }
            let (generation, replacements) = match root.credential.as_ref() {
                None if !request.replace
                    && request.expected_generation.is_none()
                    && root.credential_history.is_empty() =>
                {
                    (1, 0)
                }
                Some(old)
                    if request.replace
                        && request.expected_generation == Some(old.generation)
                        && old.replacements < 3
                        && old.digest != allocation.digest =>
                {
                    (
                        old.generation.0.checked_add(1).ok_or_else(invalid)?,
                        old.replacements.checked_add(1).ok_or_else(invalid)?,
                    )
                }
                _ => return Err(invalid()),
            };
            if root.credential_history.len() >= 4 {
                return Err(invalid());
            }
            let credential = SessionCredentialV1 {
                digest: allocation.digest.clone(),
                generation: DecimalU64(generation),
                replacements,
                claimed_at: now,
                claim_request_id: request.request_id.clone(),
                claim_request: request.clone(),
            };
            root.credential_history.push(credential.clone());
            root.credential = Some(credential);
            facts.push(SessionCredentialAllocationFactV2 {
                purpose,
                digest: allocation.digest.clone(),
                generation: DecimalU64(generation),
            });
        }
        let session = next.test_sessions_v2.get_mut(id).ok_or_else(invalid)?;
        session
            .challenges
            .get_mut(&challenge_id)
            .ok_or_else(invalid)?
            .consumed_by = Some(request.request_id.clone());
        session.credential_claims.insert(
            request.request_id.clone(),
            SessionCredentialClaimRecordV2 {
                api_version: 2,
                approved_input_digest: input_digest,
                request,
                allocations: facts.clone(),
                ingress_attempt_id: claim.proof.ingress_attempt_id.into(),
                signed_attempt_digest: proof.transport_attempt_digest(),
                challenge_request_id: challenge_id,
                source_operation_id: current.source_operation_id.into(),
                source_ticket: current.source_ticket,
                claimed_at: now,
                private_sid_generation: current.private_sid_generation,
                lifetime: (*current.lifetime).clone(),
            },
        );
        next.validate_persistence()?;
        *self = next;
        Ok(SessionCredentialClaimOutcome {
            newly_claimed: true,
            roots: facts.iter().map(|a| (a.purpose, a.generation)).collect(),
        })
    }
}
