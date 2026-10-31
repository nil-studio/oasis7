//! Trusted H-only metadata/bootstrap and explicit same-key replacement.
use super::*;
use oasis7_custody_core::{
    client_enrollment::*,
    provisioning::{SessionChallengeKindV1, SessionChallengeRequestV1},
};
impl Host {
    pub(super) fn enroll(&self) -> Result<ClientEnrollmentProofResponseV1, String> {
        let original = self.vault.read(|s| Ok(s.enrollment.original.clone()))?;
        if self.vault.read(|s| Ok(s.enrollment.response.is_none()))? {
            let body = serde_json::to_vec(&original).map_err(|_| denied())?;
            // This path is the only unsigned metadata create and is not a
            // business grant. Its conservative local bootstrap cap is eight.
            self.vault.reserve_create(&original.request_id, &body)?;
            let reply = self.transport.create_enrollment_once(&original)?;
            if reply.status != 200 && reply.status != 201 {
                return Err(denied());
            }
            let response: ClientEnrollmentCreateResponseV1 =
                serde_json::from_slice(reply.body()).map_err(|_| denied())?;
            response.validate().map_err(|_| denied())?;
            if response.descriptor_digest != original.descriptor.digest().map_err(|_| denied())?
                || response.expires_at.get() <= vault::now()?
            {
                return Err(denied());
            }
            self.vault.update(|s| {
                if s.enrollment
                    .response
                    .as_ref()
                    .is_some_and(|old| old != &response)
                {
                    return Err(denied());
                }
                s.enrollment.response = Some(response.clone());
                Ok(())
            })?;
        }
        let proof = self.vault.update(|s| {
            if let Some(old) = &s.enrollment.proof {
                return Ok(old.clone());
            }
            let r = s.enrollment.response.as_ref().ok_or_else(denied)?;
            let p = ClientEnrollmentProofRequestV1 {
                target: s.identity.target.clone(),
                client_public_key: s.identity.descriptor.client_public_key.clone(),
                server_nonce: r.server_nonce.clone(),
                challenge_expires_at: r.challenge_expires_at,
                api_version: 1,
                enrollment_id: r.enrollment_id.clone(),
                request_id: vault::random_id()?,
                descriptor_digest: r.descriptor_digest.clone(),
            };
            p.validate().map_err(|_| denied())?;
            s.enrollment.proof = Some(p.clone());
            Ok(p)
        })?;
        if self.vault.read(|s| Ok(s.enrollment.proved))? {
            return Ok(ClientEnrollmentProofResponseV1 {
                api_version: 1,
                enrollment_id: proof.enrollment_id,
                status: ClientEnrollmentProofStatusV1::Proved,
            });
        }
        let body = serde_json::to_vec(&proof).map_err(|_| denied())?;
        let response = self.signed(SignedSend {
            route: HttpPopRoute::EnrollmentProve(proof.enrollment_id.clone()),
            profile: HttpPopProfileV1::EnrollmentProof,
            request_id: &proof.request_id,
            nonce: Some(&proof.server_nonce),
            body: &body,
            purpose: None,
            expected_generation: None,
            deadline: proof.challenge_expires_at.get(),
        })?;
        if response.status != 200 {
            return Err(denied());
        }
        let proved: ClientEnrollmentProofResponseV1 =
            serde_json::from_slice(response.body()).map_err(|_| denied())?;
        proved.validate().map_err(|_| denied())?;
        if proved.enrollment_id != proof.enrollment_id {
            return Err(denied());
        }
        self.vault.update(|s| {
            s.enrollment.proved = true;
            Ok(())
        })?;
        Ok(proved)
    }
    pub(super) fn claim(&self, request: ClientCredentialClaimRequestV1) -> Result<bool, String> {
        request.validate().map_err(|_| denied())?;
        if request
            .expected_generation
            .is_some_and(|g| !(1..=3).contains(&g.0))
        {
            return Err(denied());
        }
        let body = serde_json::to_vec(&request).map_err(|_| denied())?;
        let body_digest = request.test_session_digest().map_err(|_| denied())?;
        let purpose = if request.purposes.contains(&TestSessionPurposeV1::Execute) {
            SessionPurposeV1::Execution
        } else {
            SessionPurposeV1::ReconcileRead
        };
        let target = self
            .target(self.scoped(HttpPopRoute::EnrollmentClaim(request.enrollment_id.clone()))?)?;
        self.vault.update(|s| {
            let session = s.session.as_mut().ok_or_else(denied)?;
            if request.enrollment_id != session.binding.pairing.enrollment_id
                || request.test_session_id != session.binding.session_id
            {
                return Err(denied());
            }
            if let Some(old) = session.logicals.get(&request.request_id) {
                return if old.digest == body_digest && old.kind.is_none() {
                    Ok(())
                } else {
                    Err(denied())
                };
            }
            if request.replace {
                for p in &request.purposes {
                    let purpose = match p {
                        TestSessionPurposeV1::Execute => SessionPurposeV1::Execution,
                        TestSessionPurposeV1::Read => SessionPurposeV1::ReconcileRead,
                        _ => return Err(denied()),
                    };
                    if let Ok(old) = session.credential(purpose) {
                        if request
                            .expected_generation
                            .is_none_or(|g| g.0 < old.generation.0)
                        {
                            return Err(denied());
                        }
                        let retired = old.generation.0;
                        match purpose {
                            SessionPurposeV1::Execution => {
                                session.retired_execution = session.retired_execution.max(retired)
                            }
                            SessionPurposeV1::ReconcileRead => {
                                session.retired_read = session.retired_read.max(retired)
                            }
                            _ => return Err(denied()),
                        }
                    }
                    // Unknown replacement conservatively stops using the old
                    // secret. A failed send never restores it or lowers HWM.
                    session.credentials.retain(|c| c.purpose != purpose);
                }
            }
            session
                .claims
                .insert(request.request_id.clone(), request.clone());
            session.logicals.insert(
                request.request_id.clone(),
                Logical {
                    digest: body_digest.clone(),
                    kind: None,
                    created_at: vault::now()?,
                    ack: None,
                    retained: false,
                },
            );
            Ok(())
        })?;
        self.vault.retain(&request.request_id, &target, &body)?;
        let (sid, deadline) = self.vault.read(|s| {
            let b = &s.session.as_ref().ok_or_else(denied)?.binding;
            Ok((b.session_id.clone(), b.deadline(purpose)?))
        })?;
        let challenge_request = SessionChallengeRequestV1 {
            api_version: 1,
            session_id: sid.clone(),
            request_id: vault::random_id()?,
            purpose,
            kind: SessionChallengeKindV1::CredentialClaim,
            logical_request_id: request.request_id.clone(),
            operation_request_digest: body_digest,
        };
        let bytes = serde_json::to_vec(&challenge_request).map_err(|_| denied())?;
        let response = self.signed(SignedSend {
            route: self.scoped(HttpPopRoute::ProofChallenges)?,
            profile: HttpPopProfileV1::ClaimChallenge,
            request_id: &challenge_request.request_id,
            nonce: None,
            body: &bytes,
            purpose: Some(purpose),
            expected_generation: None,
            deadline,
        })?;
        if response.status != 200 {
            return Err(denied());
        }
        let c = reply::challenge(
            response.body(),
            &challenge_request,
            request.expected_generation,
            deadline,
        )?;
        let response = self.signed(SignedSend {
            route: self.scoped(HttpPopRoute::EnrollmentClaim(request.enrollment_id.clone()))?,
            profile: HttpPopProfileV1::CredentialClaim,
            request_id: &request.request_id,
            nonce: Some(&c.nonce),
            body: &body,
            purpose: Some(purpose),
            expected_generation: Some(c.credential_generation),
            deadline: deadline.min(c.expires_at.get()),
        })?;
        if response.status != 200 {
            return Err(denied());
        }
        let reply: reply::ClaimReply =
            serde_json::from_slice(response.body()).map_err(|_| denied())?;
        self.vault.update(|s| reply.install(s, &request))
    }
}
