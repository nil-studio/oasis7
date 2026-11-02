//! Fixed typed paid reads; no admission issuer/Owner seed crosses the Native mutex.
use super::*;
use oasis7_client_api::world_service::resource_basis as resource;
use oasis7_custody_core::provisioning::{
    OwnerSetupResourceFinalHttpV2, RESOURCE_READ_PROOF_HEADER,
};
use serde::de::DeserializeOwned;
impl OwnedOwnerReadGateway {
    fn resource_post<Q: Serialize, T: DeserializeOwned + Serialize + Clone>(
        &self,
        path: &str,
        request: &Q,
        frame: &OwnerSetupResourceFinalHttpV2,
    ) -> Result<resource::SignedResourceResponseV1<T>, StoreError> {
        let now = crate::hosted_identity::now_ms();
        if now < frame.paid().payload.issued_at.0 || now >= frame.paid().payload.expires_at.0 {
            return Err(StoreError::OutcomeUnknown);
        }
        let body = serde_json::to_vec(request).map_err(|_| reject("invalid_request"))?;
        if body.len() > resource::MAX_RESOURCE_ENVELOPE_BYTES {
            return Err(reject("invalid_request"));
        }
        // A physical prepayment authorizes exactly one HTTP dispatch. This new
        // profile must not inherit the legacy client's protocol retry policy.
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(3))
            .connect_timeout(Duration::from_secs(1))
            .build()
            .map_err(|_| StoreError::OutcomeUnknown)?;
        let response = http
            .post(format!("{}{path}", self.trust.endpoint))
            .header("Content-Type", "application/json")
            .header(
                RESOURCE_READ_PROOF_HEADER,
                frame
                    .proof_header()
                    .map_err(|_| reject("invalid_request"))?,
            )
            .body(body)
            .send()
            .map_err(|_| StoreError::OutcomeUnknown)?;
        if response.status().as_u16() != 200 {
            return Err(StoreError::OutcomeUnknown);
        }
        let mut bytes = Vec::new();
        response
            .take(resource::MAX_RESOURCE_ENVELOPE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| StoreError::OutcomeUnknown)?;
        if bytes.len() > resource::MAX_RESOURCE_ENVELOPE_BYTES {
            return Err(StoreError::OutcomeUnknown);
        }
        let result: resource::SignedResourceResponseV1<T> =
            parse_request(&bytes).map_err(|_| StoreError::OutcomeUnknown)?;
        let key: [u8; 32] = hex::decode(&self.trust.service_public_key)
            .map_err(|_| StoreError::OutcomeUnknown)?
            .try_into()
            .map_err(|_| StoreError::OutcomeUnknown)?;
        let signature = ed25519_dalek::Signature::from_slice(
            &hex::decode(&result.signature_hex).map_err(|_| StoreError::OutcomeUnknown)?,
        )
        .map_err(|_| StoreError::OutcomeUnknown)?;
        let bytes = oasis7_client_api::world_service::signing_bytes(
            path,
            &(&result.request_digest, &result.payload),
        )
        .map_err(|_| StoreError::OutcomeUnknown)?;
        ed25519_dalek::VerifyingKey::from_bytes(&key)
            .map_err(|_| StoreError::OutcomeUnknown)?
            .verify_strict(&bytes, &signature)
            .map_err(|_| StoreError::OutcomeUnknown)?;
        Ok(result)
    }
    pub(in super::super::super) fn read_resource_owner_preflight(
        &self,
        frame: &OwnerSetupResourceFinalHttpV2,
    ) -> Result<SignedServiceResponse<OwnerSetupObservationV1>, StoreError> {
        frame
            .validate_bindings()
            .map_err(|_| reject("invalid_request"))?;
        let q = &frame.request().access;
        let response = self.resource_post::<_, OwnerSetupObservationV1>(
            PARTICIPANT_OWNER_VIEW_PATH,
            q,
            frame,
        )?;
        let response = SignedServiceResponse {
            request_digest: response.request_digest,
            payload: response.payload,
            signature_hex: response.signature_hex,
        };
        let expected = request_digest(PARTICIPANT_OWNER_VIEW_PATH, q)
            .map_err(|_| reject("invalid_request"))?;
        verify_service_response(
            PARTICIPANT_OWNER_VIEW_PATH,
            &expected,
            &response,
            &self.trust.service_public_key,
        )
        .map_err(|_| StoreError::OutcomeUnknown)?;
        Ok(response)
    }
    pub(in super::super::super) fn read_resource_final(
        &self,
        frame: &OwnerSetupResourceFinalHttpV2,
    ) -> Result<
        resource::SignedResourceResponseV1<resource::OwnerSetupFinalResourceObservationV2>,
        StoreError,
    > {
        let body = frame.world_body().map_err(|_| reject("invalid_request"))?;
        let response = self.resource_post(resource::RESOURCE_GOAL_PATH_V2, &body, frame)?;
        resource::verify_resource_goal_response(
            body.resource(),
            &response,
            &self.trust.service_public_key,
        )
        .map_err(|_| StoreError::OutcomeUnknown)?;
        resource::encode_native_resource_envelope(response.clone())
            .map_err(|_| StoreError::OutcomeUnknown)?;
        Ok(response)
    }
}
