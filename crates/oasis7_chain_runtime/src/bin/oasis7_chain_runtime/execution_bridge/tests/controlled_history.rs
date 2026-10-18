use super::super::controlled_capture as capture;
use super::super::controlled_history::{
    TrustedHistoryConfiguration, request_identity, verify_history,
};
use super::*;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7_distfs::controlled_authority::replicated_protocol::{
    EndpointRole, FileEndpoint, FixedTrust, ProtocolOutcome, ReplicatedCoordinator,
};
use oasis7_node::NodeExecutionHook;
use oasis7_runtime::{
    Action, ModuleKind, ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription,
    ModuleSubscriptionStage,
};
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn trust() -> FixedTrust {
    FixedTrust {
        world_id: "w1".into(),
        chain_id: "c1".into(),
        genesis_digest: "a".repeat(64),
        authority_epoch: 4,
        writer_key: hex::encode(key(1).verifying_key().to_bytes()),
        primary_id: "p".into(),
        primary_key: hex::encode(key(2).verifying_key().to_bytes()),
        replica_id: "r".into(),
        replica_key: hex::encode(key(3).verifying_key().to_bytes()),
    }
}
fn context(height: u64, nonce: u64) -> NodeExecutionCommitContext {
    let action = Action::ScheduleRecipe {
        requester_agent_id: "agent-0".into(),
        factory_id: "absent-factory".into(),
        recipe_id: "recipe.test".into(),
        plan: oasis7_wasm_abi::RecipeExecutionPlan::accepted(1, vec![], vec![], vec![], 0, 1),
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    let origin = oasis7_runtime::GameplaySubmissionOrigin {
        verified_player_id: "player".into(),
        public_key: "b".repeat(64),
        auth_nonce: nonce,
        hosted_registration_nonce: Some("registration".into()),
        requester_agent_id: "agent-0".into(),
        factory_id: "absent-factory".into(),
        recipe_id: "recipe.test".into(),
    };
    let envelope = ConsensusActionPayloadEnvelope::from_recipe_submission(action, origin);
    let action = oasis7_node::NodeConsensusAction::from_payload(
        nonce,
        "node-transport",
        encode_consensus_action_payload(&envelope).unwrap(),
    )
    .unwrap();
    NodeExecutionCommitContext {
        world_id: "w1".into(),
        node_id: "node-a".into(),
        proposer_id: "node-a".into(),
        height,
        slot: height,
        epoch: 4,
        node_block_hash: format!("node-{height}"),
        action_root: compute_consensus_action_root(std::slice::from_ref(&action)).unwrap(),
        committed_actions: vec![action],
        committed_at_unix_ms: height as i64 * 1000,
    }
}
#[test]
fn controlled_history_rejects_decodable_service_envelopes_without_dropping_them() {
    use oasis7::world_service::*;
    let legacy = context(1, 1);
    let original = capture::supported_origin(&legacy).unwrap();
    assert_eq!(original.submission.auth_nonce, 1);
    let world = oasis7::runtime_host::world::new();
    capture::prepare(&world, &legacy).unwrap();
    let signed = sign_read_request(
        "scheduler",
        SchedulerIntentV1 {
            agent_id: "agent-0".into(),
            request_id: "service-history-boundary".into(),
            delegation_generation: 0,
            captured_base_binding: oasis7_runtime::RuntimeCognitionBaseBindingV1 {
                world_id: "w1".into(),
                branch_id: "main".into(),
                finality_epoch: 0,
                finality_block_hash: None,
                finality_status: "pending".into(),
                base_tick: 0,
                base_world_hash: "0".repeat(64),
                reorg_epoch: 0,
                runtime_manifest_hash: "0".repeat(64),
            },
            operation: SchedulerOperationV1::ReleaseLease {
                lease_id: "original-lease".into(),
            },
        },
        &hex::encode(key(1).to_bytes()),
    )
    .unwrap();
    verify_read_request("scheduler", &signed).unwrap();
    let payload = WorldServicePayloadV1::Scheduler(signed);
    let request = SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: "w1".into(),
                genesis_digest: "a".repeat(64),
            },
            &payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    request.validate().unwrap();
    // Both service encodings accepted by the shared execution decoder stay
    // outside the single legacy origin-bearing ScheduleRecipe history scope.
    for envelope in [
        correlation::encode_consensus_intent(&request).unwrap(),
        encode_consensus_action_payload(&ConsensusActionPayloadEnvelope {
            version: 1,
            auth: None,
            gameplay_submission_origin: None,
            body:
                oasis7::consensus_action_payload::ConsensusActionPayloadBody::WorldServiceIntent {
                    request: request.clone(),
                },
        })
        .unwrap(),
    ] {
        let action =
            oasis7_node::NodeConsensusAction::from_payload(2, "node-transport", envelope).unwrap();
        let mut service = legacy.clone();
        service.committed_actions = vec![action];
        service.action_root = compute_consensus_action_root(&service.committed_actions).unwrap();
        let (actions, simulator, bootstrap, intents) =
            super::super::driver_replicated_input::decode_committed_actions(&service).unwrap();
        assert!(actions.is_empty() && simulator.is_empty() && bootstrap.is_none());
        assert_eq!(intents, vec![(2, request.clone())]);
        assert_eq!(
            capture::supported_origin(&service).unwrap_err(),
            "unsupported capture operation"
        );
        assert_eq!(
            capture::prepare(&world, &service).unwrap_err(),
            "unsupported capture operation"
        );
    }
    assert_eq!(capture::supported_origin(&legacy).unwrap(), original);
}

