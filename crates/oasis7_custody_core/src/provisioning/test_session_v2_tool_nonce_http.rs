use super::*;
use crate::http_pop::{
    CanonicalHttpTarget, HttpHeader, HttpPopBinding, HttpPopMessage, HttpPopRoute, PreparedHttpPop,
    prepare_http_pop,
};
impl AccountProvisionApprovalState {
    pub fn prepare_test_session_v2_tool_nonce_http(
        &self,
        request: &SessionClientHttpRequestV1,
        context: &SessionClientHttpContext<'_>,
        now: UnixMillis,
    ) -> Result<PreparedHttpPop> {
        let s = self.test_session_v2_record(&request.session_id)?;
        native_registration(s)?;
        if context.operation != SessionClientOperation::ToolNonce
            || context.public_origin != s.enrollment.registration.public_origin
            || context.custody_account_id != self.draft.entry.custody_account_id
            || context.provision_id != self.draft.entry.provision_id
            || self.session_inventory != SessionInventoryV1::Known
            || s.approved_at.is_none()
            || request.headers.len() > crate::http_pop::MAX_HTTP_HEADER_INSTANCES
            || request.body.len() > crate::http_pop::MAX_HTTP_BODY_BYTES
            || request.nonce_hint.is_some()
        {
            return Err(invalid());
        }
        let q: SessionChallengeRequestV1 =
            serde_json::from_str(&request.body).map_err(|_| invalid())?;
        request_shape(&q, &request.session_id)?;
        let root = s.roots.get(&q.purpose).ok_or_else(invalid)?;
        if root.approved_input_digest != s.input_digest()?
            || root.revoked
            || now.get() >= root.deadline.get()
        {
            return Err(invalid());
        }
        let credential = root.credential.as_ref().ok_or_else(invalid)?;
        let token = roots::hex_bytes(&credential.digest)?;
        let pair = &s.header()?.1.client_pairing;
        let target = CanonicalHttpTarget::new(
            context.public_origin,
            HttpPopRoute::ScopedClient {
                custody_account_id: self.draft.entry.custody_account_id.clone(),
                provision_id: self.draft.entry.provision_id.clone(),
                route: Box::new(HttpPopRoute::ProofChallenges),
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
        prepare_http_pop(
            &HttpPopMessage {
                method: &request.method,
                target_uri: &request.target_uri,
                headers: &headers,
                representation: context.representation,
                body: request.body.as_bytes(),
            },
            &HttpPopBinding {
                profile: HttpPopProfileV1::CustodyChallenge,
                target: &target,
                key_id: &pair.client_key_id,
                public_key_hex: &pair.client_public_key,
                binding_id: &s.input.operation_id,
                request_id: &q.request_id,
                nonce: None,
                credential_digest: &token,
                valid_until: root.deadline,
            },
            now,
        )
        .map_err(|_| invalid())
    }
}
