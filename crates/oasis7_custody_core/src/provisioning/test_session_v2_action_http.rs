//! Closed V2 action HTTP preparation only. It is not an action admission.
use super::*;
use crate::http_pop::{
    CanonicalHttpTarget, HttpHeader, HttpPopBinding, HttpPopMessage, HttpPopRoute, PreparedHttpPop,
    prepare_http_pop,
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionActionRequestV2 {
    pub api_version: u8,
    pub session_id: String,
    pub request_id: String,
    pub action: SessionToolActionV1,
}
impl core::fmt::Debug for SessionActionRequestV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionActionRequestV2(<public-action>)")
    }
}
impl SessionActionRequestV2 {
    pub fn digest(&self) -> Result<String> {
        digest(&("oasis7.custody.test-session-action-request.v2", self))
    }
    pub fn purpose(&self) -> SessionPurposeV1 {
        match self.action {
            SessionToolActionV1::Execute(_) => SessionPurposeV1::Execution,
            SessionToolActionV1::ReconcileOperation { .. }
            | SessionToolActionV1::ReadCloseState => SessionPurposeV1::ReconcileRead,
        }
    }
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
pub(in crate::provisioning) fn action_original_v2(
    proof: &SessionProofRequest<'_>,
) -> Result<SessionActionRequestV2> {
    let request: SessionActionRequestV2 = lifecycle::original(proof)?;
    if request.api_version != 2
        || !id(&request.request_id)
        || !id(&request.session_id)
        || proof.possession.request_id() != request.request_id
    {
        return Err(invalid());
    }
    Ok(request)
}
impl AccountProvisionApprovalState {
    pub fn prepare_test_session_v2_action_http(
        &self,
        request: &SessionClientHttpRequestV1,
        representation: crate::http_pop::HttpHeaderRepresentation,
        now: UnixMillis,
    ) -> Result<PreparedHttpPop> {
        let s = self.test_session_v2_record(&request.session_id)?;
        super::v2_credential_http::native_registration(s)?;
        if request.headers.len() > crate::http_pop::MAX_HTTP_HEADER_INSTANCES
            || request.body.len() > crate::http_pop::MAX_HTTP_BODY_BYTES
            || self.session_inventory != SessionInventoryV1::Known
            || s.approved_at.is_none()
        {
            return Err(invalid());
        }
        let q: SessionActionRequestV2 =
            serde_json::from_str(&request.body).map_err(|_| invalid())?;
        if q.api_version != 2 || q.session_id != request.session_id || !id(&q.request_id) {
            return Err(invalid());
        }
        let root = s.roots.get(&q.purpose()).ok_or_else(invalid)?;
        if root.approved_input_digest != s.input_digest()?
            || root.revoked
            || now.get() >= root.deadline.get()
        {
            return Err(invalid());
        }
        let token = roots::hex_bytes(&root.credential.as_ref().ok_or_else(invalid)?.digest)?;
        let nonce = request.nonce_hint.as_deref().ok_or_else(invalid)?;
        let challenge = s
            .challenges
            .values()
            .find(|c| c.nonce == nonce)
            .ok_or_else(invalid)?;
        if challenge.kind != SessionChallengeKindV1::ToolMetadata
            || challenge.purpose != q.purpose()
            || challenge.logical_request_id != q.request_id
            || challenge.body_digest != q.digest()?
            || (challenge.consumed_by.is_none()
                && challenge.credential_generation
                    != root.credential.as_ref().ok_or_else(invalid)?.generation)
        {
            return Err(invalid());
        }
        let pair = &s.header()?.1.client_pairing;
        let target = CanonicalHttpTarget::new(
            &s.enrollment.registration.public_origin,
            HttpPopRoute::ScopedClient {
                custody_account_id: self.draft.entry.custody_account_id.clone(),
                provision_id: self.draft.entry.provision_id.clone(),
                route: Box::new(HttpPopRoute::Actions),
            },
        )
        .map_err(|_| invalid())?;
        let headers: Vec<_> = request
            .headers
            .iter()
            .map(|h| HttpHeader {
                name: &h.name,
                value: &h.value,
            })
            .collect();
        let until = if challenge.consumed_by.is_some() {
            root.deadline
        } else {
            UnixMillis::new(root.deadline.get().min(challenge.expires_at.get()))
        };
        prepare_http_pop(
            &HttpPopMessage {
                method: &request.method,
                target_uri: &request.target_uri,
                headers: &headers,
                representation,
                body: request.body.as_bytes(),
            },
            &HttpPopBinding {
                profile: HttpPopProfileV1::Custody,
                target: &target,
                key_id: &pair.client_key_id,
                public_key_hex: &pair.client_public_key,
                binding_id: &s.input.operation_id,
                request_id: &q.request_id,
                nonce: Some(nonce),
                credential_digest: &token,
                valid_until: until,
            },
            now,
        )
        .map_err(|_| invalid())
    }
}
