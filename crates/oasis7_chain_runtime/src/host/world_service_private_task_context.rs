//! Native confidential task input; canonical prefix admission is separate.
use super::*;
use oasis7::world_service::private_goal_store::PrivateTaskContextError;
use oasis7::world_service::private_task_context::*;
use oasis7_agent_api::game::{GameTaskFenceV1, TaskContextIdentityV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReadConflictStage {
    InitialCaptureExpired,
    InitialCaptureExceedsAuthorityExpiry,
    InitialCaptureMissingPrefix,
    InitialCaptureInactive,
    InitialCaptureValidation,
    InitialStoredContextConflict,
    InitialRecordExpired,
    InitialCapabilityAuthorizationRootChanged,
    InitialAuthorityContextHashChanged,
    InitialCapabilitySnapshotHashChanged,
    DeliveryBindingChanged,
    DeliveryCaptureExpired,
    DeliveryCaptureExceedsAuthorityExpiry,
    DeliveryCaptureMissingPrefix,
    DeliveryCaptureInactive,
    DeliveryCaptureValidation,
    DeliveryRecordExpired,
    DeliveryCapabilityAuthorizationRootChanged,
    DeliveryAuthorityContextHashChanged,
    DeliveryCapabilitySnapshotHashChanged,
}

impl ReadConflictStage {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::InitialCaptureExpired => "initial_capture_expired",
            Self::InitialCaptureExceedsAuthorityExpiry => {
                "initial_capture_exceeds_authority_expiry"
            }
            Self::InitialCaptureMissingPrefix => "initial_capture_missing_prefix",
            Self::InitialCaptureInactive => "initial_capture_inactive",
            Self::InitialCaptureValidation => "initial_capture_validation_failed",
            Self::InitialStoredContextConflict => "initial_stored_context_conflict",
            Self::InitialRecordExpired => "initial_record_expired",
            Self::InitialCapabilityAuthorizationRootChanged => {
                "initial_capability_authorization_root_changed"
            }
            Self::InitialAuthorityContextHashChanged => "initial_authority_context_hash_changed",
            Self::InitialCapabilitySnapshotHashChanged => {
                "initial_capability_snapshot_hash_changed"
            }
            Self::DeliveryBindingChanged => "delivery_binding_changed",
            Self::DeliveryCaptureExpired => "delivery_capture_expired",
            Self::DeliveryCaptureExceedsAuthorityExpiry => {
                "delivery_capture_exceeds_authority_expiry"
            }
            Self::DeliveryCaptureMissingPrefix => "delivery_capture_missing_prefix",
            Self::DeliveryCaptureInactive => "delivery_capture_inactive",
            Self::DeliveryCaptureValidation => "delivery_capture_validation_failed",
            Self::DeliveryRecordExpired => "delivery_record_expired",
            Self::DeliveryCapabilityAuthorizationRootChanged => {
                "delivery_capability_authorization_root_changed"
            }
            Self::DeliveryAuthorityContextHashChanged => "delivery_authority_context_hash_changed",
            Self::DeliveryCapabilitySnapshotHashChanged => {
                "delivery_capability_snapshot_hash_changed"
            }
        }
    }
}

#[derive(Clone, Copy)]
enum RecordLiveConflict {
    Expired,
    CapabilityAuthorizationRootChanged,
    AuthorityContextHashChanged,
    CapabilitySnapshotHashChanged,
}

#[derive(Clone, Copy)]
enum ReadCheckPoint {
    Initial,
    Delivery,
}

/// A validated history view and the exact immutable pin it was built from.
/// Keeping these together prevents request handlers from accidentally using a
/// view validated against one World as authority for another pin.
struct ValidatedPinnedTask<'a> {
    pinned: &'a PinnedWorld,
    history: game_task::ValidatedRetentionHistory<'a>,
}

fn read_conflict(stage: ReadConflictStage) -> PrivateError {
    PrivateError::TaskContextReadConflict(stage)
}

