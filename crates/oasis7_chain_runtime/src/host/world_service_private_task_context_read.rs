//! Shared fresh load and delivery checks for both context read protocols.
use super::*;

fn fresh_output(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    proof: &SignedReadRequest<TaskContextGetV1>,
    before_delivery: impl FnOnce(),
) -> Result<CapturedTaskContextV1, PrivateError> {
    let pinned = service.pin(None)?;
    let task = validate_pinned_task(
        service,
        &pinned,
        &proof.request.context.fence,
        &proof.subject_public_key,
        true,
    )?;
    let output = load_captured(service, store, proof, &task)?;
    before_delivery();
    let fresh = service.pin(None)?;
    let fresh_task = validate_pinned_task(
        service,
        &fresh,
        &proof.request.context.fence,
        &proof.subject_public_key,
        true,
    )?;
    fresh_task
        .history
        .validate_prefix_capture(&proof.request.context, &proof.subject_public_key)
        .map_err(|error| {
            capture_validation_conflict(
                &fresh_task.pinned.world,
                &proof.request.context,
                ReadCheckPoint::Delivery,
                &error,
            )
        })?;
    check_record_live(
        fresh_task.pinned,
        &output.prepared,
        Some(ReadCheckPoint::Delivery),
    )?;
    if fresh_task.pinned.commit.binding != task.pinned.commit.binding {
        return Err(PrivateError::ContextReadDeliveryDrift(Some(
            ReadConflictStage::DeliveryBindingChanged,
        )));
    }
    // A dedicated retry category describes only a still-authorized,
    // unexpired context whose read snapshot changed during delivery.
    if fresh_task.pinned.commit != task.pinned.commit {
        return Err(PrivateError::ContextReadDeliveryDrift(None));
    }
    Ok(output)
}

pub(super) fn process(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    path: &str,
    body: &[u8],
    before_delivery: impl FnOnce(),
) -> Result<Value, PrivateError> {
    if path == TASK_CONTEXT_LIVE_READ_PATH {
        let request: TaskContextLiveReadV1 = parse(body).map_err(|_| PrivateError::Invalid)?;
        verify_live_context_read(&request).map_err(|_| PrivateError::Invalid)?;
        let output = fresh_output(service, store, &request.original, before_delivery)?;
        service
            .signed(path, &request, output)
            .map_err(PrivateError::from)
    } else {
        let proof: SignedReadRequest<TaskContextGetV1> =
            parse(body).map_err(|_| PrivateError::Invalid)?;
        verify_get(&proof).map_err(|_| PrivateError::Invalid)?;
        let output = fresh_output(service, store, &proof, before_delivery)?;
        service
            .signed(path, &proof, output)
            .map_err(PrivateError::from)
    }
}
