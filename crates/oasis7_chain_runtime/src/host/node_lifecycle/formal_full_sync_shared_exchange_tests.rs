//! Real signed session consumes one durable transport across both read and
//! material routes. This is an explicit injected operation, not V2 boot/recovery.
use super::super::super::local_io;
use super::super::{
    operation_ledger::{Identity, Ledger, Policy},
    shared_exchange::PreparedTransport,
};
use super::failover_tests::{config, local_instance};
use super::*;

use super::super::operation_worker;

#[test]
fn shared_operation_binds_real_formal_read_and_material_routes() {
    exercise_shared_operation(false);
}

#[test]
fn shared_operation_survives_provider_rebuild_without_new_budget() {
    exercise_shared_operation(true);
}

fn exercise_shared_operation(failover: bool) {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let (mut server, endpoint, key, runtime, _, _) =
        start_live_service(world, trust.clone(), anchor, material, proof);
    let node_key = test_key(92);
    let unavailable = TcpListener::bind("127.0.0.1:0").unwrap();
    let unavailable_endpoint = format!("http://{}", unavailable.local_addr().unwrap());
    drop(unavailable);
    let config = if failover {
        config(
            unavailable_endpoint.clone(),
            key.clone(),
            vec![FormalFullSyncEndpointV1 {
                endpoint: endpoint.clone(),
                trusted_service_public_key: key,
            }],
            trust,
            &node_key,
        )
    } else {
        config(endpoint.clone(), key, vec![], trust, &node_key)
    };
    let directory = TestPrivateDir::new();
    let instance = local_instance("shared-operation");
    let mut session = FormalFullSyncSession::new(
        config.clone(),
        hex::encode(node_key.to_bytes()),
        &directory.0.join("sync"),
        &instance,
    )
    .unwrap();
    let policy = Policy {
        api_version: "oasis7.formal-full-sync-transfer-policy/v1".into(),
        attempt_timeout_ms: 60_000,
        max_attempts: 2,
        max_encoded_io_bytes: 64 * 1024 * 1024,
        max_exchanges: 256,
        max_proof_pages: 64,
        max_proof_bytes: 16 * 1024 * 1024,
        max_inflight_requests: 1,
        reopen_timeout_ms: 10_000,
    };
    let endpoints: Vec<_> = config
        .providers()
        .iter()
        .map(|provider| provider.endpoint.clone())
        .collect();
    let prepared = PreparedTransport::prepare_builtin_trust(&endpoints).unwrap();
    let original_request = session
        .prepare_operation_request(&policy, &prepared)
        .unwrap();
    let original_anchor = session.client.formal_current_anchor().unwrap();
    assert_eq!(
        original_request["initial_minimum_record"],
        serde_json::to_value(original_anchor.head().record()).unwrap()
    );
    assert_eq!(
        original_request["initial_minimum"],
        serde_json::to_value(cut_from_anchor(&original_anchor).unwrap()).unwrap()
    );
    let wrong = PreparedTransport::prepare_builtin_trust(&["http://127.0.0.1:1".into()]).unwrap();
    assert!(session.prepare_operation_request(&policy, &wrong).is_err());
    let identity = Identity {
        operation_id: local_io::random_id().unwrap(),
        request_digest: local_io::digest(&original_request).unwrap(),
        policy_digest: local_io::digest(&policy).unwrap(),
        binding_digest: local_io::digest(&instance).unwrap(),
    };
    let mut ledger = Ledger::create_with_material_limit(
        &directory.0.join("operation.json"),
        identity.clone(),
        policy.clone(),
        config.max_total_sync_bytes,
    )
    .unwrap();
    ledger.bind_original_request(&original_request).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let shared = Arc::new(prepared.admit(ledger, attempt).unwrap());
    let mut wrong_request = original_request.clone();
    wrong_request["initial_minimum_record"] = serde_json::Value::Null;
    assert!(
        session
            .bind_shared_exchange(shared.clone(), &wrong_request)
            .is_err()
    );
    assert_eq!(shared.snapshot().exchanges_started, 0);
    session
        .bind_shared_exchange(shared.clone(), &original_request)
        .unwrap();
    // Signed formal view uses game_client's actual verification path.
    if failover {
        assert!(session.read_current_view().is_err());
    } else {
        session.read_current_view().unwrap();
        let before_clone = shared.snapshot().exchanges_started;
        let independent = session.client.independent_clone().unwrap();
        independent
            .read_view(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: WorldIdentity {
                    world_id: config.world_id.clone(),
                    genesis_digest: config.genesis_digest.clone(),
                },
                scope_id: "public".into(),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            })
            .unwrap();
        assert!(shared.snapshot().exchanges_started > before_clone);
    }
    let after_view = shared.snapshot();
    assert!(after_view.exchanges_started > 0);
    if !failover {
        assert!(after_view.encoded_tx_actual > 0 && after_view.encoded_rx_actual > 0);
    }
    session.sync_latest().unwrap();
    let after_sync = shared.snapshot();
    assert_eq!(after_sync.identity, identity);
    assert_eq!(after_sync.attempts_used, 1);
    assert!(after_sync.verified_target_json.is_some());
    assert!(after_sync.proof_pages > 0 && after_sync.proof_bytes > 0);
    assert!(after_sync.material_bytes > 0 && !after_sync.materials.is_empty());
    assert!(after_sync.exchanges_started > after_view.exchanges_started);
    assert!(after_sync.encoded_tx_actual > after_view.encoded_tx_actual);
    assert!(after_sync.encoded_rx_actual > after_view.encoded_rx_actual);
    assert!(after_sync.active_request.is_none());
    assert!(after_sync.reservation.is_none());
    assert!(after_sync.active_attempt_nonce.is_none());
    assert!(session.sync_latest().is_err());
    assert!(!session.status().ready);
    assert!(!session.status().state_root_verified);
    assert!(session.status().loaded_cut.is_none());
    assert_eq!(shared.snapshot(), after_sync);
    drop(session);
    drop(shared);
    let reopened = Ledger::open(
        &directory.0.join("operation.json"),
        &identity,
        &policy,
        config.max_total_sync_bytes,
    )
    .unwrap();
    assert_eq!(reopened.snapshot(), &after_sync);
    drop(reopened);
    server.stop().unwrap();
    runtime.lock().unwrap().stop().unwrap();
}

