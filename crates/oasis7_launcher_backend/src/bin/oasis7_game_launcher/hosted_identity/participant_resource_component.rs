//! Cargo-test component only. No production route or V2 payment acceptance.
use super::*;
use oasis7_client_api::world_service::resource_basis as wire;
use serde::de::DeserializeOwned;
impl ParticipantEntry {
    pub(in crate::hosted_identity) fn forward_resource_component<Q, T>(
        &self,
        path: &str,
        q: &Q,
        validate: impl FnOnce(&wire::SignedResourceResponseV1<T>) -> Result<(), String>,
    ) -> Result<wire::SignedResourceResponseV1<T>, StoreError>
    where
        Q: Serialize,
        T: DeserializeOwned + Serialize + Clone,
    {
        if ![
            wire::RESOURCE_GOAL_PATH_V2,
            wire::OWNER_RESOURCE_VIEW_PATH_V1,
        ]
        .contains(&path)
        {
            return Err(reject("invalid_request"));
        }
        let body = serde_json::to_vec(q).map_err(|_| reject("invalid_request"))?;
        if body.len() > wire::MAX_RESOURCE_ENVELOPE_BYTES {
            return Err(reject("invalid_request"));
        }
        let response = self
            .http
            .post(format!("{}{path}", self.trust.endpoint))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .map_err(|_| reject("participant_unavailable"))?;
        if response.status().as_u16() != 200 {
            return Err(reject("participant_unavailable"));
        }
        let mut body = Vec::new();
        response
            .take(wire::MAX_RESOURCE_ENVELOPE_BYTES as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| reject("participant_unavailable"))?;
        if body.len() > wire::MAX_RESOURCE_ENVELOPE_BYTES {
            return Err(reject("participant_unavailable"));
        }
        let response: wire::SignedResourceResponseV1<T> =
            parse_request(&body).map_err(|_| reject("participant_unavailable"))?;
        validate(&response).map_err(|_| reject("participant_unavailable"))?;
        // Exact bytes of the complete final envelope, not an allowance estimate.
        wire::encode_native_resource_envelope(response.clone())
            .map_err(|_| reject("participant_unavailable"))?;
        Ok(response)
    }
}
impl IdentityService {
    pub(in crate::hosted_identity) fn owner_resource_component(
        &mut self,
        sid: Option<&str>,
        q: &wire::ParticipantOwnerResourceViewRequestV1,
        now: u64,
    ) -> Result<Vec<u8>, StoreError> {
        wire::verify_owner_resource_request(q).map_err(|_| reject("invalid_request"))?;
        let (account, _) = self.session(sid, now)?;
        let entry = self
            .participant_entry
            .as_ref()
            .ok_or_else(|| reject("participant_unavailable"))?;
        self.check_participant_subject(&q.owner.request, &account.account_id, entry)?;
        if q.admission.subject_public_key != entry.issuer_public_key {
            return Err(reject("invalid_request"));
        }
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            &q.admission.request,
            &entry.issuer_public_key,
            &q.admission.signature_hex,
        )
        .map_err(|_| reject("invalid_request"))?;

        if oasis7_client_api::world_service::WorldIdentity::from(q.read.request.world.clone())
            != entry.trust.world
        {
            return Err(reject("invalid_request"));
        }
        self.store.transaction(|tx| {
            consume_quota(
                tx,
                "participant-request",
                &account.account_id,
                now,
                60_000,
                120,
            )
        })?;
        let response =
            entry.forward_resource_component(wire::OWNER_RESOURCE_VIEW_PATH_V1, q, |r| {
                wire::verify_owner_resource_response(q, r, &entry.trust.service_public_key)
            })?;
        self.session(sid, super::super::now_ms())?;
        wire::encode_native_resource_envelope(response)
            .map_err(|_| reject("participant_unavailable"))
    }
}

#[cfg(test)]
#[path = "resource_component_native_tests.rs"]
mod tests;