fn leb(mut n: u64) -> Vec<u8> {
    let mut out = vec![];
    loop {
        let mut b = (n & 127) as u8;
        n >>= 7;
        if n > 0 {
            b |= 128;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
    out
}
fn section(wasm: &mut Vec<u8>, tag: u8, data: Vec<u8>) {
    wasm.push(tag);
    wasm.extend(leb(data.len() as u64));
    wasm.extend(data);
}
fn export(name: &str, kind: u8, index: u8) -> Vec<u8> {
    let mut v = leb(name.len() as u64);
    v.extend(name.as_bytes());
    v.extend([kind, index]);
    v
}
/// Real WASM module with no imports, returning a canonical ModuleOutput.
fn real_tick_wasm() -> Vec<u8> {
    let output = serde_cbor::to_vec(&ModuleOutput {
        new_state: Some(vec![42]),
        effects: vec![],
        emits: vec![],
        tick_lifecycle: None,
        output_bytes: 0,
    })
    .unwrap();
    let mut w = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut w,
        1,
        vec![
            2, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 2, 0x7f, 0x7f, 2, 0x7f, 0x7f,
        ],
    );
    section(&mut w, 3, vec![2, 0, 1]);
    section(&mut w, 5, vec![1, 0, 1]);
    let mut exports = vec![3];
    exports.extend(export("memory", 2, 0));
    exports.extend(export("alloc", 0, 0));
    exports.extend(export("reduce", 0, 1));
    section(&mut w, 7, exports);
    let alloc = vec![0, 0x41, 0x80, 0x08, 0x0b];
    let mut reduce = vec![0, 0x41, 16, 0x41];
    let mut len = leb(output.len() as u64);
    if len.last().unwrap() & 0x40 != 0 {
        *len.last_mut().unwrap() |= 0x80;
        len.push(0);
    }
    reduce.extend(len);
    reduce.push(0x0b);
    let mut code = vec![2];
    code.extend(leb(alloc.len() as u64));
    code.extend(alloc);
    code.extend(leb(reduce.len() as u64));
    code.extend(reduce);
    section(&mut w, 10, code);
    let mut data = vec![1, 0, 0x41, 16, 0x0b];
    data.extend(leb(output.len() as u64));
    data.extend(output);
    section(&mut w, 11, data);
    w
}
fn module_world() -> RuntimeWorld {
    let bytes = real_tick_wasm();
    let hash = sha256_hex(&bytes);
    let manifest = ModuleManifest {
        module_id: "m.history.tick".into(),
        name: "Real Tick".into(),
        version: "0.1.0".into(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: hash.clone(),
        interface_version: "wasm-1".into(),
        abi_contract: oasis7_wasm_abi::ModuleAbiContract::default(),
        exports: vec!["reduce".into()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec![],
            action_kinds: vec![],
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        }],
        required_caps: vec![],
        artifact_identity: Some(signed_test_artifact_identity(&hash)),
        limits: ModuleLimits {
            max_mem_bytes: 1024 * 1024,
            max_gas: 1_000_000,
            max_call_rate: 128,
            max_output_bytes: 1024 * 1024,
            max_effects: 16,
            max_emits: 16,
        },
    };
    let mut world = oasis7::runtime_host::world::new();
    world
        .bind_node_identity(
            TEST_MODULE_ARTIFACT_SIGNER_NODE_ID,
            &hex::encode(
                test_module_artifact_signing_key()
                    .verifying_key()
                    .to_bytes(),
            ),
        )
        .unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-0".into(),
        pos: oasis7::geometry::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world
        .set_agent_resource_balance(
            "agent-0",
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            256,
        )
        .unwrap();
    world
        .set_agent_resource_balance(
            "agent-0",
            oasis7_agent_api::world_action::ResourceKind::Data,
            256,
        )
        .unwrap();
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-0".into(),
        wasm_hash: hash,
        wasm_bytes: bytes,
    });
    world.step().unwrap();
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "agent-0".into(),
        manifest,
        activate: true,
    });
    world.step().unwrap();
    world
}
#[test]
#[cfg(feature = "wasmtime")]
fn controlled_history_real_wasm_two_records_reexecute_and_fail_closed() {
    let dir = temp_dir("controlled-history-real-wasm");
    let original_world = module_world();
    let cached_snapshot = original_world.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: "w1",
            chain_id: "c1",
            genesis_ref: None,
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: original_world.state().time,
        },
        "original-bootstrap-config",
        "original-bootstrap-generator",
    );
    let world = oasis7::runtime_host::world::from_snapshot(
        cached_snapshot,
        original_world.journal().clone(),
    )
    .unwrap();
    let initial_height = world.state().time;
    let initial_root = execution_world_snapshot_root(&world).unwrap();
    let store = LocalCasStore::new(dir.join("store"));
    let initial_snapshot = store
        .put_bytes(&to_cbor(world.snapshot()).unwrap())
        .unwrap();
    let initial_journal = store.put_bytes(&to_cbor(world.journal()).unwrap()).unwrap();
    let initial_block =
        blake3_hex(&to_cbor((&initial_snapshot, &initial_journal, initial_height)).unwrap());
    let initial = ExecutionBridgeRecord::new_v3(
        "w1".into(),
        initial_height,
        None,
        None,
        "node-a".into(),
        compute_consensus_action_root(&[]).unwrap(),
        initial_block.clone(),
        initial_root.clone(),
        world.journal().len(),
        initial_snapshot,
        initial_journal,
        None,
        None,
        0,
    );
    let records = dir.join("records");
    fs::create_dir_all(&records).unwrap();
    persist_execution_bridge_record_only(&records, &initial).unwrap();
    let state = ExecutionBridgeState {
        last_applied_committed_height: initial_height,
        last_execution_block_hash: Some(initial_block.clone()),
        last_execution_state_root: Some(initial_root.clone()),
        last_node_block_hash: None,
    };
    let policy = world.release_security_policy().clone();
    let mut driver = NodeRuntimeExecutionDriver::new_with_sandbox(
        dir.join("state"),
        dir.join("world"),
        records.clone(),
        dir.join("store"),
        state,
        world,
        Box::new(WasmExecutor::new(WasmExecutorConfig::default()).unwrap()),
        32,
        32,
        4,
    );
    driver.set_controlled_capture_enabled(true).unwrap();
    let t = trust();
    let genesis = t.genesis_anchor().unwrap();
    let p = FileEndpoint::open(
        dir.join("p"),
        t.clone(),
        EndpointRole::Primary,
        key(2),
        &genesis,
    )
    .unwrap();
    let r = FileEndpoint::open(
        dir.join("r"),
        t.clone(),
        EndpointRole::Replica,
        key(3),
        &genesis,
    )
    .unwrap();
    let mut coordinator = ReplicatedCoordinator::new(p, r, key(1)).unwrap();
    let mut history = vec![];
    for n in 1..=2 {
        let context = context(initial_height + n, n);
        driver.on_commit(context.clone()).unwrap();
        let record =
            load_execution_bridge_record(&execution_bridge_record_path(&records, context.height))
                .unwrap();
        let package = capture::load_package(
            &driver.execution_store,
            record
                .controlled_capture_ref
                .as_ref()
                .expect("actual producer capture"),
        )
        .unwrap();
        let origin = capture::supported_origin(&context).unwrap();
        let parent = coordinator.heads().0;
        let ProtocolOutcome::DurabilityQualified(e) = coordinator
            .submit(
                4,
                &parent,
                request_identity(&origin.submission).unwrap(),
                package,
            )
            .unwrap()
        else {
            panic!("qualified");
        };
        history.push(*e);
    }
    let config = TrustedHistoryConfiguration {
        trust: t,
        minimum_head: coordinator.heads().0,
        release_security_policy: policy,
        initial_execution_height: initial_height,
        initial_execution_state_root: initial_root,
        initial_execution_block_hash: initial_block,
        published_initial_anchor: None,
    };
    let verified = verify_history(&config, &history).unwrap();
    assert_eq!(verified.records_reexecuted, 2);
    // Operator-pinned published prior bytes, with a resource context differing
    // from the cached pre-step snapshot, retain exact typed world continuity.
    let first_manifest: capture::CaptureManifest = capture::decode_role(
        &history[0].proposal.body.record,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest,
    )
    .unwrap();
    let first_preparation =
        capture::materialize_preparation(&history[0].proposal.body.record, &first_manifest)
            .unwrap();
    let pre_snapshot = capture::decode_snapshot(&first_preparation.before_snapshot).unwrap();
    let pre_journal: oasis7_runtime::Journal =
        capture::decode_generic(&first_preparation.before_journal).unwrap();
    let pre_world =
        oasis7::runtime_host::world::from_snapshot(pre_snapshot, pre_journal.clone()).unwrap();
    let resource_commit =
        super::super::execution_hash::execution_resource_commit_hash("w1", initial_height);
    let resource_hash = super::super::execution_hash::execution_resource_context_hash("w1");
    let published_snapshot = to_cbor(pre_world.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: "w1",
            chain_id: "w1",
            genesis_ref: None,
            created_at_height: super::super::execution_hash::execution_resource_created_at_height(
                initial_height,
            ),
            manifest_height: initial_height,
            commit_block_hash: Some(&resource_commit),
            tick: initial_height,
        },
        resource_hash.clone(),
        resource_hash,
    ))
    .unwrap();
    let mut published_record = first_manifest.record.clone();
    published_record.height = initial_height;
    published_record.execution_block_hash = config.initial_execution_block_hash.clone();
    published_record.execution_state_root = blake3_hex(&published_snapshot);
    published_record.snapshot_ref = Some(blake3_hex(&published_snapshot));
    published_record.latest_state_ref = published_record.snapshot_ref.clone();
    published_record.journal_ref = Some(blake3_hex(&first_preparation.before_journal));
    published_record.journal_len = pre_journal.len();
    let record_bytes = serde_json::to_vec(&published_record).unwrap();
    let record_path = dir.join("external-prior-record.json");
    let snapshot_path = dir.join("external-prior-snapshot.cbor");
    let journal_path = dir.join("external-prior-journal.cbor");
    fs::write(&record_path, &record_bytes).unwrap();
    fs::write(&snapshot_path, &published_snapshot).unwrap();
    fs::write(&journal_path, &first_preparation.before_journal).unwrap();
    let mut published_config = config.clone();
    published_config.initial_execution_state_root = blake3_hex(&published_snapshot);
    published_config.published_initial_anchor =
        Some(super::super::controlled_history::PublishedInitialAnchor {
            record_path: fs::canonicalize(record_path).unwrap(),
            record_hash: blake3_hex(&record_bytes),
            snapshot_path: fs::canonicalize(snapshot_path).unwrap(),
            snapshot_hash: blake3_hex(&published_snapshot),
            journal_path: fs::canonicalize(journal_path).unwrap(),
            journal_hash: blake3_hex(&first_preparation.before_journal),
            cached_resource_snapshot: None,
        });
    assert!(
        verify_history(&published_config, &history)
            .unwrap_err()
            .contains("unrecognized resource annotation")
    );
    let cached_path = dir.join("external-original-cached-snapshot.cbor");
    fs::write(&cached_path, &first_preparation.before_snapshot).unwrap();
    published_config
        .published_initial_anchor
        .as_mut()
        .unwrap()
        .cached_resource_snapshot =
        Some(super::super::controlled_history::PinnedResourceSnapshot {
            snapshot_path: fs::canonicalize(&cached_path).unwrap(),
            snapshot_hash: blake3_hex(&first_preparation.before_snapshot),
        });
    verify_history(&published_config, &history).unwrap();
    let mut relative_pin = published_config.clone();
    relative_pin
        .published_initial_anchor
        .as_mut()
        .unwrap()
        .cached_resource_snapshot
        .as_mut()
        .unwrap()
        .snapshot_path = "relative-snapshot.cbor".into();
    assert!(
        verify_history(&relative_pin, &history)
            .unwrap_err()
            .contains("must be absolute")
    );
    let cached_original = fs::read(&cached_path).unwrap();
    // A trusted complete pair cannot be switched, split, or supplied with an
    // altered pin. Current state/Journal still come from the published anchor.
    let mut bad_pin = published_config.clone();
    bad_pin
        .published_initial_anchor
        .as_mut()
        .unwrap()
        .cached_resource_snapshot
        .as_mut()
        .unwrap()
        .snapshot_hash = "0".repeat(64);
    assert!(verify_history(&bad_pin, &history).is_err());
    for mutation in 0..6 {
        let mut switched = capture::decode_snapshot(&cached_original).unwrap();
        match mutation {
            0 => {
                switched.chain_resource_manifest = capture::decode_snapshot(&published_snapshot)
                    .unwrap()
                    .chain_resource_manifest
            }
            1 => {
                switched.latest_chain_resource_delta = capture::decode_snapshot(&published_snapshot)
                    .unwrap()
                    .latest_chain_resource_delta
            }
            2 => switched.latest_chain_resource_delta.as_mut().unwrap().tick += 1,
            3 => switched.chain_resource_manifest.world_id = "foreign-world".into(),
            4 => switched.chain_resource_manifest.manifest_height = u64::MAX,
            _ => switched.state.time = u64::MAX,
        }
        let changed = to_cbor(switched).unwrap();
        fs::write(&cached_path, &changed).unwrap();
        let mut altered = published_config.clone();
        altered
            .published_initial_anchor
            .as_mut()
            .unwrap()
            .cached_resource_snapshot
            .as_mut()
            .unwrap()
            .snapshot_hash = blake3_hex(&changed);
        assert!(verify_history(&altered, &history).is_err());
    }
    fs::write(&cached_path, &cached_original).unwrap();
    verify_history(&published_config, &history).unwrap();
    let mut wrong_published = published_config.clone();
    wrong_published
        .published_initial_anchor
        .as_mut()
        .unwrap()
        .journal_hash = "0".repeat(64);
    assert!(verify_history(&wrong_published, &history).is_err());
    wrong_published = published_config.clone();
    wrong_published.initial_execution_state_root = "0".repeat(64);
    assert!(verify_history(&wrong_published, &history).is_err());

    let anchor = published_config.published_initial_anchor.as_ref().unwrap();
    let original_snapshot_bytes = fs::read(&anchor.snapshot_path).unwrap();
    // Correctly pinned malformed/huge-hint bytes must still fail the existing
    // snapshot schema budget; the pin does not waive typed decoding.
    for malformed in [
        vec![0xff],
        vec![0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    ] {
        fs::write(&anchor.snapshot_path, &malformed).unwrap();
        let mut invalid = published_config.clone();
        invalid
            .published_initial_anchor
            .as_mut()
            .unwrap()
            .snapshot_hash = blake3_hex(&malformed);
        assert!(verify_history(&invalid, &history).is_err());
    }
    let oversized = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&anchor.snapshot_path)
        .unwrap();
    oversized.set_len(64 * 1024 * 1024 + 1).unwrap();
    assert!(verify_history(&published_config, &history).is_err());
    drop(oversized);
    fs::write(&anchor.snapshot_path, &original_snapshot_bytes).unwrap();
    verify_history(&published_config, &history).unwrap();
    // Reject arbitrary diagnostic contexts and mixing halves of valid pairs.
    let previous = &history[0].proposal.body.record;
    let previous_manifest: capture::CaptureManifest = capture::decode_role(
        previous,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest,
    )
    .unwrap();
    let next_package = &history[1].proposal.body.record;
    let next_manifest: capture::CaptureManifest = capture::decode_role(
        next_package,
        oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest,
    )
    .unwrap();
    let next = capture::materialize_preparation(next_package, &next_manifest).unwrap();
    capture::validate_continuity(previous, &previous_manifest, &next).unwrap();
    for field in 0..5 {
        let mut changed = next.clone();
        let mut snapshot = capture::decode_snapshot(&changed.before_snapshot).unwrap();
        match field {
            0 => snapshot
                .chain_resource_manifest
                .world_id
                .push_str("-forged"),
            1 => snapshot
                .chain_resource_manifest
                .chain_id
                .push_str("-forged"),
            2 => snapshot.chain_resource_manifest.manifest_height += 1,
            3 => snapshot.chain_resource_manifest.manifest_hash.push('0'),
            _ => snapshot.latest_chain_resource_delta.as_mut().unwrap().tick += 1,
        }
        changed.before_snapshot = to_cbor(snapshot).unwrap();
        assert!(capture::validate_continuity(previous, &previous_manifest, &changed).is_err());
    }
    let previous_post = capture::decode_snapshot(
        capture::object(
            previous,
            previous_manifest.record.snapshot_ref.as_deref().unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut mixed = next.clone();
    let mut mixed_snapshot = capture::decode_snapshot(&mixed.before_snapshot).unwrap();
    assert_ne!(
        mixed_snapshot.chain_resource_manifest,
        previous_post.chain_resource_manifest
    );
    mixed_snapshot.chain_resource_manifest = previous_post.chain_resource_manifest;
    mixed.before_snapshot = to_cbor(mixed_snapshot).unwrap();
    assert!(capture::validate_continuity(previous, &previous_manifest, &mixed).is_err());

    assert!(
        driver
            .execution_world
            .snapshot()
            .module_registry
            .records
            .values()
            .any(|m| m.manifest.module_id == "m.history.tick")
    );
    let mut wrong = config.clone();
    wrong.trust.world_id = "wrong".into();
    assert!(verify_history(&wrong, &history).is_err());
    let mut truncated = history.clone();
    truncated.pop();
    assert!(verify_history(&config, &truncated).is_err());
    let mut forged = history.clone();
    forged[0].replica_receipt = forged[0].replica_prepare.clone();
    assert!(verify_history(&config, &forged).is_err());
    let mut tampered = history.clone();
    tampered[0].proposal.body.record.objects[0].bytes.push(0);
    assert!(verify_history(&config, &tampered).is_err());
    // Dual durable receipts alone do not establish execution correctness.
    // Stored tick-root validation rejects an inconsistent reducer state.
    // A separately re-signed wrong execution block root still needs real replay.
    let first_preparation = capture::materialize_preparation(previous, &previous_manifest).unwrap();
    let mut wrong_post = capture::decode_snapshot(
        capture::object(
            previous,
            previous_manifest.record.snapshot_ref.as_deref().unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let reducer_state = wrong_post.state.module_states.values_mut().next().unwrap();
    assert_eq!(reducer_state, &vec![42]);
    *reducer_state = vec![43];
    let wrong_post_bytes = to_cbor(wrong_post).unwrap();
    let mut wrong_record = previous_manifest.record.clone();
    wrong_record.execution_state_root = blake3_hex(&wrong_post_bytes);
    wrong_record.snapshot_ref = Some(wrong_record.execution_state_root.clone());
    wrong_record.latest_state_ref = wrong_record.snapshot_ref.clone();
    let first_journal = capture::object(
        previous,
        previous_manifest.record.journal_ref.as_deref().unwrap(),
    )
    .unwrap();
    let first_effect =
        capture::decode_generic(capture::object(previous, &previous_manifest.effect_ref).unwrap())
            .unwrap();
    assert!(
        capture::build(
            &first_preparation,
            &wrong_record,
            &wrong_post_bytes,
            first_journal,
            &first_effect,
            None,
        )
        .is_err()
    );
    let original_post_bytes = capture::object(
        previous,
        previous_manifest.record.snapshot_ref.as_deref().unwrap(),
    )
    .unwrap();
    let mut wrong_record = previous_manifest.record.clone();
    wrong_record.execution_block_hash = "f".repeat(64);
    let wrong_package = capture::build(
        &first_preparation,
        &wrong_record,
        original_post_bytes,
        first_journal,
        &first_effect,
        None,
    )
    .unwrap();
    let p = FileEndpoint::open(
        dir.join("wrong-p"),
        config.trust.clone(),
        EndpointRole::Primary,
        key(2),
        &genesis,
    )
    .unwrap();
    let r = FileEndpoint::open(
        dir.join("wrong-r"),
        config.trust.clone(),
        EndpointRole::Replica,
        key(3),
        &genesis,
    )
    .unwrap();
    let mut wrong_coordinator = ReplicatedCoordinator::new(p, r, key(1)).unwrap();
    let ProtocolOutcome::DurabilityQualified(wrong_evidence) = wrong_coordinator
        .submit(
            4,
            &genesis,
            history[0].proposal.body.request.clone(),
            wrong_package,
        )
        .unwrap()
    else {
        panic!("wrong outcome is still durably recorded");
    };
    oasis7_distfs::controlled_authority::replicated_protocol::verify_evidence(
        &wrong_evidence,
        &config.trust,
        &genesis,
    )
    .unwrap();
    let mut wrong_config = config.clone();
    wrong_config.minimum_head = wrong_coordinator.heads().0;
    let error = verify_history(&wrong_config, &[*wrong_evidence]).unwrap_err();
    assert!(error.contains("reexecution"), "{error}");
    drop(wrong_coordinator);
    // Exercise the public offline command without changing its input files.
    let trusted_path = dir.join("trusted.json");
    let evidence_path = dir.join("evidence.json");
    let trusted_bytes = serde_json::to_vec(&config).unwrap();
    let evidence_bytes = serde_json::to_vec(&history).unwrap();
    fs::write(&trusted_path, &trusted_bytes).unwrap();
    fs::write(&evidence_path, &evidence_bytes).unwrap();
    let canonical_trust = fs::canonicalize(&trusted_path).unwrap();
    let canonical_evidence = fs::canonicalize(&evidence_path).unwrap();
    crate::controlled_history_cli::run(
        [
            "--trusted-config",
            canonical_trust.to_str().unwrap(),
            "--evidence",
            canonical_evidence.to_str().unwrap(),
        ]
        .into_iter(),
    )
    .unwrap();
    assert_eq!(fs::read(&trusted_path).unwrap(), trusted_bytes);
    assert_eq!(fs::read(&evidence_path).unwrap(), evidence_bytes);
    // Missing predecessor material must roll back the actual world after step.
    let prior_record =
        load_execution_bridge_record(&execution_bridge_record_path(&records, initial_height + 2))
            .unwrap();
    let reference = prior_record.controlled_capture_ref.unwrap();
    let prior_bytes = driver.execution_store.get(&reference).unwrap();
    fs::remove_file(
        driver
            .execution_store
            .blobs_dir()
            .join(format!("{reference}.blob")),
    )
    .unwrap();
    let before_world = to_cbor(driver.execution_world.snapshot()).unwrap();
    let before_journal = to_cbor(driver.execution_world.journal()).unwrap();
    let before_state = driver.state.clone();
    let third = context(initial_height + 3, 3);
    assert!(driver.on_commit(third.clone()).is_err());
    assert_eq!(
        to_cbor(driver.execution_world.snapshot()).unwrap(),
        before_world
    );
    assert_eq!(
        to_cbor(driver.execution_world.journal()).unwrap(),
        before_journal
    );
    assert_eq!(driver.state, before_state);
    assert_eq!(
        driver.execution_store.put_bytes(&prior_bytes).unwrap(),
        reference
    );
    driver.on_commit(third).unwrap();
    drop((driver, coordinator));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(feature = "wasmtime")]
fn controlled_history_first_intent_write_retains_original_preparation_on_reopen() {
    let dir = temp_dir("controlled-history-marker-first");
    let world = oasis7::runtime_host::world::new();
    let before = to_cbor(world.snapshot()).unwrap();
    let ctx = context(1, 1);
    let preparation = capture::prepare(&world, &ctx).unwrap();
    let store = LocalCasStore::new(dir.join("store"));
    let reference = capture::persist_preparation(&store, &preparation).unwrap();
    persist_execution_world(&dir.join("world"), &world).unwrap();
    let effect =
        build_execution_external_effect_materialization_with_pre_step_root(&world, &ctx, None)
            .unwrap();
    let mut staged = world.clone();
    staged.submit_action(Action::RegisterAgent {
        agent_id: "staged-test-agent".into(),
        pos: oasis7::geometry::GeoPos::new(0, 0, 0),
    });
    staged.step().unwrap();
    super::super::product_validation_intent::persist_product_validation_intent_for_staged_world(
        &dir.join("records"),
        &staged,
        &ctx,
        &blake3_hex(&before),
        effect,
        Some(reference.clone()),
    )
    .unwrap();
    // The first marker publication already includes the original pre-state.
    // Simulate process death before any staged world publication.
    let marker = load_product_validation_intent(&dir.join("records"))
        .unwrap()
        .unwrap();
    assert_eq!(
        marker.controlled_preparation_ref.as_deref(),
        Some(reference.as_str())
    );
    assert_eq!(
        capture::load_preparation(&store, &reference, &ctx).unwrap(),
        preparation
    );
    let mut reopened = NodeRuntimeExecutionDriver::new(
        dir.join("state"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
    )
    .unwrap();
    reopened.set_controlled_capture_enabled(true).unwrap();
    assert!(reopened.pending_product_validation_intent.is_none());
    assert_eq!(
        to_cbor(reopened.execution_world.snapshot()).unwrap(),
        before
    );
    assert!(capture::load_preparation(&store, &reference, &context(1, 2)).is_err());
    drop(reopened);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(unix)]
fn controlled_history_capture_durability_rejects_existing_corruption_and_symlink() {
    let dir = temp_dir("controlled-history-corrupt-cas");
    let store = LocalCasStore::new(dir.join("store"));
    let reference = capture::persist_capture_bytes(&store, b"capture material").unwrap();
    let path = store.blobs_dir().join(format!("{reference}.blob"));
    fs::write(&path, b"corrupt material").unwrap();
    assert!(capture::persist_capture_bytes(&store, b"capture material").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"corrupt material");
    fs::remove_file(&path).unwrap();
    let target = dir.join("target");
    fs::write(&target, b"capture material").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(capture::persist_capture_bytes(&store, b"capture material").is_err());
    assert_eq!(fs::read(&target).unwrap(), b"capture material");
    fs::remove_dir_all(dir).unwrap();
}

/// Explicit manual probe for a fresh, opt-in producer capture. Never runs as a
/// default fixture and never modifies the supplied world, records, or CAS.
#[test]
#[ignore = "requires actual captured records and external diagnostic trust/keys"]
fn controlled_history_actual_operator_probe() {
    use super::super::controlled_history::validate_capture_prefix;
    use serde::Deserialize;
    use std::path::{Path, PathBuf};
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        diagnostic_only: bool,
        trusted_config: PathBuf,
        signing_keys: PathBuf,
        records: Vec<PathBuf>,
        store: PathBuf,
        output: PathBuf,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Keys {
        writer_seed: String,
        primary_seed: String,
        replica_seed: String,
    }
    fn read(path: &Path, maximum: u64) -> Vec<u8> {
        crate::controlled_history_cli::read_bounded(path, maximum).unwrap()
    }
    fn signing_key(seed: &str) -> SigningKey {
        let bytes: [u8; 32] = hex::decode(seed)
            .expect("diagnostic key encoding")
            .try_into()
            .expect("diagnostic key width");
        SigningKey::from_bytes(&bytes)
    }
    let path = PathBuf::from(
        std::env::var("OASIS7_CONTROLLED_HISTORY_PROBE")
            .expect("explicit operator probe configuration required"),
    );
    let probe: Probe = serde_json::from_slice(&read(&path, 64 * 1024)).unwrap();
    assert!(probe.diagnostic_only, "this probe cannot activate a world");
    assert!(!probe.records.is_empty() && probe.records.len() <= 64);
    let mut config: TrustedHistoryConfiguration =
        serde_json::from_slice(&read(&probe.trusted_config, 64 * 1024)).unwrap();
    let genesis = config.trust.genesis_anchor().unwrap();
    assert_eq!(
        config.minimum_head, genesis,
        "new isolated diagnostic endpoints only"
    );
    let keys: Keys = serde_json::from_slice(&read(&probe.signing_keys, 4096)).unwrap();
    let writer = signing_key(&keys.writer_seed);
    let primary = signing_key(&keys.primary_seed);
    let replica = signing_key(&keys.replica_seed);
    assert_eq!(
        hex::encode(writer.verifying_key().to_bytes()),
        config.trust.writer_key
    );
    assert_eq!(
        hex::encode(primary.verifying_key().to_bytes()),
        config.trust.primary_key
    );
    assert_eq!(
        hex::encode(replica.verifying_key().to_bytes()),
        config.trust.replica_key
    );
    let store = LocalCasStore::new(probe.store);
    let packages: Vec<_> = probe
        .records
        .iter()
        .map(|path| {
            let bytes = read(path, 64 * 1024);
            let record: ExecutionBridgeRecord = serde_json::from_slice(&bytes).unwrap();
            capture::load_package(
                &store,
                record
                    .controlled_capture_ref
                    .as_ref()
                    .expect("actual opt-in producer capture required"),
            )
            .unwrap()
        })
        .collect();
    // No receipt is signed until every original module, typed closure, prefix,
    // and initial external execution anchor passes actual in-memory reexecution.
    validate_capture_prefix(&config, &packages.iter().collect::<Vec<_>>()).unwrap();
    fs::create_dir(&probe.output).expect("output must be a new exclusive directory");
    let p = FileEndpoint::open(
        probe.output.join("primary"),
        config.trust.clone(),
        EndpointRole::Primary,
        primary,
        &genesis,
    )
    .unwrap();
    let r = FileEndpoint::open(
        probe.output.join("replica"),
        config.trust.clone(),
        EndpointRole::Replica,
        replica,
        &genesis,
    )
    .unwrap();
    let mut coordinator = ReplicatedCoordinator::new(p, r, writer).unwrap();
    let mut evidence = Vec::new();
    for package in packages {
        let manifest: capture::CaptureManifest = capture::decode_role(&package,
            oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest).unwrap();
        let origin = capture::supported_origin(&manifest.context).unwrap();
        let parent = coordinator.heads().0;
        let ProtocolOutcome::DurabilityQualified(receipt) = coordinator
            .submit(
                config.trust.authority_epoch,
                &parent,
                request_identity(&origin.submission).unwrap(),
                package,
            )
            .unwrap()
        else {
            panic!("diagnostic dual endpoint qualification failed")
        };
        evidence.push(*receipt);
    }
    config.minimum_head = coordinator.heads().0;
    verify_history(&config, &evidence).unwrap();
    let trust_path = probe.output.join("trusted-config.json");
    let evidence_path = probe.output.join("evidence.json");
    super::super::durable_transaction::write_file_durable(
        &trust_path,
        &serde_json::to_vec_pretty(&config).unwrap(),
    )
    .unwrap();
    super::super::durable_transaction::write_file_durable(
        &evidence_path,
        &serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    crate::controlled_history_cli::run(
        [
            "--trusted-config",
            trust_path.to_str().unwrap(),
            "--evidence",
            evidence_path.to_str().unwrap(),
        ]
        .into_iter(),
    )
    .unwrap();
}
