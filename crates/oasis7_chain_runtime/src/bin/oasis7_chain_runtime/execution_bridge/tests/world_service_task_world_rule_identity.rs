//! Trusted setup scope for the private WorldRule gameplay fixture only.
use super::*;

pub(super) fn install_provider_fixture(world: &mut oasis7_runtime::World, world_rule: bool) {
    if world_rule {
        // Cover the bounded 360s service window at 2s/tick plus setup; remain finite.
        world
            .install_test_provider_capability_fixture_without_cognition_balance_with_lifetime(
                "agent-a", 256,
            )
            .unwrap();
    } else {
        world
            .install_test_provider_capability_fixture_without_cognition_balance("agent-a")
            .unwrap();
    }
}

pub(crate) fn pin_world_rule_resource_identity(
    driver: &mut NodeRuntimeExecutionDriver,
    identity: &WorldIdentity,
) {
    let manifest = driver.execution_world.chain_resource_manifest();
    assert_eq!(manifest.world_id, identity.world_id);
    assert_eq!(manifest.chain_id, "w1");
    assert_eq!(
        manifest.genesis_ref.as_deref(),
        Some(identity.genesis_digest.as_str())
    );
    driver.resource_identity = Some(
        super::super::super::super::super::driver::ExecutionResourceIdentity {
            chain_id: manifest.chain_id.clone(),
            genesis_ref: identity.genesis_digest.clone(),
        },
    );
}

fn assert_cas_scope(dir: &std::path::Path, height: u64, identity: &WorldIdentity) {
    let record =
        load_execution_bridge_record(&execution_bridge_record_path(&dir.join("records"), height))
            .unwrap();
    let store = oasis7_runtime::LocalCasStore::new(dir.join("store"));
    let bytes = store
        .get_verified(record.snapshot_ref.as_deref().unwrap())
        .unwrap();
    let snapshot: oasis7_runtime::Snapshot = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(snapshot.chain_resource_manifest.world_id, identity.world_id);
    assert_eq!(snapshot.chain_resource_manifest.chain_id, "w1");
    assert_eq!(
        snapshot.chain_resource_manifest.genesis_ref.as_deref(),
        Some(identity.genesis_digest.as_str())
    );
}

#[test]
fn world_rule_resource_scope_survives_commit_cas_and_restart() {
    let _permit = super::super::private_task_resource_permit();
    let _slot = super::super::prepared_world_probe_slot();
    let (dir, mut driver, identity, owner, _) = super::world_rule_fixture();
    let create = game_task_tests::create(&identity, &owner, "scope-task");
    commit(&mut driver, 3, create.clone());
    assert_cas_scope(&dir, 3, &identity);
    drop(driver);
    let bootstrap =
        super::super::super::super::super::local_bootstrap::derive_service_execution_bootstrap(
            &dir.join("world"),
            &dir.join("records"),
            &dir.join("store"),
            "w1",
        )
        .unwrap();
    let mut restarted = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
        &oasis7_proto::storage_profile::StorageProfileConfig::default(),
        bootstrap,
    )
    .unwrap();
    pin_world_rule_resource_identity(&mut restarted, &identity);
    commit(&mut restarted, 4, create);
    assert_cas_scope(&dir, 4, &identity);
    drop(restarted);
    fs::remove_dir_all(dir).unwrap();
}

fn signed_agent_view(dir: &std::path::Path, identity: &WorldIdentity, owner: &str) -> String {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    let signed = sign_read_request(
        VIEW_PATH,
        ReadWorldViewRequest {
            contract_version: 1,
            world: identity.clone(),
            scope_id: "agent:agent-a".into(),
            observation_nonce: None,
            formal_anchor: None,
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        },
        owner,
    )
    .unwrap();
    let body = serde_json::to_string(&signed).unwrap();
    let bytes = format!(
        "POST {VIEW_PATH} HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    let runtime = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    let key = hex::encode([9; 32]);
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        public_key_hex: sign_read_request("key", (), &key)
            .unwrap()
            .subject_public_key,
        private_key_hex: key,
    };
    std::thread::scope(|scope| {
        let reader = scope.spawn(move || {
            let mut response = String::new();
            client.read_to_string(&mut response).unwrap();
            response
        });
        assert!(
            crate::world_service_api::maybe_handle(
                &mut stream,
                bytes.as_bytes(),
                &runtime,
                "POST",
                VIEW_PATH,
                "w1",
                &dir.join("world"),
                &dir.join("records"),
                &dir.join("store"),
                &signer,
            )
            .unwrap()
        );
        drop(stream);
        reader.join().unwrap()
    })
}

#[test]
fn world_rule_finite_provider_lifetime_keeps_signed_task_read_live_then_expires() {
    let _permit = super::super::private_task_resource_permit();
    let _slot = super::super::prepared_world_probe_slot();
    let (dir, mut driver, identity, owner, _) = super::world_rule_fixture();
    let create = game_task_tests::create(&identity, &owner, "lifetime-task");
    commit(&mut driver, 3, create);
    for height in 4..=105 {
        commit_empty_tick(&mut driver, height);
    }
    assert!(driver.execution_world.state().time > 102);
    let response = signed_agent_view(&dir, &identity, &owner);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(
        response.contains("lifetime-task"),
        "signed read must retain task control"
    );
    assert!(
        response.contains("valid_until_tick"),
        "signed read must carry its finite capability catalog"
    );
    for height in 106..=260 {
        commit_empty_tick(&mut driver, height);
    }
    let response = signed_agent_view(&dir, &identity, &owner);
    assert!(response.starts_with("HTTP/1.1 503"), "{response}");
    assert!(response.contains("world_service_unavailable"), "{response}");
    assert!(
        driver
            .execution_world
            .canonical_agent_service_context("agent-a")
            .unwrap_err()
            .contains("no live command capability grant")
    );
    drop(driver);
    fs::remove_dir_all(dir).unwrap();
}

fn commit_empty_tick(driver: &mut NodeRuntimeExecutionDriver, height: u64) {
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height,
            slot: height,
            epoch: 0,
            node_block_hash: format!("node-h{height}"),
            action_root: compute_consensus_action_root(&[]).unwrap(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: height as i64 * 1000,
        })
        .unwrap();
}
