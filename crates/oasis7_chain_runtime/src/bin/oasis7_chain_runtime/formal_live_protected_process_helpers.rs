//! Shared assertions and requests for actual protected process scenarios.
use super::*;

pub(super) fn assert_live_descends_from_view(
    runtime: &FormalLiveRuntime,
    current: &crate::formal_live_service::PublishedReadCut,
    view: &oasis7::world_service::verified_view::VerifiedWorldView,
) -> Result<(), String> {
    let observed = view
        .formal_read_anchor()
        .ok_or("verified HTTP read anchor missing")?;
    if observed.head().record().position != view.version().commit.position
        || observed.head().head().record_id != view.version().commit.execution_block_hash
        || current.anchor.head().record().position < observed.head().record().position
    {
        return Err("verified HTTP read differs from its exact anchor or exceeds live cut".into());
    }
    runtime.history.history_page(
        observed,
        &current.anchor,
        oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS,
    )?;
    Ok(())
}

pub(super) fn world_with_owner_seed() -> (oasis7_runtime::World, SigningKey) {
    let owner = fixtures::key(71);
    let world = oasis7_runtime::World::new_with_state(WorldState::default())
        .try_with_cognition_scheduler(
            SchedulerPolicyV1 {
                schema_version: SchedulerPolicyV1::SCHEMA_VERSION.into(),
                max_total_wakes_per_tick: 1,
                max_wakes_per_agent_per_tick: 1,
                aging_after_ticks: 1,
                max_starvation_ticks: 1,
                initial_priority: 0,
                comparator: SchedulerPolicyV1::COMPARATOR.into(),
                service_order: SchedulerPolicyV1::SERVICE_ORDER.into(),
            },
            1,
        )
        .expect("fixture cognition scheduler");
    (world, owner)
}

pub(super) fn install_wasmtime_action_observer(
    world: &mut oasis7_runtime::World,
) -> Result<(), String> {
    let mut hasher = Sha256::new();
    hasher.update(WASM_EXECUTOR_TEST_MODULE);
    let wasm_hash = hex::encode(hasher.finalize());
    world
        .register_module_artifact(wasm_hash.clone(), WASM_EXECUTOR_TEST_MODULE)
        .map_err(|error| format!("register Wasmtime fixture: {error:?}"))?;
    let module = ModuleManifest {
        module_id: "m.formal-live-wasm-observer".into(),
        name: "FormalLiveWasmtimeObserver".into(),
        version: "0.1.0".into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".into()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(&wasm_hash)),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 10_000_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 2,
        },
    };
    let changes = ModuleChangeSet {
        register: vec![module.clone()],
        activate: vec![ModuleActivation {
            module_id: module.module_id.clone(),
            version: module.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".into(),
        serde_json::to_value(changes).map_err(|error| error.to_string())?,
    );
    let manifest = Manifest {
        version: 2,
        content: Value::Object(content),
    };
    let proposal = world
        .propose_manifest_update(manifest, "alice")
        .map_err(|error| format!("propose Wasmtime fixture: {error:?}"))?;
    world
        .shadow_proposal(proposal)
        .map_err(|error| format!("shadow Wasmtime fixture: {error:?}"))?;
    world
        .approve_proposal(proposal, "bob", ProposalDecision::Approve)
        .map_err(|error| format!("approve Wasmtime fixture: {error:?}"))?;
    world
        .apply_proposal(proposal)
        .map(|_| ())
        .map_err(|error| format!("activate Wasmtime fixture: {error:?}"))
}

pub(super) fn request_for_owner(
    owner: &SigningKey,
    world: &WorldIdentity,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let create = GameTaskCreateV1 {
        api_rev: 1,
        client_operation_id: "formal-live-protected-operation-1".into(),
        world_id: world.world_id.clone(),
        genesis_digest: world.genesis_digest.clone(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
    };
    let payload = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            create,
            &hex::encode(owner.to_bytes()),
        )
        .unwrap(),
    );
    SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: correlation::derive_correlation(world.clone(), &payload).unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}

