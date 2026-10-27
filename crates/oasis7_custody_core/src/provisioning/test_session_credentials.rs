use super::*;
impl AccountProvisionApprovalState {
    pub fn claim_test_session_credentials(
        &mut self,
        id: &str,
        claim: SessionCredentialClaim<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionCredentialClaimOutcome> {
        self.require_session_source_funding(
            id,
            current.source_operation_id,
            SessionSourceFundingV1::Control,
        )?;
        let request: ClientCredentialClaimRequestV1 = super::lifecycle::original(&claim.proof)?;
        let allocations = claim.allocations;
        let proof = claim.proof.possession;
        request.validate().map_err(|_| invalid())?;
        self.require_session_http_paid(id, proof, claim.proof.ingress_attempt_id, None)?;
        if request.test_session_id != id
            || request.enrollment_id
                != self
                    .test_sessions
                    .get(id)
                    .ok_or_else(invalid)?
                    .enrollment
                    .registration
                    .enrollment_id
            || proof.profile() != HttpPopProfileV1::CredentialClaim
            || proof.request_id() != request.request_id
        {
            return Err(invalid());
        }
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        public_receipt(s, proof, now)?;
        let claim_digest = request.test_session_digest()?;
        if s.control_logicals
            .get(&request.request_id)
            .is_some_and(|d| d != &claim_digest)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let prior: Option<Vec<_>> = request
            .purposes
            .iter()
            .map(|p| {
                let purpose = match p {
                    TestSessionPurposeV1::Execute => SessionPurposeV1::Execution,
                    TestSessionPurposeV1::Read => SessionPurposeV1::ReconcileRead,
                    _ => return None,
                };
                s.roots
                    .get(&purpose)
                    .and_then(|r| {
                        r.credential_history.iter().find(|c| {
                            c.claim_request_id == request.request_id && c.claim_request == request
                        })
                    })
                    .map(|c| (purpose, c.generation))
            })
            .collect();
        if let Some(roots) = prior {
            return Ok(SessionCredentialClaimOutcome {
                newly_claimed: false,
                roots,
            });
        }
        let challenge = s
            .challenges
            .values()
            .find(|c| Some(c.nonce.as_str()) == proof.nonce())
            .ok_or_else(invalid)?;
        if challenge.kind != SessionChallengeKindV1::CredentialClaim
            || challenge.logical_request_id != request.request_id
            || challenge.body_digest != request.test_session_digest()?
            || now.get() >= challenge.expires_at.get()
            || challenge.consumed_by.is_some()
        {
            return Err(invalid());
        }
        let challenge_id = challenge.request_id.clone();
        if allocations
            .iter()
            .map(|a| &a.digest)
            .collect::<BTreeSet<_>>()
            .len()
            != allocations.len()
        {
            return Err(invalid());
        }
        if allocations.len() != request.purposes.len() {
            return Err(invalid());
        }
        // Every issued digest is permanently retired when replaced, across all purposes.
        if allocations.iter().any(|allocation| {
            s.roots.values().any(|root| {
                root.credential_history
                    .iter()
                    .any(|credential| credential.digest == allocation.digest)
            })
        }) {
            return Err(invalid());
        }

        let mut next = self.clone();
        let mut newly = false;
        let mut results = Vec::new();
        let mut seen = BTreeSet::new();
        for p in &request.purposes {
            let purpose = match p {
                TestSessionPurposeV1::Execute => SessionPurposeV1::Execution,
                TestSessionPurposeV1::Read => SessionPurposeV1::ReconcileRead,
                _ => return Err(invalid()),
            };
            verifier.verify(&self.test_session_live_checks(id, purpose, current, now)?)?;
            let allocation = allocations
                .iter()
                .find(|a| a.purpose == purpose)
                .ok_or_else(invalid)?;
            if !hex64(&allocation.digest) || !seen.insert(purpose) {
                return Err(invalid());
            }
            let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
            let root = s.roots.get_mut(&purpose).ok_or_else(invalid)?;
            let generation = match root.credential.as_ref() {
                None => {
                    if request.replace {
                        return Err(invalid());
                    }
                    1
                }
                Some(old) if old.claim_request_id == request.request_id => {
                    results.push((purpose, old.generation));
                    continue;
                }
                Some(old) => {
                    if !request.replace
                        || request.expected_generation != Some(old.generation)
                        || old.replacements >= 3
                        || old.digest == allocation.digest
                    {
                        return Err(invalid());
                    }
                    old.generation.0.checked_add(1).ok_or_else(invalid)?
                }
            };
            let replacements = root
                .credential
                .as_ref()
                .map_or(0, |old| old.replacements + 1);
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
            super::lifecycle::reserve_control_logical(
                s,
                &request.request_id,
                &request.test_session_digest()?,
            )?;
            newly = true;
            results.push((purpose, DecimalU64(generation)));
        }
        next.test_sessions
            .get_mut(id)
            .and_then(|s| s.challenges.get_mut(&challenge_id))
            .ok_or_else(invalid)?
            .consumed_by = Some(request.request_id.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(SessionCredentialClaimOutcome {
            newly_claimed: newly,
            roots: results,
        })
    }
}
