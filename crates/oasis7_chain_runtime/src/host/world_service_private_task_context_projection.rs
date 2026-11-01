//! Actual World plaintext producer. Only the new public projection may leave
//! Identity for Account; the existing private wire remains in the new delivery.
use super::*;
use oasis7_client_api::world_service::task_context::{
    CapturedTaskContextV1 as ApiCaptured, PreparedTaskContextV1 as ApiPrepared,
};
use oasis7_client_api::world_service::task_context_projection::*;

fn invalid(_: String) -> PrivateError {
    PrivateError::Invalid
}
fn derivation_error(error: String) -> PrivateError {
    if error == "context_too_large" {
        PrivateError::ContextTooLarge
    } else {
        PrivateError::Recovery
    }
}
fn original_bytes<T: Serialize>(value: &T) -> Result<Zeroizing<Vec<u8>>, PrivateError> {
    serde_json::to_vec(value)
        .map(Zeroizing::new)
        .map_err(|_| PrivateError::Invalid)
}
fn typed<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, PrivateError> {
    serde_json::from_value(value).map_err(|_| PrivateError::Recovery)
}
fn finish(
    service: &Service<'_>,
    original: &SignedReadRequest<TaskContextPrepareV1>,
    prepared: &ApiPrepared,
    exact_read_commit: Option<&CommitRef>,
) -> Result<(), PrivateError> {
    let pinned = service.pin(None)?;
    let task = validate_pinned_task(
        service,
        &pinned,
        &original.request.fence,
        &original.subject_public_key,
        true,
    )?;
    check_record_live(task.pinned, prepared, Some(ReadCheckPoint::Delivery))?;
    if exact_read_commit.is_some_and(|commit| commit != &pinned.commit) {
        return Err(PrivateError::ContextReadDeliveryDrift(None));
    }
    Ok(())
}
fn bounded(value: Value) -> Result<Value, PrivateError> {
    let bytes = Zeroizing::new(serde_json::to_vec(&value).map_err(|_| PrivateError::Recovery)?);
    if bytes.len() > MAX_ATTESTED_CONTEXT_WIRE_BYTES_V2 {
        return Err(PrivateError::ContextTooLarge);
    }
    Ok(value)
}
pub(super) fn handles(path: &str) -> bool {
    matches!(
        path,
        CONTEXT_PREPARE_ATTESTED_PATH_V2
            | CONTEXT_READ_ATTESTED_PATH_V2
            | CONTEXT_MOVE_PROJECTION_PATH_V2
    )
}
pub(super) fn process(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    path: &str,
    body: &[u8],
    before_delivery: impl FnOnce(),
) -> Result<Value, PrivateError> {
    if body.len() > MAX_ATTESTED_CONTEXT_WIRE_BYTES_V2 {
        return Err(PrivateError::Invalid);
    }
    let key = &service.signer.public_key_hex;
    match path {
        CONTEXT_PREPARE_ATTESTED_PATH_V2 => {
            let request: TaskContextPrepareAttestationRequestV2 =
                parse(body).map_err(|_| PrivateError::Invalid)?;
            validate_prepare_attestation_request_v2(&request).map_err(invalid)?;
            // A real local call into the unchanged producer, never self-HTTP,
            // cached response re-signing, or an Identity-supplied body/hash.
            let private_body: SignedServiceResponse<ApiPrepared> = typed(super::process(
                service,
                store,
                TASK_CONTEXT_PREPARE_PATH,
                &original_bytes(&request.original)?,
                before_delivery,
            )?)?;
            let projection = derive_prepared_projection_v2(&request, &private_body, key)
                .map_err(derivation_error)?;
            finish(service, &request.original, &private_body.payload, None)?;
            let projection =
                typed(service.signed(PREPARED_PROJECTION_DOMAIN_V2, &request, projection)?)?;
            bounded(service.signed(
                path,
                &request,
                TaskContextPreparedDeliveryV2 {
                    private_body,
                    projection,
                },
            )?)
        }
        CONTEXT_READ_ATTESTED_PATH_V2 => {
            let request: TaskContextGetAttestationRequestV2 =
                parse(body).map_err(|_| PrivateError::Invalid)?;
            validate_get_attestation_request_v2(&request, key).map_err(invalid)?;
            let private_body: SignedServiceResponse<ApiCaptured> = typed(super::process(
                service,
                store,
                TASK_CONTEXT_READ_PATH,
                &original_bytes(&request.original)?,
                before_delivery,
            )?)?;
            let projection = derive_captured_projection_v2(&request, &private_body, key)
                .map_err(derivation_error)?;
            finish(
                service,
                &request.prepared_projection.payload.original,
                &private_body.payload.prepared,
                Some(&private_body.payload.current_public_context.read_commit),
            )?;
            let projection =
                typed(service.signed(CAPTURED_PROJECTION_DOMAIN_V2, &request, projection)?)?;
            bounded(service.signed(
                path,
                &request,
                TaskContextCapturedDeliveryV2 {
                    private_body,
                    projection,
                },
            )?)
        }
        CONTEXT_MOVE_PROJECTION_PATH_V2 => {
            let request: TaskContextMoveProjectionRequestV2 =
                parse(body).map_err(|_| PrivateError::Invalid)?;
            validate_move_projection_request_v2(&request, key).map_err(invalid)?;
            // This is a new real read from actual pinned World and private
            // store. No historical Captured body is accepted from the caller.
            let output =
                context_read::fresh_output(service, store, &request.original, before_delivery)?;
            let old_live_request = TaskContextLiveReadV1 {
                original: request.original.clone(),
                observation_nonce: request.observation_nonce.clone(),
            };
            let private_body: SignedServiceResponse<ApiCaptured> =
                typed(service.signed(TASK_CONTEXT_LIVE_READ_PATH, &old_live_request, output)?)?;
            let projection = derive_move_projection_v2(&request, &private_body, key)
                .map_err(derivation_error)?;
            finish(
                service,
                &request.prepared_projection.payload.original,
                &private_body.payload.prepared,
                Some(&private_body.payload.current_public_context.read_commit),
            )?;
            // Only this public attestation is delivered. The temporary complete
            // plaintext is not put into the projection or any Account RPC.
            bounded(service.signed(path, &request, projection)?)
        }
        _ => Err(PrivateError::Invalid),
    }
}
