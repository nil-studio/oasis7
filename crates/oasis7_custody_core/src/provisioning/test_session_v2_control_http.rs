use super::*;
use crate::http_pop::{
    CanonicalHttpTarget, HttpHeader, HttpPopBinding, HttpPopMessage, HttpPopRoute, PreparedHttpPop,
    prepare_http_pop,
};
fn id(v: &str) -> bool {
    valid_id(v)
}
pub(super) fn authenticated<'a>(
    s: &'a TestSessionAuthorityV2,
    purpose: SessionPurposeV1,
    proof: &HttpPossessionReceipt,
    now: UnixMillis,
) -> Result<&'a SessionCredentialV1> {
    super::super::v2_credential_http::native_registration(s)?;
    let pair = &s.header()?.1.client_pairing;
    let root = s.roots.get(&purpose).ok_or_else(invalid)?;
    let credential = root.credential.as_ref().ok_or_else(invalid)?;
    let stored_digest = super::super::hex_bytes(&credential.digest)?;
    if purpose == SessionPurposeV1::TerminationPlan
        || root.approved_input_digest != s.input_digest()?
        || root.revoked
        || now.get() >= root.deadline.get()
        || proof.binding_id() != s.input.operation_id
        || proof.key_id() != pair.client_key_id
        || proof.public_key_hex() != pair.client_public_key
        || !matches!(
            proof.profile(),
            HttpPopProfileV1::CustodyChallenge | HttpPopProfileV1::Custody
        )
        || stored_digest != *proof.token_digest()
        || proof
            .expires()
            .checked_millis()
            .map_err(|_| invalid())?
            .get()
            <= now.get()
    {
        return Err(invalid());
    }
    Ok(credential)
}
pub(super) fn paid(
    state: &AccountProvisionApprovalState,
    id: &str,
    proof: &HttpPossessionReceipt,
    attempt: &str,
) -> Result<()> {
    state.require_test_session_v2_claim_http_paid(id, proof, attempt)
}
fn route(kind: SessionControlKindV1, id: String) -> HttpPopRoute {
    match kind {
        SessionControlKindV1::Initialize => HttpPopRoute::Initialize(id),
        SessionControlKindV1::ToolsList => HttpPopRoute::ToolsList(id),
        SessionControlKindV1::Heartbeat => HttpPopRoute::Heartbeat(id),
        SessionControlKindV1::Finish => HttpPopRoute::Finish(id),
        SessionControlKindV1::Abort => HttpPopRoute::Abort(id),
    }
}
impl AccountProvisionApprovalState {
    pub fn prepare_test_session_v2_control_http(
        &self,
        q: &SessionClientHttpRequestV1,
        c: &SessionClientHttpContext<'_>,
        now: UnixMillis,
    ) -> Result<PreparedHttpPop> {
        let s = self.test_session_v2_record(&q.session_id)?;
        super::super::v2_credential_http::native_registration(s)?;
        if c.custody_account_id != self.draft.entry.custody_account_id
            || c.provision_id != self.draft.entry.provision_id
            || c.public_origin != s.enrollment.registration.public_origin
            || s.approved_at.is_none()
            || s.roots.len() != 3
            || self.session_inventory != SessionInventoryV1::Known
            || q.headers.len() > crate::http_pop::MAX_HTTP_HEADER_INSTANCES
            || q.body.len() > crate::http_pop::MAX_HTTP_BODY_BYTES
        {
            return Err(invalid());
        }
        let hs: Vec<_> = q
            .headers
            .iter()
            .map(|h| HttpHeader {
                name: &h.name,
                value: &h.value,
            })
            .collect();
        if crate::http_pop::http_pop_nonce_hint(&hs, c.representation).map_err(|_| invalid())?
            != q.nonce_hint
        {
            return Err(invalid());
        }
        let (profile, tail, request_id, purpose, challenge) = match c.operation {
            SessionClientOperation::Challenge => {
                let r: SessionChallengeRequestV1 =
                    serde_json::from_str(&q.body).map_err(|_| invalid())?;
                let SessionChallengeKindV1::Control(kind) = r.kind else {
                    return Err(invalid());
                };
                if r.api_version != 1
                    || r.session_id != q.session_id
                    || !id(&r.request_id)
                    || !id(&r.logical_request_id)
                    || !hex64(&r.operation_request_digest)
                    || q.nonce_hint.is_some()
                    || r.purpose == SessionPurposeV1::TerminationPlan
                    || kind == SessionControlKindV1::Heartbeat
                        && r.purpose != SessionPurposeV1::Execution
                {
                    return Err(invalid());
                }
                (
                    HttpPopProfileV1::CustodyChallenge,
                    HttpPopRoute::ProofChallenges,
                    r.request_id,
                    r.purpose,
                    None,
                )
            }
            SessionClientOperation::Control(kind) => {
                let r: SessionControlRequestV1 =
                    serde_json::from_str(&q.body).map_err(|_| invalid())?;
                if r.api_version != 1
                    || r.session_id != q.session_id
                    || r.kind != kind
                    || !id(&r.request_id)
                {
                    return Err(invalid());
                }
                let nonce = q.nonce_hint.as_deref().ok_or_else(invalid)?;
                let v = s
                    .challenges
                    .values()
                    .find(|v| v.nonce == nonce)
                    .ok_or_else(invalid)?;
                if v.kind != SessionChallengeKindV1::Control(kind)
                    || v.logical_request_id != r.request_id
                    || v.body_digest != r.digest()?
                    || v.consumed_by.as_ref().is_some_and(|x| x != &r.request_id)
                    || v.consumed_by.is_none() && now.get() >= v.expires_at.get()
                {
                    return Err(invalid());
                }
                (
                    HttpPopProfileV1::Custody,
                    route(kind, q.session_id.clone()),
                    r.request_id,
                    v.purpose,
                    Some(v),
                )
            }
            _ => return Err(invalid()),
        };
        let root = s.roots.get(&purpose).ok_or_else(invalid)?;
        let credential = root.credential.as_ref().ok_or_else(invalid)?;
        if root.revoked
            || root.approved_input_digest != s.input_digest()?
            || purpose == SessionPurposeV1::TerminationPlan
        {
            return Err(invalid());
        }
        if challenge.is_some_and(|v| {
            v.consumed_by.is_none() && v.credential_generation != credential.generation
        }) {
            return Err(invalid());
        }
        let until = challenge
            .filter(|v| v.consumed_by.is_none())
            .map_or(root.deadline, |v| {
                UnixMillis::new(root.deadline.get().min(v.expires_at.get()))
            });
        let target = CanonicalHttpTarget::new(
            c.public_origin,
            HttpPopRoute::ScopedClient {
                provision_id: self.draft.entry.provision_id.clone(),
                custody_account_id: self.draft.entry.custody_account_id.clone(),
                route: Box::new(tail),
            },
        )
        .map_err(|_| invalid())?;
        let token = super::super::hex_bytes(&credential.digest)?;
        prepare_http_pop(
            &HttpPopMessage {
                method: &q.method,
                target_uri: &q.target_uri,
                headers: &hs,
                representation: c.representation,
                body: q.body.as_bytes(),
            },
            &HttpPopBinding {
                profile,
                target: &target,
                key_id: &s.header()?.1.client_pairing.client_key_id,
                public_key_hex: &s.header()?.1.client_pairing.client_public_key,
                binding_id: &s.input.operation_id,
                request_id: &request_id,
                nonce: challenge.map(|v| v.nonce.as_str()),
                credential_digest: &token,
                valid_until: until,
            },
            now,
        )
        .map_err(|_| invalid())
    }
    pub fn charge_test_session_v2_control_http_attempt(
        &mut self,
        id: &str,
        a: SessionHttpAdmission<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        if authenticated(s, SessionPurposeV1::Execution, a.possession, now).is_err() {
            authenticated(s, SessionPurposeV1::ReconcileRead, a.possession, now)?;
        }
        if !valid_id(a.ingress_attempt_id) {
            return Err(invalid());
        }
        let signature = a.possession.transport_attempt_digest();
        if let Some(old) = s.client_http_admissions.get(a.ingress_attempt_id) {
            return if old.signed_attempt_digest == signature
                && old.hop == SessionControlHopV1::Host
                && s.control_transports.get(a.ingress_attempt_id)
                    == Some(&SessionControlHopV1::Host)
            {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.test_session_v2_transport_id_available(id, a.ingress_attempt_id)?;
        if self.frozen()
            || s.consumed_at.is_none()
            || s.approved_at.is_none()
            || s.client_http_admissions.len() >= 240
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
        s.control_transports
            .insert(a.ingress_attempt_id.into(), SessionControlHopV1::Host);
        s.client_http_admissions.insert(
            a.ingress_attempt_id.into(),
            SessionClientHttpAdmissionV1 {
                signed_attempt_digest: signature,
                hop: SessionControlHopV1::Host,
                charged_at: now,
            },
        );
        super::super::v2_credential_http::close_if_control_exhausted(s, now);
        append_session_event_v2(
            s,
            SessionEventKindV2::ClientHttpCharged {
                ingress_attempt_id: a.ingress_attempt_id.into(),
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
