//! Shared exact bounded transport plus an owned read-only gateway. The owned
//! gateway contains no Native admission issuer or Cloud Owner secret.
use super::*;
pub(super) fn forward_bounded<Q: Serialize, R: Serialize>(
    trust: &EntryTrust,
    http: &reqwest::blocking::Client,
    path: &str,
    response_path: &str,
    request: &Q,
    response_request: &R,
    maximum: usize,
) -> Result<Value, StoreError> {
    // Only this read-only path may expose a fixed failure stage. Mutation
    // and original-lookup transport failures keep their unknown outcome.
    let failure = |code| {
        if path == PARTICIPANT_OWNER_VIEW_PATH {
            reject(code)
        } else {
            StoreError::OutcomeUnknown
        }
    };
    let bytes = serde_json::to_vec(request).map_err(|_| reject("invalid_request"))?;
    if bytes.len() > 65536 {
        return Err(reject("invalid_request"));
    }
    let response = http
        .post(format!("{}{path}", trust.endpoint))
        .header("Content-Type", "application/json")
        .body(bytes)
        .send()
        .map_err(|error| {
            failure(if error.is_timeout() {
                "participant_read_timeout"
            } else {
                "participant_read_transport"
            })
        })?;
    if response.status().as_u16() != 200 {
        let status = response.status().as_u16();
        if path == PARTICIPANT_OWNER_VIEW_PATH {
            return Err(failure("participant_read_http_status"));
        }
        let mut error = Vec::new();
        response
            .take(4097)
            .read_to_end(&mut error)
            .map_err(|_| failure("participant_read_response_read"))?;
        if error.len() <= 4096
            && status == 409
            && let Ok(value) = parse_request::<Value>(&error)
        {
            let code = value["error"].as_str().unwrap_or("");
            let quote=path==oasis7_client_api::world_service::task_recipe_quote::TASK_RECIPE_QUOTE_PATH || path==oasis7_client_api::world_service::task_factory_quote::TASK_FACTORY_QUOTE_PATH;
            let allowed = (quote
                    && ["task_quote_base_drift", "task_quote_delivery_drift"].contains(&code))
                    || (matches!(path,
                        oasis7_client_api::world_service::task_context::TASK_CONTEXT_READ_PATH
                        | oasis7_client_api::world_service::task_context::TASK_CONTEXT_LIVE_READ_PATH)
                        && code == "task_context_read_delivery_drift");
            let projection = matches!(
                path,
                CONTEXT_PREPARE_ATTESTED_PATH_V2
                    | CONTEXT_READ_ATTESTED_PATH_V2
                    | CONTEXT_MOVE_PROJECTION_PATH_V2
            );
            let allowed = allowed
                || (projection
                    && matches!(
                        code,
                        "context_too_large" | "task_context_read_delivery_drift"
                    ));
            if allowed {
                return Ok(
                    json!({"api_version":2,"ok":true,"upstream_read_error":{"status":409,"code":code}}),
                );
            }
        }
        return Err(failure("participant_read_http_status"));
    }
    let mut bytes = Vec::new();
    response
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure("participant_read_response_read"))?;
    if bytes.len() > maximum {
        return Err(failure("participant_read_response_limit"));
    }
    let digest =
        request_digest(response_path, response_request).map_err(|_| reject("invalid_request"))?;
    macro_rules! verified {
        ($kind:ty) => {{
            let original: Value = parse_bounded_json(&bytes, maximum)
                .map_err(|_| failure("participant_read_response_decode"))?;
            let response: SignedServiceResponse<$kind> =
                serde_json::from_value(original.clone())
                    .map_err(|_| failure("participant_read_response_schema"))?;
            let wire = serde_json::to_value(&response)
                .map_err(|_| failure("participant_read_response_shape"))?;
            if wire != original {
                return Err(failure("participant_read_response_shape"));
            }
            if response.request_digest != digest {
                return Err(failure("participant_read_response_context"));
            }
            verify_service_response(response_path, &digest, &response, &trust.service_public_key)
                .map_err(|_| failure("participant_read_response_signature"))?;
            wire
        }};
    }
    use oasis7_client_api::world_service::task_context_projection::*;
    use oasis7_client_api::world_service::{
        private_goal::*, private_goal_read::*, task_context::*, task_factory_quote::*,
        task_recipe_quote::*,
    };
    // Private signatures cover typed CBOR, including typed map keys.
    // Never reinterpret that proof as the normalized JSON view profile.
    let response = match response_path {
        PARTICIPANT_OWNER_VIEW_PATH => {
            // Both profiles use this path, but signatures cover their
            // distinct typed CBOR. Select only from the exact request whose
            // digest is being verified, never from the response body.
            let owner_request: ParticipantOwnerViewRequestV1 = serde_json::from_value(
                serde_json::to_value(response_request).map_err(|_| reject("invalid_request"))?,
            )
            .map_err(|_| reject("invalid_request"))?;
            match owner_request.response_profile {
                Some(ParticipantOwnerViewProfileV1::OwnerSetupV1) => {
                    verified!(OwnerSetupObservationV1)
                }
                Some(ParticipantOwnerViewProfileV1::OwnerControlV1) => {
                    verified!(ParticipantOwnerControlResponseV1)
                }
                None => return Err(reject("invalid_request")),
            }
        }
        GOAL_PREPARE_PATH => verified!(PreparedGoalReceiptV1),
        GOAL_CURRENT_PATH => verified!(CurrentGoalResponse),
        TASK_CONTEXT_PREPARE_PATH => verified!(PreparedTaskContextV1),
        TASK_CONTEXT_READ_PATH | TASK_CONTEXT_LIVE_READ_PATH => {
            verified!(CapturedTaskContextV1)
        }
        CONTEXT_PREPARE_ATTESTED_PATH_V2 => verified!(TaskContextPreparedDeliveryV2),
        CONTEXT_READ_ATTESTED_PATH_V2 => verified!(TaskContextCapturedDeliveryV2),
        CONTEXT_MOVE_PROJECTION_PATH_V2 => verified!(TaskContextMoveProjectionV2),
        TASK_SNAPSHOT_PATH => verified!(GoalExecutionSnapshotV1),
        TASK_RECIPE_QUOTE_PATH => verified!(TaskRecipeQuoteV1),
        TASK_FACTORY_QUOTE_PATH => verified!(TaskFactoryQuoteV1),
        _ => verified!(Value),
    };
    Ok(json!({"api_version":2,"ok":true,"world_response":response}))
}
pub(in super::super) struct OwnedOwnerReadGateway {
    trust: EntryTrust,
    http: reqwest::blocking::Client,
}
impl OwnedOwnerReadGateway {
    pub(in super::super) fn fingerprint(&self) -> Result<String, StoreError> {
        let bytes =
            serde_json::to_vec(&self.trust).map_err(|_| reject("participant_unavailable"))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
    pub(in super::super) fn read_owner_setup(
        &self,
        request: &ParticipantOwnerViewRequestV1,
    ) -> Result<SignedServiceResponse<OwnerSetupObservationV1>, StoreError> {
        if request.response_profile != Some(ParticipantOwnerViewProfileV1::OwnerSetupV1) {
            return Err(reject("invalid_request"));
        }
        let value = forward_bounded(
            &self.trust,
            &self.http,
            PARTICIPANT_OWNER_VIEW_PATH,
            PARTICIPANT_OWNER_VIEW_PATH,
            request,
            request,
            262144,
        )?;
        serde_json::from_value(value["world_response"].clone())
            .map_err(|_| reject("participant_read_response_schema"))
    }
}
impl ParticipantEntry {
    pub(in super::super) fn owned_owner_read_gateway(&self) -> OwnedOwnerReadGateway {
        OwnedOwnerReadGateway {
            trust: self.trust.clone(),
            http: self.http.clone(),
        }
    }
}
