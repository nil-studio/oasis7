use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7::geometry::GeoPos;
use oasis7_node::{
    BftTimeoutsV1, NodeBftConfigV1, NodeConfig, NodeExecutionHook, NodeReplicationConfig, NodeRole,
    NodeRuntime,
};
use oasis7_runtime::Action as RuntimeAction;

use super::driver_bft::{bft_genesis_world, driver_for_world, keys, scope, validator_set};
use super::temp_dir;

const NODE_COUNT: usize = 4;
const AUTHORITY_EPOCH: u64 = 9;
const TARGET_HEIGHT: u64 = 1;

fn ephemeral_gossip_addresses() -> Vec<SocketAddr> {
    let sockets = (0..NODE_COUNT)
        .map(|_| UdpSocket::bind("127.0.0.1:0").expect("reserve loopback UDP address"))
        .collect::<Vec<_>>();
    let addresses = sockets
        .iter()
        .map(|socket| socket.local_addr().expect("read loopback address"))
        .collect();
    drop(sockets);
    addresses
}

fn bft_config(
    scope: oasis7_node::BftScopeV1,
    validators: Vec<oasis7_node::BftValidatorV1>,
    local_validator_id: String,
) -> NodeBftConfigV1 {
    NodeBftConfigV1 {
        scope,
        authority_epoch: AUTHORITY_EPOCH,
        validators,
        local_validator_id,
        timeouts: BftTimeoutsV1 {
            propose_ms: 300,
            prevote_ms: 300,
            precommit_ms: 300,
            round_delta_ms: 100,
        },
    }
}

fn make_runtime(
    index: usize,
    root: &std::path::Path,
    gossip_addresses: &[SocketAddr],
    scope: &oasis7_node::BftScopeV1,
    validators: &[oasis7_node::BftValidatorV1],
    keys: &[SigningKey],
) -> NodeRuntime {
    let node_id = format!("v{index}");
    let replication = NodeReplicationConfig::new(root.join(format!("replication-{index}")))
        .expect("replication config")
        .with_signing_keypair(
            hex::encode(keys[index].to_bytes()),
            hex::encode(keys[index].verifying_key().to_bytes()),
        )
        .expect("pin validator signing key");
    let peers = gossip_addresses
        .iter()
        .enumerate()
        .filter_map(|(peer_index, address)| (peer_index != index).then_some(*address))
        .collect();
    let node_config = NodeConfig::new(node_id.clone(), scope.world_id.clone(), NodeRole::Sequencer)
        .expect("node config")
        .with_player_id("shared-bft-runtime-test-player")
        .expect("shared replicated action identity")
        .with_tick_interval(Duration::from_millis(20))
        .expect("node tick interval")
        .with_replication(replication)
        .with_gossip(gossip_addresses[index], peers)
        .expect("all-to-all loopback gossip config");
    let driver_root = root.join(format!("driver-{index}"));
    NodeRuntime::new(node_config)
        .with_replication_network_consensus_enabled(false)
        .with_execution_hook(driver_for_world(
            driver_root.as_path(),
            bft_genesis_world(scope),
        ))
        .with_tendermint_v1(bft_config(scope.clone(), validators.to_vec(), node_id))
        .expect("explicit fixed-set BFT config")
}

fn wait_for_same_head(
    nodes: &[NodeRuntime],
    minimum_height: u64,
) -> Vec<oasis7_node::NodeSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let snapshots = nodes.iter().map(NodeRuntime::snapshot).collect::<Vec<_>>();
        let all_reached = snapshots
            .iter()
            .all(|snapshot| snapshot.consensus.committed_height >= minimum_height);
        let same_head = snapshots.first().is_some_and(|first| {
            snapshots.iter().all(|snapshot| {
                snapshot.consensus.committed_height == first.consensus.committed_height
                    && snapshot.consensus.last_block_hash == first.consensus.last_block_hash
            })
        });
        if all_reached && same_head {
            return snapshots;
        }
        if let Some(failed) = snapshots
            .iter()
            .find(|snapshot| snapshot.last_error.is_some() || !snapshot.running)
        {
            let gossip = nodes
                .iter()
                .map(NodeRuntime::gossip_traffic_snapshot)
                .collect::<Vec<_>>();
            panic!("BFT node stopped before reaching a shared head: {failed:?}; gossip={gossip:?}");
        }
        let gossip = nodes
            .iter()
            .map(NodeRuntime::gossip_traffic_snapshot)
            .collect::<Vec<_>>();
        assert!(
            Instant::now() < deadline,
            "four-node BFT did not converge: {snapshots:?}; gossip={gossip:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn runtime_action_payload() -> Vec<u8> {
    encode_consensus_action_payload(&ConsensusActionPayloadEnvelope::from_runtime_action(
        RuntimeAction::RegisterAgent {
            agent_id: "node-runtime-bft-agent".into(),
            pos: GeoPos::new(1, 2, 3),
        },
    ))
    .expect("encode a real runtime action")
}

