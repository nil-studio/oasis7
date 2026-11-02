//! Fixed typed paid reads; no admission issuer/Owner seed crosses the Native mutex.
use super::*;
use oasis7_client_api::world_service::resource_basis as resource;
use oasis7_custody_core::provisioning::{
    OwnerSetupResourceFinalHttpV2, RESOURCE_READ_PROOF_HEADER,
};
impl OwnedOwnerReadGateway {
    fn resource_post<Q: Serialize>(
        &self,
        path: &str,
        request: &Q,
        frame: &OwnerSetupResourceFinalHttpV2,
        substep: resource::ResourcePaidWorldSubstepV1,
    ) -> Result<resource::SignedResourcePaidWorldReplyV1, StoreError> {
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
        let result = resource::decode_paid_resource_world_reply(&bytes)
            .map_err(|_| StoreError::OutcomeUnknown)?;
        let expected = frame
            .paid_world_reply_binding(substep)
            .map_err(|_| StoreError::OutcomeUnknown)?;
        if path != substep.world_path() {
            return Err(StoreError::OutcomeUnknown);
        }
        resource::verify_paid_resource_world_reply(
            &result,
            &expected,
            &self.trust.service_public_key,
        )
        .map_err(|_| StoreError::OutcomeUnknown)?;
        Ok(result)
    }
    pub(in super::super::super) fn read_resource_owner_preflight(
        &self,
        frame: &OwnerSetupResourceFinalHttpV2,
    ) -> Result<
        (
            SignedServiceResponse<OwnerSetupObservationV1>,
            resource::SignedResourcePaidWorldReplyV1,
        ),
        StoreError,
    > {
        frame
            .validate_bindings()
            .map_err(|_| reject("invalid_request"))?;
        let q = &frame.request().access;
        let outer = self.resource_post(
            PARTICIPANT_OWNER_VIEW_PATH,
            q,
            frame,
            resource::ResourcePaidWorldSubstepV1::OwnerPreflight,
        )?;
        let resource::ResourcePaidWorldReplyV1::OwnerPreflight { observation, .. } = &outer.payload
        else {
            return Err(StoreError::OutcomeUnknown);
        };
        let response = observation.as_ref().clone();
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
        Ok((response, outer))
    }
    pub(in super::super::super) fn read_resource_final(
        &self,
        frame: &OwnerSetupResourceFinalHttpV2,
    ) -> Result<
        (
            resource::SignedResourceResponseV1<resource::OwnerSetupFinalResourceObservationV2>,
            resource::SignedResourcePaidWorldReplyV1,
        ),
        StoreError,
    > {
        let body = frame.world_body().map_err(|_| reject("invalid_request"))?;
        let outer = self.resource_post(
            resource::RESOURCE_GOAL_PATH_V2,
            &body,
            frame,
            resource::ResourcePaidWorldSubstepV1::ResourceFinal,
        )?;
        let resource::ResourcePaidWorldReplyV1::ResourceFinal { observation, .. } = &outer.payload
        else {
            return Err(StoreError::OutcomeUnknown);
        };
        let response = observation.as_ref().clone();
        resource::verify_resource_goal_response(
            body.resource(),
            &response,
            &self.trust.service_public_key,
        )
        .map_err(|_| StoreError::OutcomeUnknown)?;
        resource::encode_native_resource_envelope(response.clone())
            .map_err(|_| StoreError::OutcomeUnknown)?;
        Ok((response, outer))
    }
}