fn record_live_conflict(
    point: Option<ReadCheckPoint>,
    conflict: RecordLiveConflict,
) -> PrivateError {
    let Some(point) = point else {
        return PrivateError::Conflict;
    };
    let stage = match (point, conflict) {
        (ReadCheckPoint::Initial, RecordLiveConflict::Expired) => {
            ReadConflictStage::InitialRecordExpired
        }
        (ReadCheckPoint::Initial, RecordLiveConflict::CapabilityAuthorizationRootChanged) => {
            ReadConflictStage::InitialCapabilityAuthorizationRootChanged
        }
        (ReadCheckPoint::Initial, RecordLiveConflict::AuthorityContextHashChanged) => {
            ReadConflictStage::InitialAuthorityContextHashChanged
        }
        (ReadCheckPoint::Initial, RecordLiveConflict::CapabilitySnapshotHashChanged) => {
            ReadConflictStage::InitialCapabilitySnapshotHashChanged
        }
        (ReadCheckPoint::Delivery, RecordLiveConflict::Expired) => {
            ReadConflictStage::DeliveryRecordExpired
        }
        (ReadCheckPoint::Delivery, RecordLiveConflict::CapabilityAuthorizationRootChanged) => {
            ReadConflictStage::DeliveryCapabilityAuthorizationRootChanged
        }
        (ReadCheckPoint::Delivery, RecordLiveConflict::AuthorityContextHashChanged) => {
            ReadConflictStage::DeliveryAuthorityContextHashChanged
        }
        (ReadCheckPoint::Delivery, RecordLiveConflict::CapabilitySnapshotHashChanged) => {
            ReadConflictStage::DeliveryCapabilitySnapshotHashChanged
        }
    };
    read_conflict(stage)
}

fn capture_validation_conflict(
    world: &oasis7_runtime::World,
    context: &TaskContextIdentityV1,
    point: ReadCheckPoint,
    error: &str,
) -> PrivateError {
    let expired_context = error == "task context expired or exceeds canonical grant expiry"
        && world.state().time > context.valid_until_tick.0;
    let exceeds_authority_expiry =
        error == "task context expired or exceeds canonical grant expiry" && !expired_context;
    let missing_prefix = error == "task context has no canonical prefix";
    let inactive_context = error == "task context is not active";
    let stage = match point {
        ReadCheckPoint::Initial if expired_context => ReadConflictStage::InitialCaptureExpired,
        ReadCheckPoint::Initial if exceeds_authority_expiry => {
            ReadConflictStage::InitialCaptureExceedsAuthorityExpiry
        }
        ReadCheckPoint::Initial if missing_prefix => ReadConflictStage::InitialCaptureMissingPrefix,
        ReadCheckPoint::Initial if inactive_context => ReadConflictStage::InitialCaptureInactive,
        ReadCheckPoint::Initial => ReadConflictStage::InitialCaptureValidation,
        ReadCheckPoint::Delivery if expired_context => ReadConflictStage::DeliveryCaptureExpired,
        ReadCheckPoint::Delivery if exceeds_authority_expiry => {
            ReadConflictStage::DeliveryCaptureExceedsAuthorityExpiry
        }
        ReadCheckPoint::Delivery if missing_prefix => {
            ReadConflictStage::DeliveryCaptureMissingPrefix
        }
        ReadCheckPoint::Delivery if inactive_context => ReadConflictStage::DeliveryCaptureInactive,
        ReadCheckPoint::Delivery => ReadConflictStage::DeliveryCaptureValidation,
    };
    read_conflict(stage)
}

