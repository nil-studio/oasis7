use super::*;
impl State {
    pub fn validate(&self) -> Result<(), String> {
        self.identity.validate()?;
        let raw = super::super::vault::secret32(&self.signing_seed)?;
        let key = ed25519_dalek::SigningKey::from_bytes(&raw);
        if self.api_version != 1
            || hex::encode(key.verifying_key().to_bytes())
                != self.identity.descriptor.client_public_key
            || self.enrollment.original.target != self.identity.target
            || self.enrollment.original.descriptor != self.identity.descriptor
            || self.attempts.len() > 248
        {
            return Err(denied());
        }
        self.enrollment.original.validate().map_err(|_| denied())?;
        if let Some(r) = &self.enrollment.response {
            r.validate().map_err(|_| denied())?;
            if r.descriptor_digest != self.identity.descriptor.digest().map_err(|_| denied())? {
                return Err(denied());
            }
        }
        if let Some(p) = &self.enrollment.proof {
            let r = self.enrollment.response.as_ref().ok_or_else(denied)?;
            p.validate().map_err(|_| denied())?;
            if p.target != self.identity.target
                || p.client_public_key != self.identity.descriptor.client_public_key
                || p.enrollment_id != r.enrollment_id
                || p.server_nonce != r.server_nonce
                || p.challenge_expires_at != r.challenge_expires_at
                || p.descriptor_digest != r.descriptor_digest
            {
                return Err(denied());
            }
        }
        if self.enrollment.proved && self.enrollment.proof.is_none() {
            return Err(denied());
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut boot = 0u64;
        let mut live = 0u64;
        for a in &self.attempts {
            if !id(&a.id)
                || !id(&a.original_id)
                || a.at > self.time_floor
                || !seen.insert(&a.id)
                || [&a.target_digest, &a.body_digest]
                    .iter()
                    .any(|s| s.len() != 64 || hex::decode(s).is_err())
                || a.generation.is_some_and(|g| g.0 > 4)
                || a.purpose == Some(SessionPurposeV1::TerminationPlan)
                || a.bootstrap
                    && (a.purpose.is_some()
                        || a.generation.is_some()
                        || !matches!(a.profile, None | Some(HttpPopProfileV1::EnrollmentProof)))
                || !a.bootstrap
                    && (a.purpose.is_none()
                        || !matches!(
                            a.profile,
                            Some(
                                HttpPopProfileV1::Custody
                                    | HttpPopProfileV1::CustodyChallenge
                                    | HttpPopProfileV1::ClaimChallenge
                                    | HttpPopProfileV1::CredentialClaim
                            )
                        ))
            {
                return Err(denied());
            }
            if a.bootstrap { boot += 1 } else { live += 1 }
        }
        if boot > 8 {
            return Err(denied());
        }
        if let Some(s) = &self.session {
            s.binding.validate(
                &self.identity,
                self.enrollment.response.as_ref().ok_or_else(denied)?,
            )?;
            if !self.enrollment.proved
                || live > 0 && !s.inbox_ready
                || super::super::vault::secret32(&s.inbox_key).is_err()
                || s.credentials.len() > 2
                || s.retired_execution > 4
                || s.retired_read > 4
                || s.logicals.len() as u64 > s.binding.plan.control_budget().requests.0
                || live > s.binding.plan.control_budget().transport_per_hop.0
                || 8u64
                    .checked_add(live.checked_mul(2).ok_or_else(denied)?)
                    .ok_or_else(denied)?
                    > s.binding.plan.control_budget().identity_transport.0
            {
                return Err(denied());
            }
            let mut purposes = std::collections::BTreeSet::new();
            for c in &s.credentials {
                c.validate()?;
                let claim = s.claims.get(&c.claim_request_id).ok_or_else(denied)?;
                let wanted = match c.purpose {
                    SessionPurposeV1::Execution => {
                        oasis7_custody_core::client_enrollment::TestSessionPurposeV1::Execute
                    }
                    SessionPurposeV1::ReconcileRead => {
                        oasis7_custody_core::client_enrollment::TestSessionPurposeV1::Read
                    }
                    _ => return Err(denied()),
                };
                if !purposes.insert(c.purpose)
                    || c.generation.0 <= s.floor(c.purpose)?
                    || !claim.purposes.contains(&wanted)
                    || !self.attempts.iter().any(|a| {
                        a.original_id == c.claim_request_id
                            && a.retained
                            && a.profile == Some(HttpPopProfileV1::CredentialClaim)
                    })
                    || claim
                        .expected_generation
                        .map_or(Some(1), |g| g.0.checked_add(1))
                        != Some(c.generation.0)
                {
                    return Err(denied());
                }
            }
            for (id, c) in &s.claims {
                c.validate().map_err(|_| denied())?;
                let l = s.logicals.get(id).ok_or_else(denied)?;
                if c.request_id != *id
                    || c.test_session_id != s.binding.session_id
                    || c.enrollment_id != s.binding.pairing.enrollment_id
                    || l.kind.is_some()
                    || l.digest != c.test_session_digest().map_err(|_| denied())?
                {
                    return Err(denied());
                }
            }
            for (id, l) in &s.logicals {
                if !super::id(id)
                    || l.created_at > self.time_floor
                    || l.digest.len() != 64
                    || hex::decode(&l.digest).is_err()
                    || hex::encode(hex::decode(&l.digest).map_err(|_| denied())?) != l.digest
                {
                    return Err(denied());
                }
                if let Some(kind) = l.kind {
                    let request = oasis7_custody_core::provisioning::SessionControlRequestV1 {
                        api_version: 1,
                        session_id: s.binding.session_id.clone(),
                        request_id: id.clone(),
                        kind,
                    };
                    if request.digest().map_err(|_| denied())? != l.digest {
                        return Err(denied());
                    }
                } else if !s.claims.contains_key(id) {
                    return Err(denied());
                }
                if let Some(a) = &l.ack
                    && (!l.retained
                        || a.request_id != *id
                        || Some(a.kind) != l.kind
                        || !self.attempts.iter().any(|attempt| {
                            attempt.original_id == *id
                                && attempt.retained
                                && attempt.profile == Some(HttpPopProfileV1::Custody)
                        }))
                {
                    return Err(denied());
                }
            }
        } else if live != 0 {
            return Err(denied());
        }
        Ok(())
    }
}
