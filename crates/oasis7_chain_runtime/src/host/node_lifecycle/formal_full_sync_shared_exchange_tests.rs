//! Real signed session consumes one durable transport across both read and
//! material routes. This is an explicit injected operation, not V2 boot/recovery.
use super::super::super::local_io;
use super::super::{
    operation_ledger::{Identity, Ledger, Policy},
    shared_exchange::PreparedTransport,
};
use super::failover_tests::{config, local_instance};
use super::*;

#[test]
fn shared_operation_binds_real_formal_read_and_material_routes() {
    exercise_shared_operation(false, false);
}

#[test]
fn shared_operation_survives_provider_rebuild_without_new_budget() {
    exercise_shared_operation(true, false);
}

#[test]
fn pre_target_recovery_rebinds_real_signed_and_material_consumers() {
    exercise_shared_operation(false, true);
}

fn exercise_shared_operation(failover: bool, recover: bool) {
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
    local_io::private_dir(&directory.0.join("state")).unwrap();
    let sync_root = directory.0.join("state/full-sync");
    let ledger_path = sync_root.join("operation.json");
    let mut session = FormalFullSyncSession::new(
        config.clone(),
        hex::encode(node_key.to_bytes()),
        &sync_root,
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
        &ledger_path,
        identity.clone(),
        policy.clone(),
        config.max_total_sync_bytes,
    )
    .unwrap();
    ledger.bind_original_request(&original_request).unwrap();
    let original_owner = if recover {
        drop(local_io::private_file(&directory.0.join("process.lock")).unwrap());
        let owner = Arc::new(super::super::super::process::lock(&directory.0).unwrap());
        ledger.bind_owner(owner.clone(), &directory.0).unwrap();
        Some(owner)
    } else {
        None
    };
    let attempt = ledger.begin_attempt().unwrap();
    let shared = if recover {
        let _interrupted = ledger.begin_exchange(&attempt, 17, 23).unwrap();
        drop(ledger);
        drop(original_owner);
        drop(session);
        drop(local_io::private_file(&directory.0.join("process.lock")).unwrap());
        let owner = Arc::new(super::super::super::process::lock(&directory.0).unwrap());
        session = FormalFullSyncSession::new(
            config.clone(),
            hex::encode(node_key.to_bytes()),
            &sync_root,
            &instance,
        )
        .unwrap();
        let reopened = Ledger::open(
            &ledger_path,
            &identity,
            &policy,
            config.max_total_sync_bytes,
        )
        .unwrap();
        session
            .resume_pre_target_operation(reopened, owner, &directory.0, prepared)
            .unwrap()
    } else {
        Arc::new(prepared.admit(ledger, attempt).unwrap())
    };
    let mut wrong_request = original_request.clone();
    wrong_request["initial_minimum_record"] = serde_json::Value::Null;
    assert!(
        session
            .bind_shared_exchange(shared.clone(), &wrong_request)
            .is_err()
    );
    assert_eq!(shared.snapshot().exchanges_started, u64::from(recover));
    if !recover {
        session
            .bind_shared_exchange(shared.clone(), &original_request)
            .unwrap();
    }
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
    assert_eq!(after_sync.attempts_used, if recover { 2 } else { 1 });
    assert_eq!(after_sync.interrupted_attempts.len(), usize::from(recover));
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
        &ledger_path,
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
