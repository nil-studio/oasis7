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
    exercise_shared_operation(false, false, false, false);
}

#[test]
fn shared_operation_survives_provider_rebuild_without_new_budget() {
    exercise_shared_operation(true, false, false, false);
}

#[test]
fn pre_target_recovery_rebinds_real_signed_and_material_consumers() {
    exercise_shared_operation(false, true, false, false);
}

#[test]
fn selected_target_recovery_keeps_original_cut_across_real_material_restore() {
    exercise_shared_operation(false, true, true, false);
}

#[test]
fn selected_target_recovery_rejects_later_durable_head_without_pruning() {
    exercise_shared_operation(false, true, true, true);
}

fn exercise_shared_operation(failover: bool, recover: bool, selected: bool, head_ahead: bool) {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let newer = selected.then(|| same_epoch_successor_fixture(&world, &anchor, &material));
    let (mut server, endpoint, key, runtime, published, history) =
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
    let shared = if selected {
        use oasis7_game_client::formal_sync_exchange::FullSyncExchange;
        let first = Arc::new(prepared.admit(ledger, attempt).unwrap());
        session
            .bind_shared_exchange(first.clone(), &original_request)
            .unwrap();
        let view = session.read_best_effort_current_view().unwrap();
        let anchor = verified_anchor(&view, &config).unwrap();
        let cut = cut_from_anchor(&anchor).unwrap();
        let target = serde_json::to_vec(&(&cut, anchor.head().record())).unwrap();
        first.bind_verified_target(&target).unwrap();
        if head_ahead {
            let newer = newer.as_ref().unwrap();
            history.push(newer.clone());
            published
                .publish(newer.world.clone(), newer.anchor.clone())
                .unwrap();
            session.read_best_effort_current_view().unwrap();
        }
        let store = Arc::clone(&session.cas);
        use oasis7_runtime::BlobStore;
        let orphan = store
            .put_bytes(b"recovery must not prune on rejected target")
            .unwrap();
        let prior = first.snapshot();
        assert!(prior.encoded_tx_actual > 0 && prior.encoded_rx_actual > 0);
        drop(first);
        drop(session);
        drop(original_owner);
        drop(local_io::private_file(&directory.0.join("process.lock")).unwrap());
        let owner = Arc::new(super::super::super::process::lock(&directory.0).unwrap());
        if !head_ahead {
            let original_disk: serde_json::Value = local_io::read(&ledger_path).unwrap();
            for fault in [
                "target-record",
                "pending-publication",
                "acknowledged-publication",
            ] {
                let mut changed = original_disk.clone();
                if fault == "target-record" {
                    let bytes = serde_json::to_vec(&(&cut, serde_json::Value::Null)).unwrap();
                    changed["verified_target_json"] = serde_json::to_value(bytes).unwrap();
                } else {
                    changed["publication"] = serde_json::json!({
                        "intent_digest":"11".repeat(32), "attempt_no":prior.attempts_used,
                        "completed":fault == "acknowledged-publication",
                    });
                }
                local_io::write(&ledger_path, &changed).unwrap();
                let before = std::fs::read(&ledger_path).unwrap();
                let bad = Ledger::open(
                    &ledger_path,
                    &identity,
                    &policy,
                    config.max_total_sync_bytes,
                )
                .unwrap();
                let prepared = PreparedTransport::prepare_builtin_trust(&endpoints).unwrap();
                assert!(
                    FormalFullSyncSession::recover_selected_target(
                        config.clone(),
                        hex::encode(node_key.to_bytes()),
                        &instance,
                        bad,
                        owner.clone(),
                        &directory.0,
                        prepared,
                    )
                    .is_err(),
                    "{fault} unexpectedly resumed"
                );
                assert_eq!(std::fs::read(&ledger_path).unwrap(), before);
                assert_eq!(
                    store.get(&orphan).unwrap(),
                    b"recovery must not prune on rejected target"
                );
                local_io::write(&ledger_path, &original_disk).unwrap();
            }
        }
        let reopened = Ledger::open(
            &ledger_path,
            &identity,
            &policy,
            config.max_total_sync_bytes,
        )
        .unwrap();
        let prepared = PreparedTransport::prepare_builtin_trust(&endpoints).unwrap();
        let before = std::fs::read(&ledger_path).unwrap();
        let result = FormalFullSyncSession::recover_selected_target(
            config.clone(),
            hex::encode(node_key.to_bytes()),
            &instance,
            reopened,
            owner,
            &directory.0,
            prepared,
        );
        if head_ahead {
            let error = match result {
                Ok(_) => panic!("later durable head accepted"),
                Err(e) => e,
            };
            assert!(error.contains("target differs"), "{error}");
            assert_eq!(std::fs::read(&ledger_path).unwrap(), before);
            assert_eq!(
                store.get(&orphan).unwrap(),
                b"recovery must not prune on rejected target"
            );
            server.stop().unwrap();
            runtime.lock().unwrap().stop().unwrap();
            return;
        }
        let (recovered_session, exchange) = result.unwrap();
        let recovered = exchange.snapshot();
        assert_eq!(recovered.verified_target_json, prior.verified_target_json);
        assert_eq!(recovered.encoded_tx_actual, prior.encoded_tx_actual);
        assert_eq!(recovered.encoded_rx_actual, prior.encoded_rx_actual);
        session = recovered_session;
        exchange
    } else if recover {
        let _interrupted = ledger.begin_exchange(&attempt, 17, 23).unwrap();
        drop(ledger);
        drop(original_owner);
        drop(session);
        drop(local_io::private_file(&directory.0.join("process.lock")).unwrap());
        let owner = Arc::new(super::super::super::process::lock(&directory.0).unwrap());
        let reopened = Ledger::open(
            &ledger_path,
            &identity,
            &policy,
            config.max_total_sync_bytes,
        )
        .unwrap();
        let (recovered_session, exchange) = FormalFullSyncSession::recover_pre_target(
            config.clone(),
            hex::encode(node_key.to_bytes()),
            &instance,
            reopened,
            owner,
            &directory.0,
            prepared,
        )
        .unwrap();
        session = recovered_session;
        exchange
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
    if !selected {
        assert_eq!(shared.snapshot().exchanges_started, u64::from(recover));
    }
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
    if selected {
        let newer = newer.as_ref().unwrap();
        history.push(newer.clone());
        published
            .publish(newer.world.clone(), newer.anchor.clone())
            .unwrap();
    }
    let after_view = shared.snapshot();
    assert!(after_view.exchanges_started > 0);
    if !failover {
        assert!(after_view.encoded_tx_actual > 0 && after_view.encoded_rx_actual > 0);
    }
    let restored = session.sync_latest().unwrap();
    if selected {
        let (target, _): (FormalFullSyncCutV1, serde_json::Value) =
            serde_json::from_slice(after_view.verified_target_json.as_ref().unwrap()).unwrap();
        assert_eq!(restored.cut, target);
        assert!(
            session
                .status()
                .last_verified_cut
                .as_ref()
                .unwrap()
                .position
                > target.position
        );
    }
    let after_sync = shared.snapshot();
    assert_eq!(after_sync.identity, identity);
    assert_eq!(after_sync.attempts_used, if recover { 2 } else { 1 });
    assert_eq!(after_sync.interrupted_attempts.len(), usize::from(recover));
    assert!(after_sync.verified_target_json.is_some());
    if selected {
        assert_eq!(
            after_sync.verified_target_json,
            after_view.verified_target_json
        );
    }
    assert!(after_sync.publication.as_ref().unwrap().completed);
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

#[test]
fn recovery_checks_publication_before_session_can_prune_cas() {
    use oasis7_runtime::BlobStore;
    for phase in [
        "selected",
        "pending",
        "acknowledged",
        "legacy",
        "changed-config",
        "minimum-record",
        "minimum-cut",
        "exhausted",
    ] {
        let (_, trust, _, _, _) = full_sync_fixture();
        let node_key = test_key(92);
        let mut config = config(
            "http://127.0.0.1:1".into(),
            hex::encode(test_key(93).verifying_key().to_bytes()),
            vec![],
            trust,
            &node_key,
        );
        let directory = TestPrivateDir::new();
        let sync_root = directory.0.join("state/full-sync");
        local_io::private_dir(&directory.0.join("state")).unwrap();
        local_io::private_dir(&sync_root).unwrap();
        drop(local_io::private_file(&directory.0.join("process.lock")).unwrap());
        let owner = Arc::new(super::super::super::process::lock(&directory.0).unwrap());
        let original_session = FormalFullSyncSession::new(
            config.clone(),
            hex::encode(node_key.to_bytes()),
            &sync_root,
            &local_instance("guarded-recovery"),
        )
        .unwrap();
        let store = LocalCasStore::new(&sync_root);
        let orphan = store
            .put_bytes(b"must survive refused recovery initialization")
            .unwrap();
        let prepared =
            PreparedTransport::prepare_builtin_trust(&[config.endpoint.clone()]).unwrap();
        let policy = Policy {
            api_version: "oasis7.formal-full-sync-transfer-policy/v1".into(),
            attempt_timeout_ms: 10_000,
            max_attempts: if phase == "exhausted" { 1 } else { 2 },
            max_encoded_io_bytes: 1024,
            max_exchanges: 2,
            max_proof_pages: 1,
            max_proof_bytes: 1024,
            max_inflight_requests: 1,
            reopen_timeout_ms: 10_000,
        };
        let mut request = original_session
            .prepare_operation_request(&policy, &prepared)
            .unwrap();
        drop(original_session);
        if phase == "minimum-record" {
            request["initial_minimum_record"] = serde_json::Value::Null;
        }
        if phase == "minimum-cut" {
            request["initial_minimum"]["position"] = serde_json::json!(u64::MAX);
        }
        let identity = Identity {
            operation_id: local_io::random_id().unwrap(),
            request_digest: local_io::digest(&request).unwrap(),
            policy_digest: local_io::digest(&policy).unwrap(),
            binding_digest: "11".repeat(32),
        };
        let path = sync_root.join("operation.json");
        let mut ledger = Ledger::create_with_material_limit(
            &path,
            identity.clone(),
            policy.clone(),
            config.max_total_sync_bytes,
        )
        .unwrap();
        ledger.bind_original_request(&request).unwrap();
        ledger.bind_owner(owner.clone(), &directory.0).unwrap();
        let attempt = ledger.begin_attempt().unwrap();
        if matches!(phase, "selected" | "pending" | "acknowledged" | "legacy") {
            ledger.bind_verified_target(&attempt, b"[{},{}]").unwrap();
        }
        if phase == "pending" || phase == "acknowledged" {
            ledger
                .begin_publication(
                    &attempt,
                    &serde_json::json!({}),
                    &std::collections::BTreeSet::from(["ab".repeat(32)]),
                )
                .unwrap();
        }
        if phase == "acknowledged" {
            ledger
                .complete_publication(
                    &attempt,
                    &serde_json::json!({}),
                    &std::collections::BTreeSet::from(["ab".repeat(32)]),
                )
                .unwrap();
        }
        drop(ledger);
        if phase == "legacy" {
            let mut old: serde_json::Value = local_io::read(&path).unwrap();
            old.as_object_mut()
                .unwrap()
                .remove("publication_journal_version");
            local_io::write(&path, &old).unwrap();
        }
        if phase == "changed-config" {
            config.poll_interval_ms += 1;
        }
        let before = std::fs::read(&path).unwrap();
        let ledger = Ledger::open(&path, &identity, &policy, config.max_total_sync_bytes).unwrap();
        let error = match FormalFullSyncSession::recover_pre_target(
            config,
            hex::encode(node_key.to_bytes()),
            &local_instance("guarded-recovery"),
            ledger,
            owner,
            &directory.0,
            prepared,
        ) {
            Ok(_) => panic!("unsafe recovery constructor accepted {phase}"),
            Err(error) => error,
        };
        assert!(
            error.contains(match phase {
                "changed-config" => "differs from original",
                "minimum-cut" => "minimum",
                _ => "before CAS initialization",
            }),
            "{phase}: {error}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            store.get(&orphan).unwrap(),
            b"must survive refused recovery initialization",
            "{phase}: refused recovery changed CAS"
        );
    }
}
