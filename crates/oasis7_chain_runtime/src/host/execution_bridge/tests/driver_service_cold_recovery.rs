//! Record-before-cache crash recovery across a genuine service commit boundary.
use super::*;
use oasis7_node::NodeExecutionHook;
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::ReleaseSecurityPolicy;

struct Fixture {
    root: std::path::PathBuf,
    baseline: oasis7_node::NodeExecutionBootstrap,
    prefix_state: Vec<u8>,
    latest_record: Vec<u8>,
    canonical_tick: u64,
    prefix_cache_root: String,
}
impl Fixture {
    fn new(admin_last: bool) -> Self {
        let root = temp_dir("service-record-before-cache");
        let world_dir = root.join("world");
        let records = root.join("records");
        let mut world = oasis7::runtime_host::world::new();
        world.step().unwrap();
        world
            .bind_cognition_runtime("cold-world", "main", 0, None, "pending", 0)
            .unwrap();
        // Use the ordinary service fixture's explicit chain-resource identity
        // before persistence; the cognition binding must agree with it.
        let context_hash =
            super::super::execution_hash::execution_resource_context_hash("cold-world");
        world
            .save_to_dir_with_chain_resource_context(
                &world_dir,
                oasis7_runtime::ChainResourceDerivationContext {
                    world_id: "cold-world",
                    chain_id: "cold-world",
                    genesis_ref: Some("cold-genesis"),
                    created_at_height: world.state().time,
                    manifest_height: world.state().time,
                    commit_block_hash: None,
                    tick: world.state().time,
                },
                context_hash.clone(),
                context_hash,
            )
            .unwrap();
        fs::write(
            world_dir.join("world-service-identity.json"),
            serde_json::to_vec(&oasis7::world_service::WorldIdentity {
                world_id: "cold-world".into(),
                genesis_digest: "cold-genesis".into(),
            })
            .unwrap(),
        )
        .unwrap();
        let setup = super::super::local_bootstrap::derive_local_execution_bootstrap(
            &world_dir,
            &records,
            "cold-world",
            1,
            "cold-setup-h1",
            &ReleaseSecurityPolicy::default(),
        )
        .unwrap();
        let mut driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
            root.join("state.json"),
            world_dir.clone(),
            records.clone(),
            root.join("store"),
            &StorageProfileConfig::default(),
            setup,
        )
        .unwrap();
        let mut prefix = None;
        for height in 2..=132 {
            let mut actions = Vec::new();
            if admin_last && height == 132 {
                use oasis7::world_service::*;
                let payload = WorldServicePayloadV1::Scheduler(
                    sign_read_request(
                        "scheduler",
                        SchedulerIntentV1 {
                            task_prefix_binding_revision: 0,
                            agent_id: "absent-agent".into(),
                            request_id: "cold-rejected-settle".into(),
                            delegation_generation: 0,
                            captured_base_binding: driver
                                .execution_world
                                .current_cognition_base_binding()
                                .unwrap(),
                            operation: SchedulerOperationV1::SettleLease {
                                lease_id: "absent-lease".into(),
                                consumed_amount: 1,
                            },
                        },
                        &hex::encode([43; 32]),
                    )
                    .unwrap(),
                );
                let identity = WorldIdentity {
                    world_id: "cold-world".into(),
                    genesis_digest: "cold-genesis".into(),
                };
                let request = SubmitIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    correlation: derive_correlation(identity, &payload).unwrap(),
                    deadline_unix_ms: None,
                    signed_payload: payload,
                };
                actions.push(
                    oasis7_node::NodeConsensusAction::from_payload(
                        132,
                        "cold-node",
                        correlation::encode_consensus_intent(&request).unwrap(),
                    )
                    .unwrap(),
                );
            }
            driver
                .on_commit(NodeExecutionCommitContext {
                    world_id: "cold-world".into(),
                    node_id: "cold-node".into(),
                    proposer_id: "cold-node".into(),
                    height,
                    slot: height,
                    epoch: 0,
                    node_block_hash: format!("cold-node-h{height}"),
                    action_root: compute_consensus_action_root(&actions).unwrap(),
                    committed_actions: actions,
                    committed_at_unix_ms: height as i64 * 1000,
                })
                .unwrap();
            if height == 131 {
                prefix = Some((
                    driver.execution_world.clone(),
                    fs::read(root.join("state.json")).unwrap(),
                ));
            }
        }
        let canonical_tick = driver.execution_world.state().time;
        assert_eq!(canonical_tick, if admin_last { 131 } else { 132 });
        drop(driver);
        let baseline = super::super::local_bootstrap::derive_service_execution_bootstrap(
            &world_dir,
            &records,
            &root.join("store"),
            "cold-world",
        )
        .unwrap();
        assert_eq!(baseline.height, 132);
        let latest_record = fs::read(records.join(format!("{:020}.json", 132))).unwrap();
        let (prefix_world, prefix_state) = prefix.unwrap();
        let state: ExecutionBridgeState = serde_json::from_slice(&prefix_state).unwrap();
        let record =
            load_execution_bridge_record(&execution_bridge_record_path(&records, 131)).unwrap();
        assert_eq!(state.last_applied_committed_height, 131);
        assert_eq!(
            state.last_execution_block_hash.as_ref(),
            Some(&record.execution_block_hash)
        );
        assert_eq!(
            state.last_execution_state_root.as_ref(),
            Some(&record.execution_state_root)
        );
        assert_eq!(state.last_node_block_hash, record.node_block_hash);
        // Re-publish the actual prior cache generation/state: simulate a crash
        // after successor record/CAS durability, before those mutable caches.
        persist_execution_world(&world_dir, &prefix_world).unwrap();
        fs::write(root.join("state.json"), &prefix_state).unwrap();
        assert_eq!(
            super::super::driver::load_execution_world_with_policy(
                &world_dir,
                ReleaseSecurityPolicy::default()
            )
            .unwrap()
            .state()
            .time,
            131
        );
        let prefix_cache = super::super::driver::load_execution_world_with_policy(
            &world_dir,
            ReleaseSecurityPolicy::default(),
        )
        .unwrap();
        let prefix_cache_root = execution_world_snapshot_root(&prefix_cache).unwrap();
        Self {
            root,
            baseline,
            prefix_state,
            latest_record,
            canonical_tick,
            prefix_cache_root,
        }
    }
    fn restart(&self) -> Result<NodeRuntimeExecutionDriver, String> {
        NodeRuntimeExecutionDriver::new_with_local_bootstrap(
            self.root.join("state.json"),
            self.root.join("world"),
            self.root.join("records"),
            self.root.join("store"),
            &StorageProfileConfig::default(),
            self.baseline.clone(),
        )
    }
    fn state_unchanged(&self, expected: &[u8]) {
        assert!(fs::read(self.root.join("state.json")).unwrap() == expected);
        let cache = super::super::driver::load_execution_world_with_policy(
            &self.root.join("world"),
            ReleaseSecurityPolicy::default(),
        )
        .unwrap();
        assert_eq!(cache.state().time, 131);
        assert_eq!(
            execution_world_snapshot_root(&cache).unwrap(),
            self.prefix_cache_root
        );
        assert!(
            fs::read(self.root.join("records").join(format!("{:020}.json", 132))).unwrap()
                == self.latest_record
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn service_cold_bootstrap_recovers_durable_132_before_checking_prefix_131_cache() {
    for admin_last in [false, true] {
        let fixture = Fixture::new(admin_last);
        if admin_last {
            // A lagging latest pointer is only a cache, not permission to
            // ignore the higher complete original numbered record.
            let old_pointer = fs::read(
                fixture
                    .root
                    .join("records")
                    .join(format!("{:020}.json", 131)),
            )
            .unwrap();
            fs::write(fixture.root.join("records/latest.json"), old_pointer).unwrap();
        }
        let restored = fixture
            .restart()
            .expect("trusted complete successor CAS must recover before checking stale cache time");
        assert_eq!(restored.state.last_applied_committed_height, 132);
        assert_eq!(
            restored.state.last_execution_block_hash.as_deref(),
            Some(fixture.baseline.execution_block_hash.as_str())
        );
        assert_eq!(
            restored.state.last_execution_state_root.as_deref(),
            Some(fixture.baseline.execution_state_root.as_str())
        );
        assert_eq!(
            restored.execution_world.state().time,
            fixture.canonical_tick
        );
        assert!(
            fs::read(
                fixture
                    .root
                    .join("records")
                    .join(format!("{:020}.json", 132))
            )
            .unwrap()
                == fixture.latest_record
        );
        restored
            .execution_world
            .verify_tick_consensus_chain()
            .unwrap();
    }
}

#[test]
fn service_cold_bootstrap_does_not_overwrite_foreign_future_or_missing_material() {
    let fixture = Fixture::new(true);
    let state_path = fixture.root.join("state.json");
    for future in [false, true] {
        let mut state: serde_json::Value = serde_json::from_slice(&fixture.prefix_state).unwrap();
        if future {
            state["last_applied_committed_height"] = serde_json::json!(133);
        } else {
            state["last_execution_block_hash"] = serde_json::json!("foreign-prefix");
        }
        let invalid = serde_json::to_vec(&state).unwrap();
        fs::write(&state_path, &invalid).unwrap();
        assert!(fixture.restart().is_err());
        fixture.state_unchanged(&invalid);
        fs::write(&state_path, &fixture.prefix_state).unwrap();
    }
    let records = fixture.root.join("records");
    let higher = records.join(format!("{:020}.json", 133));
    let mut wrong_height: serde_json::Value =
        serde_json::from_slice(&fixture.latest_record).unwrap();
    wrong_height["height"] = serde_json::json!(132);
    let mut wrong_world = wrong_height.clone();
    wrong_world["height"] = serde_json::json!(133);
    wrong_world["world_id"] = serde_json::json!("foreign-world");
    for invalid in [
        b"{broken-record".to_vec(),
        serde_json::to_vec(&wrong_height).unwrap(),
        serde_json::to_vec(&wrong_world).unwrap(),
    ] {
        fs::write(&higher, &invalid).unwrap();
        assert!(
            fixture.restart().is_err(),
            "a higher invalid numeric record cannot be skipped"
        );
        fixture.state_unchanged(&fixture.prefix_state);
        assert!(fs::read(&higher).unwrap() == invalid);
        fs::remove_file(&higher).unwrap();
    }
    fs::create_dir(&higher).unwrap();
    assert!(
        fixture.restart().is_err(),
        "a numeric directory cannot be skipped as a head"
    );
    fixture.state_unchanged(&fixture.prefix_state);
    assert!(higher.is_dir());
    fs::remove_dir(&higher).unwrap();
    #[cfg(unix)]
    {
        let external = fixture.root.join("external-record.json");
        fs::write(&external, &fixture.latest_record).unwrap();
        std::os::unix::fs::symlink(&external, &higher).unwrap();
        assert!(
            fixture.restart().is_err(),
            "a numeric symlink cannot be skipped or followed"
        );
        fixture.state_unchanged(&fixture.prefix_state);
        assert!(fs::read(&external).unwrap() == fixture.latest_record);
        fs::remove_file(&higher).unwrap();
    }
    let latest_pointer = records.join("latest.json");
    let original_pointer = fs::read(&latest_pointer).unwrap();
    for (height, conflict) in [(133, false), (132, true), (131, true)] {
        let mut pointer: serde_json::Value =
            serde_json::from_slice(&fixture.latest_record).unwrap();
        pointer["height"] = serde_json::json!(height);
        if conflict {
            pointer["execution_block_hash"] = serde_json::json!("foreign-pointer");
        }
        let invalid = serde_json::to_vec(&pointer).unwrap();
        fs::write(&latest_pointer, &invalid).unwrap();
        assert!(
            fixture.restart().is_err(),
            "missing-higher or conflicting pointer must not authorize recovery"
        );
        fixture.state_unchanged(&fixture.prefix_state);
        assert!(fs::read(&latest_pointer).unwrap() == invalid);
        fs::write(&latest_pointer, &original_pointer).unwrap();
    }
    let mut conflicting_baseline = fixture.baseline.clone();
    conflicting_baseline.execution_block_hash = "foreign-canonical-cut".into();
    assert!(
        NodeRuntimeExecutionDriver::new_with_local_bootstrap(
            fixture.root.join("state.json"),
            fixture.root.join("world"),
            fixture.root.join("records"),
            fixture.root.join("store"),
            &StorageProfileConfig::default(),
            conflicting_baseline,
        )
        .is_err()
    );
    fixture.state_unchanged(&fixture.prefix_state);
    let identity = fixture.root.join("world/world-service-identity.json");
    let identity_bytes = fs::read(&identity).unwrap();
    fs::remove_file(&identity).unwrap();
    assert!(fixture.restart().is_err());
    fixture.state_unchanged(&fixture.prefix_state);
    fs::write(&identity, identity_bytes).unwrap();
    let blob = fixture
        .root
        .join("store/blobs")
        .join(format!("{}.blob", fixture.baseline.execution_state_root));
    let bytes = fs::read(&blob).unwrap();
    fs::remove_file(&blob).unwrap();
    assert!(fixture.restart().is_err());
    fixture.state_unchanged(&fixture.prefix_state);
    fs::write(blob, bytes).unwrap();
}
