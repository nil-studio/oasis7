use super::super::driver::{
    NodeRuntimeExecutionDriver, simulator_world_dir_from_execution_world_dir,
};
use super::super::external_effect::load_execution_external_effect_materialization;
use super::temp_dir;
use super::*;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7::simulator::{WorldSnapshot, initialize_kernel};
use oasis7_agent_api::world_geometry::ChunkCoord;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_runtime::LocalCasStore;
use oasis7_simulator::{
    Action as SimulatorAction, ActionSubmitter, WorldConfig, WorldInitConfig, WorldScenario,
};
use std::fs;

#[test]
fn node_runtime_execution_driver_processes_simulator_payload_envelope() {
    let dir = temp_dir("execution-driver-simulator-payload");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let simulator_world_dir = simulator_world_dir_from_execution_world_dir(world_dir.as_path());
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir,
        records_dir.clone(),
        storage_root,
    )
    .expect("driver");
    let config = WorldConfig::default();
    let mut init =
        WorldInitConfig::from_scenario(WorldScenario::AsteroidFragmentBootstrap, &config);
    init.seed = 91;
    init.asteroid_fragment.bootstrap_chunks = vec![ChunkCoord { x: 0, y: 0, z: 0 }];
    let (kernel, _) = initialize_kernel(config, init).expect("simulator init");
    driver.simulator_mirror = kernel;

    let payload =
        encode_consensus_action_payload(&ConsensusActionPayloadEnvelope::from_simulator_action(
            SimulatorAction::HarvestRadiation {
                agent_id: "agent-0".to_string(),
                max_amount: 1,
            },
            ActionSubmitter::System,
        ))
        .expect("encode simulator payload");
    let committed_action = oasis7_node::NodeConsensusAction::from_payload(1, "node-a", payload)
        .expect("consensus action");
    let action_root =
        compute_consensus_action_root(std::slice::from_ref(&committed_action)).expect("root");
    let expected_action_root = action_root.clone();
    let expected_payload_hash = committed_action.payload_hash.clone();

    let result = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root,
            committed_actions: vec![committed_action],
            committed_at_unix_ms: 1_000,
        })
        .expect("commit");

    assert_eq!(result.execution_height, 1);
    assert!(records_dir.join("00000000000000000001.json").exists());
    let record_bytes = fs::read(records_dir.join("00000000000000000001.json"))
        .expect("read execution bridge record");
    let record: ExecutionBridgeRecord =
        serde_json::from_slice(record_bytes.as_slice()).expect("parse execution bridge record");
    assert_eq!(record.schema_version, EXECUTION_BRIDGE_RECORD_SCHEMA_V3);
    assert_eq!(
        record.latest_state_ref.as_deref(),
        record.snapshot_ref.as_deref()
    );
    assert!(
        record
            .snapshot_ref
            .as_deref()
            .is_some_and(|r| !r.is_empty())
    );
    assert!(record.journal_ref.as_deref().is_some_and(|r| !r.is_empty()));
    let external_effect_ref = record
        .external_effect_ref
        .as_deref()
        .expect("external effect ref should exist");
    let store = LocalCasStore::new(dir.join("store"));
    let external_effect =
        load_execution_external_effect_materialization(&store, external_effect_ref)
            .expect("load external effect materialization");
    assert_eq!(external_effect.height, 1);
    assert_eq!(external_effect.slot, 0);
    assert_eq!(external_effect.epoch, 0);
    assert_eq!(external_effect.action_root, expected_action_root);
    assert_eq!(external_effect.committed_actions.len(), 1);
    assert_eq!(external_effect.committed_actions[0].action_id, 1);
    assert_eq!(
        external_effect.committed_actions[0].payload_hash,
        expected_payload_hash
    );
    assert!(external_effect.unresolved_inputs.is_empty());
    let simulator = record
        .simulator_mirror
        .expect("simulator mirror record should exist");
    assert_eq!(simulator.action_count, 1);
    assert_eq!(simulator.rejected_action_count, 0);
    assert!(!simulator.snapshot_ref.is_empty());
    assert!(!simulator.journal_ref.is_empty());
    assert!(!simulator.state_root.is_empty());
    let simulator_snapshot_bytes = store
        .get_verified(simulator.snapshot_ref.as_str())
        .expect("load simulator snapshot");
    let simulator_snapshot: WorldSnapshot =
        serde_cbor::from_slice(simulator_snapshot_bytes.as_slice())
            .expect("decode simulator snapshot");
    let manifest = &simulator_snapshot.chain_resource_manifest;
    let delta = &simulator_snapshot.latest_chain_resource_delta;
    assert_eq!(
        (manifest.world_id.as_str(), manifest.chain_id.as_str()),
        ("w1", "w1")
    );
    let commit_hash = delta
        .commit_block_hash
        .as_deref()
        .expect("simulator resource commit hash");
    assert!(!commit_hash.is_empty());
    assert_ne!(commit_hash, "node-h1");
    assert!(!manifest.generated_chunks.is_empty());
    assert!(!delta.entries.is_empty());
    assert!(simulator_world_dir.join("snapshot.json").exists());
    assert!(simulator_world_dir.join("journal.json").exists());
    let _ = fs::remove_dir_all(dir);
}