#[test]
fn prepared_transport_rejects_invalid_origins_before_attempt_admission() {
    for endpoint in [
        "http://example.com",
        "https://example.com",
        "http://127.0.0.1/path",
        "http://user@127.0.0.1",
        "http://127.0.0.1?query=1",
        "http://127.0.0.1:0",
    ] {
        assert!(PreparedTransport::prepare(&[endpoint.to_owned()], None).is_err());
    }
    assert!(PreparedTransport::prepare(&[], None).is_err());
    assert!(
        PreparedTransport::prepare(
            &["http://127.0.0.1".into(), "http://127.0.0.1:80/".into()],
            None
        )
        .is_err()
    );
    assert!(PreparedTransport::prepare(&["http://127.0.0.1:1".into()], None).is_ok());
}

#[test]
fn prepared_transport_cannot_reactivate_a_finished_attempt() {
    let directory = TestPrivateDir::new();
    let policy = Policy {
        api_version: "oasis7.formal-full-sync-transfer-policy/v1".into(),
        attempt_timeout_ms: 1000,
        max_attempts: 2,
        max_encoded_io_bytes: 1024,
        max_exchanges: 2,
        max_proof_pages: 1,
        max_proof_bytes: 1024,
        max_inflight_requests: 1,
        reopen_timeout_ms: 1000,
    };
    let identity = Identity {
        operation_id: local_io::random_id().unwrap(),
        request_digest: "11".repeat(32),
        policy_digest: local_io::digest(&policy).unwrap(),
        binding_digest: "22".repeat(32),
    };
    let prepared = PreparedTransport::prepare(&["http://127.0.0.1:1".into()], None).unwrap();
    let path = directory.0.join("operation.json");
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    ledger.finish_attempt(&attempt).unwrap();
    let terminated = ledger.snapshot().clone();
    assert!(prepared.admit(ledger, attempt).is_err());
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &terminated);
}

