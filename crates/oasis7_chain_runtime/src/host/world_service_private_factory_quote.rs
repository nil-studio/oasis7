//! Pure quote over the same authenticated canonical pin as the captured task.
use super::*;
use oasis7::world_service::task_factory_quote::*;

pub(super) fn process(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    body: &[u8],
    before_delivery: impl FnOnce(),
) -> Result<Value, PrivateError> {
    let proof: SignedReadRequest<TaskFactoryQuoteRequestV1> =
        parse(body).map_err(|_| PrivateError::Invalid)?;
    verify_factory_quote_request(&proof).map_err(|_| PrivateError::Invalid)?;
    let context_proof = &proof.request.context_proof;
    let fence = &context_proof.request.context.fence;
    let pinned = service.pin(None)?;
    let task = validate_pinned_task(service, &pinned, fence, &proof.subject_public_key, true)?;
    let pinned = task.pinned;
    let captured = load_captured(service, store, context_proof, &task)?;
    if pinned
        .world
        .current_cognition_base_binding()
        .map_err(|_| PrivateError::Unavailable)?
        != proof.request.expected_base_binding
    {
        return Err(PrivateError::QuoteBaseDrift);
    }
    if oasis7_runtime::builtin_factory_spec(&proof.request.factory_kind, &proof.request.factory_id)
        .is_none()
    {
        return Err(PrivateError::Forbidden);
    }
    // Classify only a missing object grant after authenticating the full task.
    // Never expose another object's existence or the private grant set.
    let authorized = pinned
        .world
        .agent_delegation_authorizations(&fence.agent_id)
        .map_err(|_| PrivateError::Forbidden)?
        .iter()
        .any(|a| {
            a.grant.object_id == proof.request.factory_id
                && a.grant
                    .action_kinds
                    .iter()
                    .any(|kind| kind == "BuildFactory")
        });
    let output = if authorized {
        let quote = pinned
            .world
            .builtin_factory_quote(
                &fence.agent_id,
                &proof.request.site_id,
                &proof.request.factory_kind,
                &proof.request.factory_id,
            )
            .map_err(|_| PrivateError::Forbidden)?;
        TaskFactoryQuoteResponseV1::Quote(Box::new(TaskFactoryQuoteV1 {
            schema_revision: 1,
            context: context_proof.request.context.clone(),
            read_commit: pinned.commit.clone(),
            current_public_context: captured.current_public_context.clone(),
            quote,
        }))
    } else {
        TaskFactoryQuoteResponseV1::Denied(Box::new(TaskFactoryQuoteDeniedV1 {
            schema_revision: 1,
            context: context_proof.request.context.clone(),
            read_commit: pinned.commit.clone(),
            current_public_context: captured.current_public_context.clone(),
            reason: FactoryQuoteDenialV1::FactoryNotAuthorized,
            no_action_submitted: true,
        }))
    };
    output
        .validate_against(&proof)
        .map_err(|_| PrivateError::Recovery)?;
    before_delivery();
    let fresh = service.pin(None)?;
    let fresh_task = validate_pinned_task(service, &fresh, fence, &proof.subject_public_key, true)?;
    fresh_task
        .history
        .validate_prefix_capture(&context_proof.request.context, &proof.subject_public_key)
        .map_err(|error| {
            capture_validation_conflict(
                &fresh_task.pinned.world,
                &context_proof.request.context,
                ReadCheckPoint::Delivery,
                &error,
            )
        })?;
    check_record_live(
        fresh_task.pinned,
        &captured.prepared,
        Some(ReadCheckPoint::Delivery),
    )?;
    if fresh_task.pinned.commit != pinned.commit {
        return Err(PrivateError::QuoteDeliveryDrift);
    }
    service
        .signed(TASK_FACTORY_QUOTE_PATH, &proof, output)
        .map_err(PrivateError::from)
}