#[test]
fn four_node_runtime_commits_and_replays_through_real_execution_drivers() {
    let base = std::env::temp_dir()
        .canonicalize()
        .expect("canonical temp root");
    let allocated = temp_dir("bft-node-runtime");
    let name = allocated
        .file_name()
        .expect("unique test dir basename")
        .to_string_lossy();
    let root = base.join(name.as_ref());
    std::fs::create_dir(&root).expect("create private test root");

    let keys = keys();
    let scope = scope();
    let set = validator_set(&scope, &keys);
    let validators = set.validators().values().cloned().collect::<Vec<_>>();
    let addresses = ephemeral_gossip_addresses();
    let payload = runtime_action_payload();
    let mut nodes = (0..NODE_COUNT)
        .map(|index| make_runtime(index, &root, &addresses, &scope, &validators, &keys))
        .collect::<Vec<_>>();

    for node in &nodes {
        node.submit_consensus_action_payload(1, payload.clone())
            .expect("queue authenticated ordinary RuntimeAction before BFT starts");
    }
    for node in &mut nodes {
        node.start()
            .expect("start real BFT runtime with real driver hook");
    }
    let first_head = wait_for_same_head(&nodes, TARGET_HEIGHT);
    let committed_height = first_head[0].consensus.committed_height;
    assert!(committed_height >= TARGET_HEIGHT);

    let committed_records = (0..NODE_COUNT)
        .map(|index| {
            let record = root.join(format!(
                "driver-{index}/records/bft-execution-v1/height-{committed_height:020}.cbor"
            ));
            assert!(
                record.is_file(),
                "missing BFT-only driver record: {}",
                record.display()
            );
            assert!(
                !root
                    .join(format!(
                        "driver-{index}/records/{committed_height:020}.json"
                    ))
                    .exists(),
                "BFT commit must not create a legacy Pos execution record"
            );
            std::fs::read(record).expect("read committed BFT record for replay comparison")
        })
        .collect::<Vec<_>>();

    for node in &mut nodes {
        node.stop().expect("stop node before same-instance replay");
    }
    for node in &mut nodes {
        node.start()
            .expect("same pinned BFT config replays existing real driver history");
    }
    let restarted_head = wait_for_same_head(&nodes, committed_height);
    assert!(restarted_head[0].consensus.committed_height >= committed_height);
    let restarted_height = restarted_head[0].consensus.committed_height;
    let restarted_block_id = restarted_head[0]
        .consensus
        .last_block_hash
        .clone()
        .expect("restarted BFT committed block identity");
    for (index, original_record) in committed_records.iter().enumerate() {
        let path = root.join(format!(
            "driver-{index}/records/bft-execution-v1/height-{committed_height:020}.cbor"
        ));
        assert_eq!(
            &std::fs::read(path).expect("read original BFT evidence after restart"),
            original_record,
            "replaying the exact QC must not rewrite its historical execution record"
        );
    }
    // A live snapshot does not freeze consensus: retain a genuine later QC before
    // stopping, so cold recovery must use the final stopped observation.
    let advanced_head = wait_for_same_head(
        &nodes,
        restarted_height.checked_add(1).expect("next BFT height"),
    );
    assert!(
        advanced_head
            .iter()
            .all(|snapshot| snapshot.consensus.committed_height > restarted_height),
        "real BFT must commit beyond the earlier restart observation"
    );
    let advanced_height = advanced_head[0].consensus.committed_height;
    let advanced_block_id = advanced_head[0]
        .consensus
        .last_block_hash
        .as_ref()
        .expect("newer shared BFT block identity");
    for node in &mut nodes {
        node.stop().expect("stop restarted node");
    }
    // stop joins the worker; these observations cannot race later commits.
    // Nodes may have different valid tails because shutdown is not an atomic
    // network-wide barrier. Their overlapping certified blocks must still agree.
    let stopped_heads = nodes.iter().map(NodeRuntime::snapshot).collect::<Vec<_>>();
    for snapshot in &stopped_heads {
        assert!(!snapshot.running, "cold recovery requires a stopped node");
        assert!(
            snapshot.consensus.committed_height >= advanced_height,
            "shutdown must not lose the newer observed committed height"
        );
        assert!(
            snapshot.last_error.is_none(),
            "stopped node must not hide a runtime failure"
        );
    }
    drop(nodes);

    // A fresh driver process re-verifies each raw QC and materializes the same
    // committed World; it cannot trust a serialized verified token.
    let config = oasis7_node::NodeBftExecutionConfigV1::from_validator_set(&set, AUTHORITY_EPOCH);
    let mut certified_blocks = BTreeMap::<u64, String>::new();
    for (index, stopped) in stopped_heads.iter().enumerate() {
        let final_height = stopped.consensus.committed_height;
        let final_block_id = stopped
            .consensus
            .last_block_hash
            .as_ref()
            .expect("stopped BFT committed block identity");
        let mut driver = driver_for_world(
            root.join(format!("driver-{index}")).as_path(),
            bft_genesis_world(&scope),
        );
        let head = driver
            .configure_bft_execution(&config)
            .expect("reopen BFT driver with its exact pinned configuration")
            .expect("durable BFT head");
        assert_eq!(
            head.height, final_height,
            "cold driver must recover its exact stopped Node height"
        );
        assert!(
            &head.block_id == final_block_id,
            "cold driver must recover its exact stopped Node block"
        );
        let mut history = Vec::with_capacity(final_height as usize);
        let mut after_height = 0;
        loop {
            let page = driver
                .load_bft_commit_history_page(
                    &config,
                    after_height,
                    oasis7_node::BFT_MAX_COMMIT_HISTORY_PAGE_ENTRIES_V1,
                )
                .expect("read bounded raw QC history page");
            if page.is_empty() {
                break;
            }
            after_height = page
                .last()
                .expect("nonempty history page")
                .proposal
                .block
                .height;
            history.extend(page);
        }
        assert_eq!(history.len() as u64, final_height);
        let mut schedule = oasis7_node::BftHeightProposerScheduleV1::genesis(&set, AUTHORITY_EPOCH)
            .expect("fixed genesis proposer schedule");
        let mut retained_restart_checkpoint = false;
        let mut retained_advanced_checkpoint = false;
        let mut replayed_block_id = None;
        for certificate in history {
            let token =
                oasis7_node::verify_commit_certificate_token_v1(&set, &schedule, &certificate)
                    .expect("independently verify raw QC from fixed-set schedule");
            let height = token.certificate().proposal.block.height;
            let block_id = token
                .certificate()
                .proposal
                .block
                .block_id()
                .expect("verified QC block identity");
            if height == restarted_height {
                assert!(
                    block_id == restarted_block_id,
                    "cold history must retain the original shared restart checkpoint"
                );
                retained_restart_checkpoint = true;
            }
            if height == advanced_height {
                assert!(
                    &block_id == advanced_block_id,
                    "cold history must retain the newer shared committed checkpoint"
                );
                retained_advanced_checkpoint = true;
            }
            if let Some(previous) = certified_blocks.get(&height) {
                assert!(
                    previous == &block_id,
                    "all overlapping independently verified heights must agree"
                );
            } else {
                certified_blocks.insert(height, block_id.clone());
            }
            replayed_block_id = Some(block_id);
            driver
                .commit_bft_certificate(token.clone())
                .expect("restore exact QC through the production driver hook");
            schedule = token.next_height_schedule().clone();
        }
        assert!(
            retained_restart_checkpoint,
            "each cold history must contain the original common checkpoint"
        );
        assert!(
            retained_advanced_checkpoint,
            "each cold history must contain the newer common checkpoint"
        );
        assert!(
            replayed_block_id.as_ref() == Some(final_block_id),
            "independently verified history must end at the stopped Node block"
        );
        assert_eq!(schedule.height(), final_height + 1);
        assert!(
            driver
                .execution_world
                .state()
                .agents
                .contains_key("node-runtime-bft-agent"),
            "the committed real action must materialize after cold QC replay"
        );
    }
    for (index, original_record) in committed_records.iter().enumerate() {
        let path = root.join(format!(
            "driver-{index}/records/bft-execution-v1/height-{committed_height:020}.cbor"
        ));
        assert!(
            &std::fs::read(path).expect("read original BFT evidence after cold replay")
                == original_record,
            "cold replay must not rewrite the original shared-height record"
        );
    }
    std::fs::remove_dir_all(root).expect("remove isolated BFT Node test state");
}
