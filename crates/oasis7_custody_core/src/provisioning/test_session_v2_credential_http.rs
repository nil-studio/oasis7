//! Fixed Native V2 claim ingress. Preparation is not PoP success or payment.
use super::*;
use crate::http_pop::{
    CanonicalHttpTarget, HttpHeader, HttpPopBinding, HttpPopMessage, HttpPopRoute, PreparedHttpPop,
    prepare_http_pop,
};
use sha2::{Digest, Sha256};

pub(super) fn native_registration(s: &TestSessionAuthorityV2) -> Result<()> {
    let descriptor = &s.enrollment.registration.request.descriptor;
    let scope = s.header()?.1;
    let pair = &scope.client_pairing;
    if descriptor.transport_profile != HostTransportProfileV1::NativeBrokerCustodyV1
        || pair.connector_public_key.is_some()
        || pair.client_public_key != descriptor.client_public_key
        || !matches!(&scope.client_binding, crate::approval::ApprovedClientBindingV1::NativeBrokerCustodyV1 { client_key }
            if client_key == &pair.client_public_key)
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn public_receipt(
    s: &TestSessionAuthorityV2,
    proof: &HttpPossessionReceipt,
    now: UnixMillis,
) -> Result<()> {
    native_registration(s)?;
    let pair = &s.header()?.1.client_pairing;
    if proof.binding_id() != s.input.operation_id
        || proof.key_id() != pair.client_key_id
        || proof.public_key_hex() != pair.client_public_key
        || !matches!(
            proof.profile(),
            HttpPopProfileV1::ClaimChallenge | HttpPopProfileV1::CredentialClaim
        )
        || proof.token_digest().as_slice() != Sha256::digest([]).as_slice()
        || proof
            .expires()
            .checked_millis()
            .map_err(|_| invalid())?
            .get()
            <= now.get()
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn control_exhausted(s: &TestSessionAuthorityV2) -> bool {
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
pub(super) fn close_if_control_exhausted(s: &mut TestSessionAuthorityV2, now: UnixMillis) {
    if (control_exhausted(s) || super::source_v2::v2_purpose_exhausted(s)) && s.closing.is_none() {
        s.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
    }
}
fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
impl AccountProvisionApprovalState {
    pub fn prepare_test_session_v2_credential_http(
        &self,
        request: &SessionClientHttpRequestV1,
        context: &SessionClientHttpContext<'_>,
        now: UnixMillis,
    ) -> Result<PreparedHttpPop> {
        let s = self.test_session_v2_record(&request.session_id)?;
        native_registration(s)?;
        if context.custody_account_id != self.draft.entry.custody_account_id
            || context.provision_id != self.draft.entry.provision_id
            || context.public_origin != s.enrollment.registration.public_origin
            || request.headers.len() > crate::http_pop::MAX_HTTP_HEADER_INSTANCES
            || request.body.len() > crate::http_pop::MAX_HTTP_BODY_BYTES
            || s.approved_at.is_none()
            || self.session_inventory != SessionInventoryV1::Known
        {
            return Err(invalid());
        }
        let pair = &s.header()?.1.client_pairing;
        let headers: Vec<_> = request
            .headers
            .iter()
            .map(|h| HttpHeader {
                name: &h.name,
                value: &h.value,
            })
            .collect();
        if crate::http_pop::http_pop_nonce_hint(&headers, context.representation)
            .map_err(|_| invalid())?
            != request.nonce_hint
        {
            return Err(invalid());
        }
        let (profile, route, request_id, purpose, selected) = match context.operation {
            SessionClientOperation::Challenge => {
                let q: SessionChallengeRequestV1 =
                    serde_json::from_str(&request.body).map_err(|_| invalid())?;
                if q.api_version != 1
                    || q.session_id != request.session_id
                    || !id(&q.request_id)
                    || !id(&q.logical_request_id)
                    || !hex64(&q.operation_request_digest)
                    || q.kind != SessionChallengeKindV1::CredentialClaim
                    || q.purpose == SessionPurposeV1::TerminationPlan
                    || request.nonce_hint.is_some()
                {
                    return Err(invalid());
                }
                (
                    HttpPopProfileV1::ClaimChallenge,
                    HttpPopRoute::ProofChallenges,
                    q.request_id,
                    q.purpose,
                    None,
                )
            }
            SessionClientOperation::CredentialClaim => {
                let q: ClientCredentialClaimRequestV1 =
                    serde_json::from_str(&request.body).map_err(|_| invalid())?;
                q.validate().map_err(|_| invalid())?;
                if q.test_session_id != request.session_id || q.enrollment_id != pair.enrollment_id
                {
                    return Err(invalid());
                }
                let nonce = request.nonce_hint.as_deref().ok_or_else(invalid)?;
                let challenge = s
                    .challenges
                    .values()
                    .find(|c| c.nonce == nonce)
                    .ok_or_else(invalid)?;
                if challenge.kind != SessionChallengeKindV1::CredentialClaim
                    || challenge.logical_request_id != q.request_id
                    || challenge.body_digest != q.test_session_digest()?
                    || challenge
                        .consumed_by
                        .as_ref()
                        .is_some_and(|old| old != &q.request_id)
                    || (challenge.consumed_by.is_none() && now.get() >= challenge.expires_at.get())
                    || !q.purposes.iter().any(|p| {
                        super::v2_credentials::credential_purpose_v2(p).ok()
                            == Some(challenge.purpose)
                    })
                {
                    return Err(invalid());
                }
                (
                    HttpPopProfileV1::CredentialClaim,
                    HttpPopRoute::EnrollmentClaim(pair.enrollment_id.clone()),
                    q.request_id,
                    challenge.purpose,
                    Some(challenge),
                )
            }
            _ => return Err(invalid()),
        };
        let root = s.roots.get(&purpose).ok_or_else(invalid)?;
        if root.revoked || root.approved_input_digest != s.input_digest()? {
            return Err(invalid());
        }
        let valid_until = selected
            .filter(|c| c.consumed_by.is_none())
            .map_or(root.deadline, |c| {
                UnixMillis::new(root.deadline.get().min(c.expires_at.get()))
            });
        let target = CanonicalHttpTarget::new(
            context.public_origin,
            HttpPopRoute::ScopedClient {
                provision_id: self.draft.entry.provision_id.clone(),
                custody_account_id: self.draft.entry.custody_account_id.clone(),
                route: Box::new(route),
            },
        )
        .map_err(|_| invalid())?;
        let empty_token_digest: [u8; 32] = Sha256::digest([]).into();
        prepare_http_pop(
            &HttpPopMessage {
                method: &request.method,
                target_uri: &request.target_uri,
                headers: &headers,
                representation: context.representation,
                body: request.body.as_bytes(),
            },
            &HttpPopBinding {
                profile,
                target: &target,
                key_id: &pair.client_key_id,
                public_key_hex: &pair.client_public_key,
                binding_id: &s.input.operation_id,
                request_id: &request_id,
                nonce: selected.map(|c| c.nonce.as_str()),
                credential_digest: &empty_token_digest,
                valid_until,
            },
            now,
        )
        .map_err(|_| invalid())
    }

    /// The Store must commit this in its first, independently durable SQL
    /// callback before any Source/private-history/Native observation begins.
    pub fn charge_test_session_v2_claim_http_attempt(
        &mut self,
        id: &str,
        admission: SessionHttpAdmission<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let proof = admission.possession;
        public_receipt(s, proof, now)?;
        if !matches!(
            proof.profile(),
            HttpPopProfileV1::ClaimChallenge | HttpPopProfileV1::CredentialClaim
        ) || !id_nonempty(admission.ingress_attempt_id)
        {
            return Err(invalid());
        }
        let signature = proof.transport_attempt_digest();
        if let Some(old) = s.client_http_admissions.get(admission.ingress_attempt_id) {
            return if old.signed_attempt_digest == signature
                && old.hop == SessionControlHopV1::Host
                && s.control_transports.get(admission.ingress_attempt_id)
                    == Some(&SessionControlHopV1::Host)
            {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.test_session_v2_transport_id_available(id, admission.ingress_attempt_id)?;
        if s.client_http_admissions.len() >= 240
            || self.frozen()
            || s.consumed_at.is_none()
            || !s.roots.iter().any(|(p, r)| {
                *p != SessionPurposeV1::TerminationPlan
                    && !r.revoked
                    && now.get() < r.deadline.get()
            })
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        let s = next.test_sessions_v2.get_mut(id).ok_or_else(invalid)?;
        s.control
            .as_mut()
            .ok_or_else(invalid)?
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        s.control_transports.insert(
            admission.ingress_attempt_id.into(),
            SessionControlHopV1::Host,
        );
        s.client_http_admissions.insert(
            admission.ingress_attempt_id.into(),
            SessionClientHttpAdmissionV1 {
                signed_attempt_digest: signature,
                hop: SessionControlHopV1::Host,
                charged_at: now,
            },
        );
        close_if_control_exhausted(s, now);
        append_session_event_v2(
            s,
            SessionEventKindV2::ClientHttpCharged {
                ingress_attempt_id: admission.ingress_attempt_id.into(),
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub(in crate::provisioning) fn require_test_session_v2_claim_http_paid(
        &self,
        id: &str,
        proof: &HttpPossessionReceipt,
        attempt: &str,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        if s.control_transports.get(attempt) != Some(&SessionControlHopV1::Host)
            || !s.client_http_admissions.get(attempt).is_some_and(|p| {
                p.hop == SessionControlHopV1::Host
                    && p.signed_attempt_digest == proof.transport_attempt_digest()
            })
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn id_nonempty(value: &str) -> bool {
    id(value)
}
