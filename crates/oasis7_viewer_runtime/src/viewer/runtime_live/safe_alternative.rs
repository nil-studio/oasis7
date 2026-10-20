use super::*;
use crate::world_service::{WorldServicePayloadV1, client::WorldServicePort};
use oasis7_client_api::{safe_alternative::SafeAlternativeCommandV1, world_service::*};

impl ViewerRuntimeLiveServer {
    pub(super) fn prepare_shared_safe_alternative(
        shared: &std::sync::Arc<std::sync::Mutex<Self>>,
        request: &ViewerRequest,
    ) -> Option<Result<serde_json::Value, String>> {
        let ViewerRequest::SafeAlternative { request, recover } = request else {
            return None;
        };
        let recover = *recover;
        let result = (|| -> Result<serde_json::Value, String> {
            let client = {
                let server = lock_shared_server(shared).map_err(|e| format!("{e:?}"))?;
                if server.authoritative_recovery_write_fence.is_some() {
                    return Err("authoritative recovery is read-only".into());
                }
                let signed: SignedReadRequest<SafeAlternativeCommandV1> =
                    serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
                signed.request.validate()?;
                oasis7_client_api::world_service::verify_read_request(
                    signed.request.signing_domain(),
                    &signed,
                )?;
                server.session_policy.validate_known_session_key(
                    &signed.request.owner_binding,
                    &signed.subject_public_key,
                )?;
                let client = server
                    .world_service_client()
                    .map_err(|e| format!("{e:?}"))?
                    .ok_or("safe alternatives require WorldService")?;
                client
            };
            let signed: SignedReadRequest<SafeAlternativeCommandV1> =
                serde_json::from_value(request.clone()).map_err(|e| e.to_string())?;
            let payload = WorldServicePayloadV1::SafeAlternative(signed);
            let correlation = crate::world_service::derive_correlation(
                client.config().expected_world.clone(),
                &payload,
            )?;
            if recover {
                let response = client
                    .lookup(
                        LookupIntentRequest {
                            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                            key: correlation.key,
                        },
                        payload,
                    )
                    .map_err(|e| e.to_string())?;
                serde_json::to_value(response).map_err(|e| e.to_string())
            } else {
                let response = client
                    .submit(SubmitIntentRequest {
                        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                        correlation,
                        deadline_unix_ms: None,
                        signed_payload: payload,
                    })
                    .map_err(|e| e.to_string())?;
                match response {
                    SubmitObservation::Response(response) => {
                        serde_json::to_value(response).map_err(|e| e.to_string())
                    }
                    SubmitObservation::OutcomeUnknown(_) => {
                        Ok(serde_json::json!({"outcome":{"status":"unknown"}}))
                    }
                }
            }
        })();
        Some(result)
    }
    pub(super) fn handle_safe_alternative(
        &mut self,
        request: serde_json::Value,
        _recover: bool,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let result = self.prepared_safe_alternative.take().unwrap_or_else(|| {
            Err("safe alternative requires prepared transport outside server lock".into())
        });
        send_response(
            writer,
            &ViewerResponse::SafeAlternativeResult {
                request,
                result: result.unwrap_or_else(
                    |reason| serde_json::json!({"error":reason,"observation":"unknown"}),
                ),
            },
        )
    }
}