fn validate_pinned_task<'a>(
    service: &Service<'_>,
    pinned: &'a PinnedWorld,
    fence: &GameTaskFenceV1,
    subject: &str,
    executor: bool,
) -> Result<ValidatedPinnedTask<'a>, PrivateError> {
    if fence.world_id != service.identity.world_id
        || fence.genesis_digest != service.identity.genesis_digest
    {
        return Err(PrivateError::Forbidden);
    }
    let history = game_task::validate_retention_history(&pinned.world)
        .map_err(|_| PrivateError::Forbidden)?;
    if executor {
        history.validate_task_executor(fence, subject)
    } else {
        history.validate_task_reader(fence, subject)
    }
    .map_err(|_| PrivateError::Forbidden)?;
    Ok(ValidatedPinnedTask { pinned, history })
}
fn snapshot(
    service: &Service<'_>,
    task: &ValidatedPinnedTask<'_>,
    fence: &GameTaskFenceV1,
    authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
    store: &PrivateGoalStore,
) -> Result<GoalExecutionSnapshotV1, PrivateError> {
    let pinned = task.pinned;
    let control = task
        .history
        .current_control(&fence.agent_id)?
        .ok_or(PrivateError::Forbidden)?;
    let publication = task
        .history
        .active_publish(&fence.agent_id)?
        .ok_or(PrivateError::Unavailable)?;
    let WorldServicePayloadV1::GameGoalPublish(public) = &publication.request.signed_payload else {
        return Err(PrivateError::Recovery);
    };
    let historical = service
        .pin_at_height(publication.committed_height)
        .map_err(|_| PrivateError::Recovery)?;
    let key = correlation::key_digest(&publication.request.correlation.key)?;
    if historical
        .world
        .capability_revocation_state()
        .world_service_results
        .get(&key)
        != Some(&serde_json::to_value(&publication).map_err(|_| PrivateError::Recovery)?)
    {
        return Err(PrivateError::Recovery);
    }
    let active_goal = store
        .get_prepared(&public.request)
        .map_err(|_| PrivateError::Recovery)?;
    let provider_authority = if authority_mode.is_none() {
        Some(
            pinned
                .world
                .canonical_agent_service_context(&fence.agent_id)?,
        )
    } else {
        None
    };
    let native_authority = if authority_mode.is_some() {
        Some(pinned.world.canonical_native_task_authority(fence)?)
    } else {
        None
    };
    let actor = pinned
        .world
        .state()
        .agents
        .get(&fence.agent_id)
        .ok_or(PrivateError::Forbidden)?;
    let runtime_binding = pinned
        .world
        .current_cognition_runtime_binding()
        .map_err(|_| PrivateError::Unavailable)?;
    let output = GoalExecutionSnapshotV1 {
        schema_revision: oasis7_agent_api::game::TaskContextAuthorityModeV1::schema_revision(
            authority_mode,
        ),
        control,
        read_commit: pinned.commit.clone(),
        activation_commit: historical.commit,
        active_goal,
        actor_observation_schema: "world-agent-state/v1".into(),
        actor_observation: serde_json::to_value(actor).map_err(|_| PrivateError::Unavailable)?,
        provider_authority,
        native_authority,
        runtime_binding,
        captured_base_binding: pinned
            .world
            .current_cognition_base_binding()
            .map_err(|_| PrivateError::Unavailable)?,
    };
    output.validate().map_err(|_| PrivateError::Recovery)?;
    Ok(output)
}
fn task_error(error: PrivateTaskContextError) -> PrivateError {
    match error {
        PrivateTaskContextError::Invalid => PrivateError::Invalid,
        PrivateTaskContextError::Conflict => PrivateError::Conflict,
        PrivateTaskContextError::Unavailable => PrivateError::Recovery,
    }
}
fn check_record_live(
    pinned: &PinnedWorld,
    record: &PreparedTaskContextV1,
    point: Option<ReadCheckPoint>,
) -> Result<(), PrivateError> {
    if pinned.world.state().time > record.valid_until_tick.0 {
        return Err(record_live_conflict(point, RecordLiveConflict::Expired));
    }
    let provider = if record.identity.authority_mode.is_none() {
        Some(
            pinned
                .world
                .canonical_agent_service_context(&record.identity.fence.agent_id)?,
        )
    } else {
        None
    };
    let native = if record.identity.authority_mode.is_some() {
        Some(
            pinned
                .world
                .canonical_native_task_authority(&record.identity.fence)?,
        )
    } else {
        None
    };
    let current = oasis7_client_api::world_service::task_context::TaskAuthorityRef::select(
        provider.as_ref(),
        native.as_ref(),
    )?;
    let old = record.snapshot.authority()?;
    if current.mode() != old.mode() {
        return Err(PrivateError::Conflict);
    }
    if current.hashes().0 != old.hashes().0 {
        return Err(record_live_conflict(
            point,
            RecordLiveConflict::CapabilityAuthorizationRootChanged,
        ));
    }
    if current.hashes().2 != old.hashes().2 {
        return Err(record_live_conflict(
            point,
            RecordLiveConflict::AuthorityContextHashChanged,
        ));
    }
    if current.hashes().1 != old.hashes().1 {
        return Err(record_live_conflict(
            point,
            RecordLiveConflict::CapabilitySnapshotHashChanged,
        ));
    }
    if let (Some(current), Some(old)) = (&native, &record.snapshot.native_authority)
        && current.native_authorization_root != old.native_authorization_root
    {
        return Err(record_live_conflict(
            point,
            RecordLiveConflict::CapabilityAuthorizationRootChanged,
        ));
    }
    Ok(())
}
fn capture_result(
    history: &game_task::ValidatedRetentionHistory<'_>,
    context: &TaskContextIdentityV1,
    subject: &str,
) -> Result<CanonicalIntentResultV1, PrivateError> {
    history
        .matching_task_prefix(context, subject)
        .map_err(|_| PrivateError::Recovery)?
        .cloned()
        .ok_or_else(|| read_conflict(ReadConflictStage::InitialCaptureMissingPrefix))
}
fn load_captured(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    proof: &SignedReadRequest<TaskContextGetV1>,
    task: &ValidatedPinnedTask<'_>,
) -> Result<CapturedTaskContextV1, PrivateError> {
    let pinned = task.pinned;
    verify_get(proof).map_err(|_| PrivateError::Invalid)?;
    task.history
        .validate_prefix_capture(&proof.request.context, &proof.subject_public_key)
        .map_err(|error| {
            capture_validation_conflict(
                &pinned.world,
                &proof.request.context,
                ReadCheckPoint::Initial,
                &error,
            )
        })?;
    let result = capture_result(
        &task.history,
        &proof.request.context,
        &proof.subject_public_key,
    )?;
    let historical = service
        .pin_at_height(result.committed_height)
        .map_err(|_| PrivateError::Recovery)?;
    let key = correlation::key_digest(&result.request.correlation.key)?;
    if historical
        .world
        .capability_revocation_state()
        .world_service_results
        .get(&key)
        != Some(&serde_json::to_value(&result).map_err(|_| PrivateError::Recovery)?)
    {
        return Err(PrivateError::Recovery);
    }
    let record = store.get_task_context(proof).map_err(|error| match error {
        PrivateTaskContextError::Conflict => {
            read_conflict(ReadConflictStage::InitialStoredContextConflict)
        }
        other => task_error(other),
    })?;
    check_record_live(pinned, &record, Some(ReadCheckPoint::Initial))?;
    let provider_authority = if proof.request.context.authority_mode.is_none() {
        Some(
            pinned
                .world
                .canonical_agent_service_context(&proof.request.context.fence.agent_id)?,
        )
    } else {
        None
    };
    let native_authority = if proof.request.context.authority_mode.is_some() {
        Some(
            pinned
                .world
                .canonical_native_task_authority(&proof.request.context.fence)?,
        )
    } else {
        None
    };
    let output = CapturedTaskContextV1 {
        schema_revision: 1,
        prepared: record,
        capture_commit: historical.commit,
        capture_proof: result,
        current_public_context: TaskPublicContextV1 {
            read_commit: pinned.commit.clone(),
            captured_base_binding: pinned
                .world
                .current_cognition_base_binding()
                .map_err(|_| PrivateError::Unavailable)?,
            provider_authority,
            native_authority,
            actor_observation: serde_json::to_value(
                pinned
                    .world
                    .state()
                    .agents
                    .get(&proof.request.context.fence.agent_id)
                    .ok_or(PrivateError::Forbidden)?,
            )
            .map_err(|_| PrivateError::Unavailable)?,
        },
    };
    output
        .validate_against(proof)
        .map_err(|_| PrivateError::Recovery)?;
    Ok(output)
}

