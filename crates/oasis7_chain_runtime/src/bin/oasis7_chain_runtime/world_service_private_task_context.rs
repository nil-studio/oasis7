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
    let provider_authority =
        oasis7::viewer::ViewerRuntimeLiveServer::canonical_agent_service_context(
            &pinned.world,
            &fence.agent_id,
        )?;
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
        schema_revision: 1,
        control,
        read_commit: pinned.commit.clone(),
        activation_commit: historical.commit,
        active_goal,
        actor_observation_schema: "world-agent-state/v1".into(),
        actor_observation: serde_json::to_value(actor).map_err(|_| PrivateError::Unavailable)?,
        provider_authority,
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
    let current = oasis7::viewer::ViewerRuntimeLiveServer::canonical_agent_service_context(
        &pinned.world,
        &record.identity.fence.agent_id,
    )?;
    if current.capability_authorization_root
        != record
            .snapshot
            .provider_authority
            .capability_authorization_root
    {
        return Err(record_live_conflict(
            point,
            RecordLiveConflict::CapabilityAuthorizationRootChanged,
        ));
    }
    if current.authority_context_hash != record.snapshot.provider_authority.authority_context_hash {
        return Err(record_live_conflict(
            point,
            RecordLiveConflict::AuthorityContextHashChanged,
        ));
    }
    if current.capability_snapshot_hash
        != record.snapshot.provider_authority.capability_snapshot_hash
    {
        return Err(record_live_conflict(
            point,
            RecordLiveConflict::CapabilitySnapshotHashChanged,
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
            provider_authority:
                oasis7::viewer::ViewerRuntimeLiveServer::canonical_agent_service_context(
                    &pinned.world,
                    &proof.request.context.fence.agent_id,
                )?,
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
            let output = snapshot(service, &task, &proof.request.fence, store)?;
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
                        snapshot(service, &task, &proof.request.fence, store)?,
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
mod diagnostic_tests {
    use super::*;
    use oasis7::world_service::private_goal_store::client_state::{ClientStateKind, ClientStateV1};
    fn stage<T, E>(height: u64, name: &str, result: Result<T, E>) -> Option<T> {
        println!(
            "TASK_CONTEXT_DIAGNOSTIC height={height} stage={name} passed={}",
            result.is_ok()
        );
        result.ok()
    }
    #[test]
    #[ignore = "explicit readonly failed prepared-world diagnosis; no private body output"]
    fn private_task_context_failed_live_stage_diagnostic() {
        let ready = std::path::PathBuf::from(
            std::env::var_os("OASIS7_TASK_DIAGNOSTIC_READY")
                .expect("explicit failed ready file required"),
        );
        let ready: Value = serde_json::from_slice(&std::fs::read(ready).unwrap()).unwrap();
        let root = std::path::PathBuf::from(ready["root"].as_str().unwrap());
        let config: Value =
            serde_json::from_slice(&std::fs::read(ready["config"].as_str().unwrap()).unwrap())
                .unwrap();
        let key = oasis7::world_service::private_key_file::read_private_key_file(
            std::path::Path::new(config["signer_file"].as_str().unwrap()),
        )
        .expect("diagnostic delegate key guard");
        let cache_key = Zeroizing::new(blake3::derive_key(
            "oasis7/private-task-client-key/v1",
            key.as_slice(),
        ));
        let device = PrivateGoalStore::open(
            std::path::Path::new(config["state_directory"].as_str().unwrap()),
            *cache_key,
        )
        .expect("diagnostic existing cache guard");
        let Some(ClientStateV1::ContextPrepare(original)) = device
            .read_client_state(ClientStateKind::Prepare, "play-fe567997-r1")
            .expect("diagnostic original cache record")
        else {
            panic!("diagnostic original request missing");
        };
        assert!(
            verify_prepare(&original).is_ok(),
            "diagnostic original signature/schema"
        );
        let records = root.join("records");
        let storage = root.join("store");
        let identity =
            world_service_read::identity(&root.join("world"), "w1").expect("diagnostic identity");
        let private = PrivateGoalStore::open(&root.join("private-goals"), [7; 32])
            .expect("diagnostic existing private store guard");
        let service_key = hex::encode([9; 32]);
        let signer = FeedbackSubmitSigner {
            public_key_hex: sign_read_request("service", (), &service_key)
                .unwrap()
                .subject_public_key,
            private_key_hex: service_key,
        };
        let runtime = Arc::new(Mutex::new(NodeRuntime::new(
            oasis7_node::NodeConfig::new("diagnostic", "w1", oasis7_node::NodeRole::Sequencer)
                .unwrap(),
        )));
        let service = Service {
            records: &records,
            storage: &storage,
            identity,
            signer: &signer,
            runtime: &runtime,
            guarded: None,
        };
        let mut completed = 0;
        for height in [40, 50, 60] {
            let Some(pin) = stage(
                height,
                "current_cas_and_history",
                service.pin_at_height(height),
            ) else {
                continue;
            };
            if stage(
                height,
                "executor_fence",
                game_task::validate_task_executor(
                    &pin.world,
                    &original.request.fence,
                    &original.subject_public_key,
                ),
            )
            .is_none()
            {
                continue;
            }
            let Some(task) = stage(
                height,
                "validated_pinned_task",
                validate_pinned_task(
                    &service,
                    &pin,
                    &original.request.fence,
                    &original.subject_public_key,
                    true,
                ),
            ) else {
                continue;
            };
            let Some(Some(control)) = stage(
                height,
                "task_control",
                game_task::current_control(&pin.world, "agent-a"),
            ) else {
                continue;
            };
            let Some(Some(publication)) = stage(
                height,
                "active_publish",
                game_task::active_publish(&pin.world, "agent-a"),
            ) else {
                continue;
            };
            let Some(historical) = stage(
                height,
                "activation_cas_and_history",
                service.pin_at_height(publication.committed_height),
            ) else {
                continue;
            };
            let key = correlation::key_digest(&publication.request.correlation.key).unwrap();
            assert_eq!(
                historical
                    .world
                    .capability_revocation_state()
                    .world_service_results
                    .get(&key),
                Some(&serde_json::to_value(&publication).unwrap()),
                "diagnostic exact public activation proof"
            );
            let WorldServicePayloadV1::GameGoalPublish(public) =
                &publication.request.signed_payload
            else {
                panic!("diagnostic activation kind");
            };
            let Some(active_goal) = stage(
                height,
                "encrypted_goal_lookup",
                private.get_prepared(&public.request),
            ) else {
                continue;
            };
            let Some(authority) = stage(
                height,
                "actual_provider_authority",
                oasis7::viewer::ViewerRuntimeLiveServer::canonical_agent_service_context(
                    &pin.world, "agent-a",
                ),
            ) else {
                continue;
            };
            let Some(runtime_binding) = stage(
                height,
                "actual_runtime_binding",
                pin.world.current_cognition_runtime_binding(),
            ) else {
                continue;
            };
            let Some(captured_base_binding) = stage(
                height,
                "actual_cognition_base",
                pin.world.current_cognition_base_binding(),
            ) else {
                continue;
            };
            println!(
                "TASK_CONTEXT_DIAGNOSTIC height={height} state_time={} runtime_tick={} catalog_tick={} catalog_until={}",
                pin.world.state().time,
                runtime_binding.base_tick,
                authority.capability_catalog.logical_tick,
                authority.capability_catalog.valid_until_tick
            );
            let output = GoalExecutionSnapshotV1 {
                schema_revision: 1,
                control,
                read_commit: pin.commit.clone(),
                activation_commit: historical.commit,
                active_goal,
                actor_observation_schema: "world-agent-state/v1".into(),
                actor_observation: serde_json::to_value(&pin.world.state().agents["agent-a"])
                    .unwrap(),
                provider_authority: authority,
                runtime_binding,
                captured_base_binding,
            };
            if stage(height, "complete_snapshot_validation", output.validate()).is_none() {
                continue;
            }
            assert!(
                snapshot(&service, &task, &original.request.fence, &private).is_ok(),
                "diagnostic production snapshot"
            );
            let scratch = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "oasis7-private-context-diagnostic-{}-{height}",
                std::process::id()
            ));
            assert!(!scratch.exists());
            let scratch_store = PrivateGoalStore::open(&scratch, [81; 32]).unwrap();
            let prepared = stage(
                height,
                "context_create_encrypt_publish",
                scratch_store.prepare_task_context(&original, output),
            );
            if let Some(prepared) = prepared {
                assert!(
                    check_record_live(&pin, &prepared, None).is_ok(),
                    "diagnostic unchanged live authority"
                );
                assert!(
                    prepared.validate_against(&original).is_ok(),
                    "diagnostic exact original consumer proof"
                );
                completed += 1;
            }
            drop(scratch_store);
            std::fs::remove_dir_all(scratch).unwrap();
        }
        assert_eq!(
            completed, 3,
            "all actual stop-before snapshots must prepare complete private contexts"
        );
    }
}
