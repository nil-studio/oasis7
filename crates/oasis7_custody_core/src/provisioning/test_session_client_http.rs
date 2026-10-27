//! Private raw transport only. No JSON Current, key, token digest or verified flag.
use super::*;
use crate::http_pop::{
    CanonicalHttpTarget, HttpHeader, HttpHeaderRepresentation, HttpPopBinding, HttpPopMessage,
    HttpPopRoute, PreparedHttpPop, prepare_http_pop,
};
use sha2::{Digest, Sha256};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionClientHeaderV1 {
    pub name: String,
    pub value: String,
}
impl core::fmt::Debug for SessionClientHeaderV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionClientHeaderV1(<redacted>)")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionClientHttpRequestV1 {
    pub session_id: String,
    pub method: String,
    pub target_uri: String,
    pub headers: Vec<SessionClientHeaderV1>,
    pub body: String,
    pub nonce_hint: Option<String>,
}
impl core::fmt::Debug for SessionClientHttpRequestV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionClientHttpRequestV1(<redacted>)")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionClientOperation {
    Challenge,
    CredentialClaim,
    Control(SessionControlKindV1),
}
pub struct SessionClientHttpContext<'a> {
    pub public_origin: &'a str,
    pub custody_account_id: &'a str,
    pub provision_id: &'a str,
    pub operation: SessionClientOperation,
    pub representation: HttpHeaderRepresentation,
}
/// Reconstructed only by a fixed trusted ingress adapter; never deserialized from client JSON.
pub struct SessionHttpAdmission<'a> {
    pub ingress_attempt_id: &'a str,
    pub possession: &'a HttpPossessionReceipt,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionClientHttpAdmissionV1 {
    pub signed_attempt_digest: String,
    pub hop: SessionControlHopV1,
    pub charged_at: UnixMillis,
}
impl AccountProvisionApprovalState {
    /// Pure preparation from actual Account originals. This neither charges nor grants authority.
    pub fn prepare_test_session_client_http(
        &self,
        request: &SessionClientHttpRequestV1,
        context: &SessionClientHttpContext<'_>,
        now: UnixMillis,
    ) -> Result<PreparedHttpPop> {
        if context.custody_account_id != self.draft.entry.custody_account_id
            || context.provision_id != self.draft.entry.provision_id
            || request.headers.len() > crate::http_pop::MAX_HTTP_HEADER_INSTANCES
            || request.body.len() > crate::http_pop::MAX_HTTP_BODY_BYTES
        {
            return Err(invalid());
        }
        let s = self
            .test_sessions
            .get(&request.session_id)
            .ok_or_else(invalid)?;
        if s.approved_at.is_none() || self.session_inventory != SessionInventoryV1::Known {
            return Err(invalid());
        }
        let (_, pair, _, _, _) = admission::pairing(&s.input.request.intent)?;
        let (profile, route, request_id, purpose, selected) = match context.operation {
            SessionClientOperation::Challenge => {
                let q: SessionChallengeRequestV1 =
                    serde_json::from_str(&request.body).map_err(|_| invalid())?;
                if q.api_version != 1
                    || q.session_id != request.session_id
                    || !id(&q.request_id)
                    || !id(&q.logical_request_id)
                    || !hex64(&q.operation_request_digest)
                    || request.nonce_hint.is_some()
                    || q.purpose == SessionPurposeV1::TerminationPlan
                    || q.kind == SessionChallengeKindV1::Tool
                {
                    return Err(invalid());
                }
                let profile = if q.kind == SessionChallengeKindV1::CredentialClaim {
                    HttpPopProfileV1::ClaimChallenge
                } else {
                    HttpPopProfileV1::CustodyChallenge
                };
                (
                    profile,
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
                let c = client_challenge(
                    s,
                    request,
                    SessionChallengeKindV1::CredentialClaim,
                    &q.request_id,
                    &q.test_session_digest()?,
                    now,
                )?;
                (
                    HttpPopProfileV1::CredentialClaim,
                    HttpPopRoute::EnrollmentClaim(pair.enrollment_id.clone()),
                    q.request_id,
                    c.purpose,
                    Some(c),
                )
            }
            SessionClientOperation::Control(kind) => {
                let q: SessionControlRequestV1 =
                    serde_json::from_str(&request.body).map_err(|_| invalid())?;
                if q.api_version != 1
                    || !id(&q.request_id)
                    || q.session_id != request.session_id
                    || q.kind != kind
                {
                    return Err(invalid());
                }
                let c = client_challenge(
                    s,
                    request,
                    SessionChallengeKindV1::Control(kind),
                    &q.request_id,
                    &q.digest()?,
                    now,
                )?;
                let route = match kind {
                    SessionControlKindV1::Initialize => HttpPopRoute::Initialize(q.session_id),
                    SessionControlKindV1::ToolsList => HttpPopRoute::ToolsList(q.session_id),
                    SessionControlKindV1::Heartbeat => HttpPopRoute::Heartbeat(q.session_id),
                    SessionControlKindV1::Finish => HttpPopRoute::Finish(q.session_id),
                    SessionControlKindV1::Abort => HttpPopRoute::Abort(q.session_id),
                };
                (
                    HttpPopProfileV1::Custody,
                    route,
                    q.request_id,
                    c.purpose,
                    Some(c),
                )
            }
        };
        let root = s.roots.get(&purpose).ok_or_else(invalid)?;
        if purpose == SessionPurposeV1::TerminationPlan || root.revoked {
            return Err(invalid());
        }
        if selected.is_some_and(|c| {
            c.consumed_by.is_none()
                && matches!(c.kind, SessionChallengeKindV1::Control(_))
                && root
                    .credential
                    .as_ref()
                    .is_none_or(|credential| c.credential_generation != credential.generation)
        }) {
            return Err(invalid());
        }
        let token_digest: [u8; 32] = if matches!(
            profile,
            HttpPopProfileV1::ClaimChallenge | HttpPopProfileV1::CredentialClaim
        ) {
            Sha256::digest([]).into()
        } else {
            roots::hex_bytes(&root.credential.as_ref().ok_or_else(invalid)?.digest)?
        };
        let valid_until = selected
            .filter(|c| c.consumed_by.is_none())
            .map_or(root.deadline, |c| {
                UnixMillis::new(root.deadline.get().min(c.expires_at.get()))
            });
        let route = HttpPopRoute::ScopedClient {
            provision_id: self.draft.entry.provision_id.clone(),
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            route: Box::new(route),
        };
        let target =
            CanonicalHttpTarget::new(context.public_origin, route).map_err(|_| invalid())?;
        let headers = request
            .headers
            .iter()
            .map(|h| HttpHeader {
                name: &h.name,
                value: &h.value,
            })
            .collect::<Vec<_>>();
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
                credential_digest: &token_digest,
                valid_until,
            },
            now,
        )
        .map_err(|_| invalid())
    }
}
fn client_challenge<'a>(
    s: &'a TestSessionAuthorityV1,
    request: &SessionClientHttpRequestV1,
    kind: SessionChallengeKindV1,
    logical: &str,
    body_digest: &str,
    now: UnixMillis,
) -> Result<&'a SessionChallengeV1> {
    let nonce = request.nonce_hint.as_deref().ok_or_else(invalid)?;
    let c = s
        .challenges
        .values()
        .find(|c| c.nonce == nonce)
        .ok_or_else(invalid)?;
    if c.kind != kind
        || c.logical_request_id != logical
        || c.body_digest != body_digest
        || c.consumed_by.as_ref().is_some_and(|id| id != logical)
        || (c.consumed_by.is_none() && now.get() >= c.expires_at.get())
    {
        return Err(invalid());
    }
    Ok(c)
}