pub(super) fn request_for_first_agent(
    owner: &SigningKey,
    world: &WorldIdentity,
) -> Result<SubmitIntentRequest<WorldServicePayloadV1>, String> {
    let public_key = hex::encode(owner.verifying_key().to_bytes());
    let mut action = oasis7::viewer::GameplayActionRequest {
        action_id: oasis7::viewer::ACTION_CLAIM_FIRST_AGENT.into(),
        target_agent_id: oasis7::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.into(),
        actor_agent_id: None,
        player_id: "owner".into(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    action.auth = Some(
        oasis7::viewer::sign_gameplay_action_auth_proof(
            &action,
            1,
            &public_key,
            &hex::encode(owner.to_bytes()),
        )
        .map_err(|error| format!("sign initial genesis gameplay action: {error}"))?,
    );
    let payload = WorldServicePayloadV1::GameplayJson(
        serde_json::to_vec(&action).map_err(|error| error.to_string())?,
    );
    Ok(SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: correlation::derive_correlation(world.clone(), &payload)
            .map_err(|error| error.to_string())?,
        deadline_unix_ms: None,
        signed_payload: payload,
    })
}

pub(super) fn retained_result_from_hot_or_cold(
    world: &oasis7_runtime::World,
    result_key: &str,
) -> Result<oasis7::world_service::CanonicalIntentResultV1, String> {
    optional_retained_result_from_hot_or_cold(world, result_key)?
        .ok_or_else(|| "canonical result is absent from hot and authenticated cold history".into())
}

pub(super) fn optional_retained_result_from_hot_or_cold(
    world: &oasis7_runtime::World,
    result_key: &str,
) -> Result<Option<oasis7::world_service::CanonicalIntentResultV1>, String> {
    if let Some(value) = world
        .capability_revocation_state()
        .world_service_results
        .get(result_key)
    {
        correlation::validate_result(result_key, value)
            .map_err(|error| format!("validate hot canonical result: {error}"))?;
        return serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| format!("decode hot canonical result: {error}"));
    }

    world
        .read_retained_world_service_result(result_key)
        .map_err(|error| format!("authenticated cold result read failed: {error}"))
}