#[test]
fn original_request_cannot_be_attached_to_another_transport_policy() {
    let directory = TestPrivateDir::new();
    let policy = Policy {
        api_version: "oasis7.formal-full-sync-transfer-policy/v1".into(),
        attempt_timeout_ms: 1000,
        max_attempts: 2,
        max_encoded_io_bytes: 1024,
        max_exchanges: 2,
        max_proof_pages: 1,
        max_proof_bytes: 1024,
        max_inflight_requests: 1,
        reopen_timeout_ms: 1000,
    };
    let original_transport =
        PreparedTransport::prepare_builtin_trust(&["http://127.0.0.1:1".into()]).unwrap();
    let wrong_transport =
        PreparedTransport::prepare_builtin_trust(&["http://127.0.0.1:2".into()]).unwrap();
    assert_ne!(
        original_transport.binding_digest(),
        wrong_transport.binding_digest()
    );
    let original_request = serde_json::json!({"transport_binding":original_transport.binding_digest(),"transfer_policy":&policy,"config":{"max_total_sync_bytes":0}});
    let identity = Identity {
        operation_id: local_io::random_id().unwrap(),
        request_digest: local_io::digest(&original_request).unwrap(),
        policy_digest: local_io::digest(&policy).unwrap(),
        binding_digest: "22".repeat(32),
    };
    let path = directory.0.join("operation.json");
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    ledger.bind_original_request(&original_request).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let before = ledger.snapshot().clone();
    let error = match wrong_transport.admit(ledger, attempt) {
        Ok(_) => panic!("different transport was admitted"),
        Err(error) => error,
    };
    assert!(error.contains("transport policy differs from the original operation request"));
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &before);
    assert_eq!(reopened.snapshot().exchanges_started, 0);
}

#[test]
fn v4_config_and_issuer_grant_bind_the_real_owned_worker_to_one_budget() {
    exercise_real_owned_worker(None);
}

#[test]
fn selected_target_cold_recovery_does_not_follow_an_advanced_provider() {
    exercise_real_owned_worker(Some(false));
}

#[test]
fn selected_target_cold_recovery_replays_original_boundary_with_a_later_durable_head() {
    exercise_real_owned_worker(Some(true));
}

