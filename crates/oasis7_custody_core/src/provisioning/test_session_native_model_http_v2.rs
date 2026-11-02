//! N query + Host Execution PoP. This is not a five-tool profile or dispatch.
use super::*;
use crate::time_units::UnixMillis;
use crate::{
    http_pop::{
        CanonicalHttpTarget, HttpHeader, HttpPopBinding, HttpPopMessage, HttpPopRoute,
        PreparedHttpPop, prepare_http_pop,
    },
    native_model_proxy_query_v2::SignedNativeModelReservationQueryV2,
};
pub struct NativeModelHttpContextV2<'a> {
    pub public_origin: &'a str,
    pub custody_account_id: &'a str,
    pub provision_id: &'a str,
    pub representation: crate::http_pop::HttpHeaderRepresentation,
}
impl AccountProvisionApprovalState {
    pub fn prepare_test_session_v2_native_model_reserve_http(
        &self,
        q: &SessionClientHttpRequestV1,
        c: &NativeModelHttpContextV2<'_>,
        now: UnixMillis,
    ) -> Result<PreparedHttpPop> {
        let s = self.test_session_v2_record(&q.session_id)?;
        super::v2_credential_http::native_registration(s)?;
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
        let request: SignedNativeModelReservationQueryV2 =
            serde_json::from_str(&q.body).map_err(|_| invalid())?;
        // Stored, truly consumed V2 input supplies the approved N instance key.
        request
            .verify_input_binding(&s.input, now.get())
            .map_err(|_| invalid())?;
        if request.query.session_id != q.session_id
            || q.nonce_hint.as_deref() != Some(request.query.query_nonce.as_str())
        {
            return Err(invalid());
        }
        let root = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        let credential = root.credential.as_ref().ok_or_else(invalid)?;
        if root.revoked || root.approved_input_digest != s.input_digest()? {
            return Err(invalid());
        }
        self.test_session_v2_local(&q.session_id, SessionPurposeV1::Execution, now)?;
        let headers: Vec<_> = q
            .headers
            .iter()
            .map(|h| HttpHeader {
                name: &h.name,
                value: &h.value,
            })
            .collect();
        if crate::http_pop::http_pop_nonce_hint(&headers, c.representation)
            .map_err(|_| invalid())?
            != q.nonce_hint
        {
            return Err(invalid());
        }
        let target = CanonicalHttpTarget::new(
            c.public_origin,
            HttpPopRoute::ScopedClient {
                provision_id: self.draft.entry.provision_id.clone(),
                custody_account_id: self.draft.entry.custody_account_id.clone(),
                route: Box::new(HttpPopRoute::ModelReservations(q.session_id.clone())),
            },
        )
        .map_err(|_| invalid())?;
        let pair = &s.header()?.1.client_pairing;
        let digest = super::hex_bytes(&credential.digest)?;
        prepare_http_pop(
            &HttpPopMessage {
                method: &q.method,
                target_uri: &q.target_uri,
                headers: &headers,
                representation: c.representation,
                body: q.body.as_bytes(),
            },
            &HttpPopBinding {
                profile: HttpPopProfileV1::NativeModelCustodyV2,
                target: &target,
                key_id: &pair.client_key_id,
                public_key_hex: &pair.client_public_key,
                binding_id: &q.session_id,
                request_id: &request.query.request_id,
                nonce: Some(&request.query.query_nonce),
                credential_digest: &digest,
                valid_until: UnixMillis::new(root.deadline.get().min(request.query.admit_until.0)),
            },
            now,
        )
        .map_err(|_| invalid())
    }
}

pub(super) fn authenticated_model_receipt_v2<'a>(
    state: &AccountProvisionApprovalState,
    s: &'a TestSessionAuthorityV2,
    query: &SignedNativeModelReservationQueryV2,
    proof: &HttpPossessionReceipt,
    now: UnixMillis,
) -> Result<&'a SessionCredentialV1> {
    super::v2_credential_http::native_registration(s)?;
    query
        .verify_input_binding(&s.input, now.get())
        .map_err(|_| invalid())?;
    let root = s
        .roots
        .get(&SessionPurposeV1::Execution)
        .ok_or_else(invalid)?;
    let credential = root.credential.as_ref().ok_or_else(invalid)?;
    let pair = &s.header()?.1.client_pairing;
    let target = CanonicalHttpTarget::new(
        &s.enrollment.registration.public_origin,
        HttpPopRoute::ScopedClient {
            provision_id: state.draft.entry.provision_id.clone(),
            custody_account_id: state.draft.entry.custody_account_id.clone(),
            route: Box::new(HttpPopRoute::ModelReservations(
                s.input.operation_id.clone(),
            )),
        },
    )
    .map_err(|_| invalid())?;
    if root.revoked
        || root.approved_input_digest != s.input_digest()?
        || now.get() >= root.deadline.get()
        || proof.profile() != HttpPopProfileV1::NativeModelCustodyV2
        || proof.binding_id() != s.input.operation_id
        || proof.request_id() != query.query.request_id
        || proof.nonce() != Some(query.query.query_nonce.as_str())
        || proof.key_id() != pair.client_key_id
        || proof.public_key_hex() != pair.client_public_key
        || proof.target_uri() != target.uri()
        || proof.method() != target.method()
        || super::hex_bytes(&credential.digest)? != *proof.token_digest()
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