/// Only for a newly submitted generation whose predecessor was awaited before
/// submission. Its first result belongs to the bounded recent-result window;
/// absence means execution has not published yet, not a cold-history lookup.
pub(super) fn fresh_request_result(
    world: &oasis7_runtime::World,
    key: &str,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<Option<oasis7::world_service::CanonicalIntentResultV1>, String> {
    let Some(value) = world
        .capability_revocation_state()
        .world_service_results
        .get(key)
    else {
        return Ok(None);
    };
    correlation::validate_result(key, value)?;
    let result: oasis7::world_service::CanonicalIntentResultV1 =
        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    if result.request != *request || result.rejected.is_some() {
        return Err(format!(
            "fresh exact task outcome mismatch or rejection: {:?}",
            result.rejected
        ));
    }
    Ok(Some(result))
}

// Volatile admission has no execution SLO: a request can arrive during an
// already-running Tick and then require its own Tick. Long history fixtures
// share this observation bound and retain their separate total scenario bound.
pub(super) const LONG_HISTORY_REQUEST_WAIT: Duration = Duration::from_secs(120);

pub(super) fn wait_for_new_request_result(
    runtime: &FormalLiveRuntime,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    timeout: Duration,
    label: &str,
) -> Result<oasis7::world_service::CanonicalIntentResultV1, String> {
    let key =
        correlation::key_digest(&request.correlation.key).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let deadline = started + timeout;
    let mut next_diagnostic = Duration::from_secs(20);
    let observe = || {
        runtime.published.observe(|cut| {
            Ok((
                cut.anchor.head().record().position,
                fresh_request_result(&cut.world, &key, request)?,
            ))
        })
    };
    loop {
        let (position, result) = observe()?;
        if let Some(result) = result {
            return Ok(result);
        }
        let snapshot = runtime
            .runtime
            .lock()
            .map_err(|_| "fresh request runtime lock poisoned")?
            .snapshot();
        if let Some(error) = snapshot.last_error {
            return Err(format!("{label} task failed before publication: {error}"));
        }
        let now = Instant::now();
        if now >= deadline {
            // The atomic publication may advance while diagnostics are read.
            let (position, result) = observe()?;
            if let Some(result) = result {
                return Ok(result);
            }
            return Err(format!(
                "{label} task {key} missing after {}s at position {position}; running={} ticks={} slot={} height={} committed={} executed={} pending={:?} proposal={:?} status={:?}",
                timeout.as_secs(),
                snapshot.running,
                snapshot.tick_count,
                snapshot.consensus.slot,
                snapshot.consensus.latest_height,
                snapshot.consensus.committed_height,
                snapshot.consensus.last_execution_height,
                snapshot.consensus.pending_consensus_actions,
                snapshot.consensus.pending_proposal,
                snapshot.consensus.last_status,
            ));
        }
        if now.duration_since(started) >= next_diagnostic {
            eprintln!(
                "{label} task {key} still awaiting exact publication after {}s; position={position} running={} ticks={} slot={} height={} committed={} executed={} pending={:?} proposal={:?} status={:?}",
                now.duration_since(started).as_secs(),
                snapshot.running,
                snapshot.tick_count,
                snapshot.consensus.slot,
                snapshot.consensus.latest_height,
                snapshot.consensus.committed_height,
                snapshot.consensus.last_execution_height,
                snapshot.consensus.pending_consensus_actions,
                snapshot.consensus.pending_proposal,
                snapshot.consensus.last_status,
            );
            next_diagnostic += Duration::from_secs(20);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn fresh_request_absence_never_reads_cold_history() {
    let mut staged = oasis7_runtime::World::new();
    staged.initialize_empty_formal_history().unwrap();
    let world =
        oasis7_runtime::World::from_snapshot(staged.snapshot(), staged.journal().clone()).unwrap();
    let identity = WorldIdentity {
        world_id: "formal-fixture-world".into(),
        genesis_digest: fixtures::h(8),
    };
    let request = request_for_owner(&fixtures::key(71), &identity);
    let key = correlation::key_digest(&request.correlation.key).unwrap();
    assert!(world.read_retained_world_service_result(&key).is_err());
    assert!(
        fresh_request_result(&world, &key, &request)
            .unwrap()
            .is_none(),
        "a new request must wait without consulting unavailable cold storage"
    );
}

pub(super) fn wait_for_canonical_request_result(
    runtime: &FormalLiveRuntime,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<
    (
        crate::formal_live_service::PublishedReadCut,
        oasis7::world_service::CanonicalIntentResultV1,
    ),
    String,
> {
    let result_key =
        correlation::key_digest(&request.correlation.key).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let cut = runtime.published.current()?;
        if let Some(result) = optional_retained_result_from_hot_or_cold(&cut.world, &result_key)? {
            if result.request != *request {
                return Err(
                    "published canonical result differs from the exact submitted request".into(),
                );
            }
            if result.rejected.is_some() {
                return Err(format!(
                    "exact submitted request was canonically rejected: {:?}",
                    result.rejected
                ));
            }
            return Ok((cut, result));
        }

        let snapshot = runtime
            .runtime
            .lock()
            .map_err(|_| "Node runtime lock poisoned while waiting for canonical result")?
            .snapshot();
        if let Some(error) = snapshot.last_error {
            return Err(format!(
                "Node failed before publishing the exact canonical result: {error}"
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "exact canonical result {result_key} missing at qualified position {}; running={}, ticks={}",
                cut.anchor.head().record().position,
                snapshot.running,
                snapshot.tick_count,
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn lookup_original_commit(
    client: &RemoteWorldServiceClient,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<oasis7_client_api::world_service::CommitRef, String> {
    let response = client
        .lookup_formal_original(original.clone())
        .map_err(|error| format!("ordinary signed formal lookup: {error}"))?;
    if response.correlation != original.correlation
        || response.formal_profile.as_ref()
            != Some(&oasis7_client_api::world_service::FormalReadProfileV1::controlled())
        || response.execution_evidence_scope.is_some()
    {
        return Err("formal lookup response differs from the original request profile".into());
    }
    match response.outcome {
        oasis7_client_api::world_service::IntentOutcome::Committed { commit, .. } => Ok(*commit),
        outcome => Err(format!(
            "formal lookup did not prove a committed result: {outcome:?}"
        )),
    }
}

pub(super) fn assert_volatile_admission(
    observation: SubmitObservation<Value>,
    expected: &oasis7_client_api::world_service::RequestCorrelation,
) -> Result<(), String> {
    let response = match observation {
        SubmitObservation::Response(response) => *response,
        SubmitObservation::OutcomeUnknown(_) => {
            return Err("ordinary client could not verify formal-profile admission".into());
        }
    };
    if response.correlation != *expected
        || response.formal_profile.as_ref()
            != Some(&oasis7_client_api::world_service::FormalReadProfileV1::controlled())
        || response.execution_evidence_scope.is_some()
        || !matches!(
            response.outcome,
            oasis7_client_api::world_service::IntentOutcome::Received {
                durability: oasis7_client_api::world_service::AdmissionDurability::Volatile
            }
        )
    {
        return Err("ordinary client did not verify a volatile formal-profile admission".into());
    }
    Ok(())
}

pub(super) fn wait_for_published_position(
    runtime: &FormalLiveRuntime,
    position: u64,
) -> Result<(), String> {
    wait_for_published_position_until(runtime, position, Instant::now() + Duration::from_secs(20))
}

pub(super) fn wait_for_published_position_until(
    runtime: &FormalLiveRuntime,
    position: u64,
    deadline: Instant,
) -> Result<(), String> {
    loop {
        let published_position = runtime.published.position()?;
        if published_position >= position {
            return Ok(());
        }
        let snapshot = runtime
            .runtime
            .lock()
            .map_err(|_| "Node runtime lock poisoned during qualified publication wait")?
            .snapshot();
        if let Some(error) = snapshot.last_error {
            return Err(format!(
                "Node failed before qualified formal position {position}: {error}"
            ));
        }
        if Instant::now() >= deadline {
            // Publication can advance while the diagnostic snapshot is read.
            let published_position = runtime.published.position()?;
            if published_position >= position {
                return Ok(());
            }
            let pending_proposal = snapshot
                .consensus
                .pending_proposal
                .as_ref()
                .map(|proposal| {
                    format!(
                        "actions={},status={:?}",
                        proposal.action_count, proposal.status
                    )
                })
                .unwrap_or_else(|| "none".into());
            let last_error = if snapshot.last_error.is_some() {
                "present"
            } else {
                "none"
            };
            return Err(format!(
                "Node did not publish qualified formal position {position}; published_position={published_position}, running={}, ticks={}, last_tick_unix_ms={:?}, slot={}, height={}, committed={}, executed={}, submit_buffer_actions={}, queued_actions={}, pending_proposal={pending_proposal}, last_error={last_error}",
                snapshot.running,
                snapshot.tick_count,
                snapshot.last_tick_unix_ms,
                snapshot.consensus.slot,
                snapshot.consensus.latest_height,
                snapshot.consensus.committed_height,
                snapshot.consensus.last_execution_height,
                snapshot
                    .consensus
                    .pending_consensus_actions
                    .submit_buffer_action_count,
                snapshot
                    .consensus
                    .pending_consensus_actions
                    .queued_action_count,
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn read_current(
    client: &RemoteWorldServiceClient,
    world: &WorldIdentity,
) -> Result<oasis7::world_service::verified_view::VerifiedWorldView, String> {
    client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: world.clone(),
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .map_err(|error| error.to_string())
}

pub(super) fn read_until_position(
    client: &RemoteWorldServiceClient,
    world: &WorldIdentity,
    position: u64,
) -> Result<oasis7::world_service::verified_view::VerifiedWorldView, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let view = read_current(client, world)?;
        if view.version().commit.position >= position {
            return Ok(view);
        }
        if Instant::now() >= deadline {
            return Err("signed HTTP client did not observe the qualified action".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn write_endpoint_config(
    path: &Path,
    data_dir: &Path,
    oracle_auth: &Path,
    oracle_address: std::net::SocketAddr,
    role: EndpointRole,
    trust: &InitialActivationTrustV1,
    certificate: &SignedInitialActivationV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": 2,
        "initial_trust": trust,
        "initial_certificate": certificate,
        "initial_qualified": null,
        "recovery_bootstrap": null,
        "role": role,
        "data_dir": data_dir,
        "oracle_authentication_key_file": oracle_auth,
        "oracle_address": oracle_address,
        "listen": "127.0.0.1:0",
        "timeout_ms": 5000,
    }))
    .map_err(|error| error.to_string())?;
    oracle_support::write_private(path, &bytes)
}

pub(super) fn free_loopback_address() -> Result<std::net::SocketAddr, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    listener.local_addr().map_err(|error| error.to_string())
}

pub(super) fn wasm_emit_count(world: &oasis7_runtime::World) -> usize {
    world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                oasis7_runtime::WorldEventBody::ModuleEmitted(emission)
                    if emission.kind == "wasm.test"
            )
        })
        .count()
}

/// Background simulation may publish between submit and the ordinary read.
/// Attribute emissions to the exact original request's frozen/candidate pair.
pub(super) fn assert_admin_commit_did_not_emit_wasm(
    writer_directory: &Path,
    commit: &oasis7_client_api::world_service::CommitRef,
) -> Result<(), String> {
    let (candidate, proof) = crate::execution_bridge::formal_writer::read_published_candidate_at(
        writer_directory,
        commit.position,
    )?
    .ok_or("administrative exact Qualified publication missing")?;
    if candidate
        .candidate
        .record
        .record_id()
        .map_err(|error| error.to_string())?
        != commit.execution_block_hash
        || proof.decision.record != candidate.candidate.record
    {
        return Err("administrative publication is not the original Qualified request".into());
    }
    verify_record_material(&candidate.candidate.record, &candidate.material)
        .map_err(|error| error.to_string())?;
    let frozen = candidate
        .frozen
        .as_ref()
        .ok_or("administrative Frozen certificate missing")?;
    let before = candidate
        .frozen_material
        .as_ref()
        .ok_or("administrative Frozen closure missing")?;
    verify_frozen_material(&frozen.reservation.body, before).map_err(|error| error.to_string())?;
    let journal = |material: &oasis7_distfs::controlled_authority::replicated_protocol::ClosedRecord| -> Result<oasis7_runtime::Journal, String> {
        let hash = material.roots.get(&oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Journal)
            .ok_or("administrative Journal role missing")?;
        let object = material.objects.iter().find(|object| &object.content_hash == hash)
            .ok_or("administrative Journal object missing")?;
        serde_cbor::from_slice(&object.bytes).map_err(|error| error.to_string())
    };
    let before = journal(before)?;
    let after = journal(&candidate.material)?;
    if after.events.iter().any(|event| {
        matches!(&event.body, oasis7_runtime::WorldEventBody::ModuleEmitted(emission) if emission.kind == "wasm.test")
            && !before.events.contains(event)
    }) {
        return Err("administrative exact commit unexpectedly executed simulation modules".into());
    }
    Ok(())
}