fn exercise_real_owned_worker(selected: Option<bool>) {
    use super::super::super::{actions, model, process};
    use oasis7_runtime::BlobStore;
    use operation_worker::InitialBudgetedSession;

    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let successor = selected.map(|_| same_epoch_successor_fixture(&world, &anchor, &material));
    let (mut server, endpoint, key, runtime, published, history) =
        start_live_service(world, trust.clone(), anchor, material, proof);
    let mut seed = config(endpoint, key, vec![], trust, &test_key(92));
    seed.grant = None;
    let mut wire = serde_json::to_value(&seed).unwrap();
    wire["api_version"] = serde_json::json!("oasis7.formal-full-state-sync/v2");
    wire["transfer_policy"] = serde_json::json!({
        "api_version": "oasis7.formal-full-sync-transfer-policy/v1",
        "attempt_timeout_ms": 60000,
        "max_attempts": 2,
        "max_encoded_io_bytes": 67108864,
        "max_exchanges": 256,
        "max_proof_pages": 64,
        "max_proof_bytes": 16777216,
        "max_inflight_requests": 1,
        "reopen_timeout_ms": 10000
    });
    let chain = serde_json::json!({
        "api_version": "oasis7.chain-config/v4",
        "runtime_profile": PROFILE,
        "formal_full_sync": wire
    });
    let directory = TestPrivateDir::new();
    let anchor = model::ChainConfig::load_bytes(
        &directory.0.join("chain.json"),
        &serde_json::to_vec(&chain).unwrap(),
    )
    .unwrap();
    let data = directory.0.join("node");
    let node = model::initialize(anchor, &data, None, "127.0.0.1:43170").unwrap();
    let instance_for = |node: &model::NodeConfig| LocalInstance {
        config_digest: local_io::digest(node).unwrap(),
        incarnation: local_io::random_id().unwrap(),
        ..local_instance("v4-owned")
    };
    let initial = actions::status(&data).unwrap();
    let initial = &initial["formal_full_state_sync"];
    assert_eq!(initial["api_version"], "oasis7.formal-full-sync-status/v2");
    assert_eq!(initial["blocker"], "operation_worker_inactive");
    assert!(initial["operation"].is_null());

    let owner = Arc::new(process::lock(&data).unwrap());
    let denied = InitialBudgetedSession::prepare(&node, &instance_for(&node), Arc::clone(&owner));
    assert!(
        denied
            .err()
            .unwrap()
            .contains("requires an independent issuer grant")
    );
    assert!(!data.join("state/full-sync").exists());
    drop(owner);

    let (keypair, _) =
        super::super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &data.join("keys"),
        ).unwrap();
    let private: [u8; 32] = hex::decode(keypair.private_key_hex)
        .unwrap()
        .try_into()
        .unwrap();
    let grant = signed_grant(&SigningKey::from_bytes(&private), now_unix_ms().unwrap(), 1);
    let grant_path = directory.0.join("grant.json");
    local_io::write(&grant_path, &grant).unwrap();
    install_grant(&data, &grant_path).unwrap();
    let node = model::NodeConfig::load(&data.join(model::CONFIG_FILE)).unwrap();
    let instance = instance_for(&node);
    let owner = Arc::new(process::lock(&data).unwrap());
    let owner_digest = owner.binding_digest().unwrap();
    let mut worker = InitialBudgetedSession::prepare(&node, &instance, Arc::clone(&owner)).unwrap();
    assert!(process::lock(&data).is_err());
    let advance_provider = || {
        let newer = successor.as_ref().unwrap();
        history.push(newer.clone());
        published
            .publish(newer.world.clone(), newer.anchor.clone())
            .unwrap();
    };
    let target = selected.map(|head_ahead| {
        let target = worker.sample_selected_target(true).unwrap();
        advance_provider();
        if head_ahead {
            assert_ne!(worker.sample_selected_target(false).unwrap(), target);
        }
        target
    });
    let ledger_path = data.join("state/full-sync/operation.json");
    let original = super::super::operation_snapshot::read_snapshot(&ledger_path).unwrap();
    assert_eq!(original.attempts_used, 1);
    assert_eq!(original.verified_target_json.is_some(), selected.is_some());
    assert!(original.active_attempt_nonce.is_some());
    drop(worker);
    drop(owner);

    let new_instance = LocalInstance {
        generation: 2,
        ..instance_for(&node)
    };
    let owner = Arc::new(process::lock(&data).unwrap());
    let worker = if let Some(target) = &target {
        let cas = LocalCasStore::new(data.join("state/full-sync"));
        let orphan = cas
            .put_bytes(b"retain unrelated CAS during cold admission")
            .unwrap();
        if selected == Some(false) {
            let original_disk: serde_json::Value = local_io::read(&ledger_path).unwrap();
            let mut wrong = original_disk.clone();
            wrong["verified_target_json"] = serde_json::to_value(
                serde_json::to_vec(&(target, serde_json::Value::Null)).unwrap(),
            )
            .unwrap();
            let mut pending = original_disk.clone();
            pending["publication"] = serde_json::json!({
                "intent_digest": "11".repeat(32),
                "attempt_no": original.attempts_used,
                "completed": false,
            });
            for (invalid, expected) in [
                (wrong, "target differs"),
                (pending, "unpublished continuity is unavailable"),
            ] {
                local_io::write(&ledger_path, &invalid).unwrap();
                let before = std::fs::read(&ledger_path).unwrap();
                let error = InitialBudgetedSession::resume_selected_target(
                    &node,
                    &new_instance,
                    Arc::clone(&owner),
                )
                .err()
                .expect("invalid selected operation resumed");
                assert!(error.contains(expected), "{error}");
                assert_eq!(std::fs::read(&ledger_path).unwrap(), before);
            }
            local_io::write(&ledger_path, &original_disk).unwrap();
        }
        let result = InitialBudgetedSession::resume_selected_target(
            &node,
            &new_instance,
            Arc::clone(&owner),
        );
        assert_eq!(
            cas.get(&orphan).unwrap(),
            b"retain unrelated CAS during cold admission"
        );
        result.unwrap()
    } else {
        InitialBudgetedSession::resume_pre_target(&node, &new_instance, Arc::clone(&owner)).unwrap()
    };
    let thread = owner.spawn_bound_worker(move || worker.run_once()).unwrap();
    let (status, snapshot) = thread.join().unwrap().unwrap();
    assert!(status.loaded_cut.is_some());
    if let Some(target) = target {
        assert_eq!(status.loaded_cut.as_ref(), Some(&target));
        assert_eq!(snapshot.verified_target_json, original.verified_target_json);
    }
    assert!(!status.ready);
    assert_eq!(snapshot.identity.binding_digest, owner_digest);
    assert_eq!(snapshot.attempts_used, 2);
    assert_eq!(snapshot.identity, original.identity);
    assert_eq!(snapshot.interrupted_attempts.len(), 1);
    assert!(snapshot.publication.as_ref().unwrap().completed);
    assert!(snapshot.encoded_rx_actual > 0 && snapshot.encoded_tx_actual > 0);
    assert!(snapshot.material_bytes > 0 && snapshot.proof_pages > 0);
    assert!(snapshot.active_attempt_nonce.is_none());
    assert!(snapshot.reservation.is_none());

    let status = actions::status(&data).unwrap();
    let projected = &status["formal_full_state_sync"];
    assert_eq!(projected["blocker"], "operation_worker_inactive");
    assert_eq!(projected["ready"], false);
    let operation = &projected["operation"];
    assert_eq!(operation["operation_id"], snapshot.identity.operation_id);
    assert_eq!(operation["revision"], snapshot.revision);
    assert_eq!(operation["publication"], "pins_acknowledged");
    assert_eq!(
        operation["encoded_io"]["tx_actual_known"],
        snapshot.encoded_tx_actual
    );
    assert_eq!(
        operation["encoded_io"]["rx_actual_known"],
        snapshot.encoded_rx_actual
    );
    assert_eq!(operation["encoded_io"]["unresolved_reserved"], 0);
    assert!(operation["remaining_ms"].is_null());

    let owner = Arc::new(process::lock(&data).unwrap());
    let before = std::fs::read(&ledger_path).unwrap();
    let selected =
        InitialBudgetedSession::resume_selected_target(&node, &new_instance, Arc::clone(&owner));
    assert!(
        selected
            .err()
            .unwrap()
            .contains("unpublished continuity is unavailable")
    );
    let resumed = InitialBudgetedSession::prepare(&node, &instance, owner);
    assert!(
        resumed
            .err()
            .unwrap()
            .contains("original full-sync state exists")
    );
    assert_eq!(std::fs::read(&ledger_path).unwrap(), before);
    server.stop().unwrap();
    runtime.lock().unwrap().stop().unwrap();
}