#[path = "world_service_private_factory_quote.rs"]
mod factory_quote;
#[path = "world_service_private_recipe_quote.rs"]
mod recipe_quote;

pub(super) fn process(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    path: &str,
    body: &[u8],
    before_delivery: impl FnOnce(),
) -> Result<Value, PrivateError> {
    if path == oasis7::world_service::task_recipe_quote::TASK_RECIPE_QUOTE_PATH {
        return recipe_quote::process(service, store, body, before_delivery);
    }
    if path == oasis7::world_service::task_factory_quote::TASK_FACTORY_QUOTE_PATH {
        return factory_quote::process(service, store, body, before_delivery);
    }
    match path {
        TASK_SNAPSHOT_PATH => {
            let proof: SignedReadRequest<TaskGoalSnapshotRequestV1> =
                parse(body).map_err(|_| PrivateError::Invalid)?;
            verify_snapshot_request(&proof).map_err(|_| PrivateError::Invalid)?;
            let pinned = service.pin(None)?;
            let task = validate_pinned_task(
                service,
                &pinned,
                &proof.request.fence,
                &proof.subject_public_key,
                false,
            )?;
            let output = snapshot(
                service,
                &task,
                &proof.request.fence,
                proof.request.authority_mode,
                store,
            )?;
            before_delivery();
            let fresh = service.pin(None)?;
            let fresh_task = validate_pinned_task(
                service,
                &fresh,
                &proof.request.fence,
                &proof.subject_public_key,
                false,
            )?;
            if fresh_task.pinned.commit.binding != task.pinned.commit.binding {
                return Err(PrivateError::Conflict);
            }
            service
                .signed(path, &proof, output)
                .map_err(PrivateError::from)
        }
        TASK_CONTEXT_PREPARE_PATH => {
            let proof: SignedReadRequest<TaskContextPrepareV1> =
                parse(body).map_err(|_| PrivateError::Invalid)?;
            verify_prepare(&proof).map_err(|_| PrivateError::Invalid)?;
            let pinned = service.pin(None)?;
            let task = validate_pinned_task(
                service,
                &pinned,
                &proof.request.fence,
                &proof.subject_public_key,
                true,
            )?;
            let record = match store.lookup_task_context(&proof).map_err(task_error)? {
                Some(record) => record,
                None => store
                    .prepare_task_context(
                        &proof,
                        snapshot(
                            service,
                            &task,
                            &proof.request.fence,
                            proof.request.authority_mode,
                            store,
                        )?,
                    )
                    .map_err(task_error)?,
            };
            check_record_live(task.pinned, &record, None)?;
            before_delivery();
            let fresh = service.pin(None)?;
            let fresh_task = validate_pinned_task(
                service,
                &fresh,
                &proof.request.fence,
                &proof.subject_public_key,
                true,
            )?;
            check_record_live(fresh_task.pinned, &record, None)?;
            service
                .signed(path, &proof, record)
                .map_err(PrivateError::from)
        }
        TASK_CONTEXT_READ_PATH => {
            let proof: SignedReadRequest<TaskContextGetV1> =
                parse(body).map_err(|_| PrivateError::Invalid)?;
            verify_get(&proof).map_err(|_| PrivateError::Invalid)?;
            let pinned = service.pin(None)?;
            let task = validate_pinned_task(
                service,
                &pinned,
                &proof.request.context.fence,
                &proof.subject_public_key,
                true,
            )?;
            let output = load_captured(service, store, &proof, &task)?;
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
            service
                .signed(path, &proof, output)
                .map_err(PrivateError::from)
        }
        _ => Err(PrivateError::Invalid),
    }
}

#[cfg(all(test, unix))]
#[path = "world_service_private_task_context_diagnostic_tests.rs"]
mod diagnostic_tests;
