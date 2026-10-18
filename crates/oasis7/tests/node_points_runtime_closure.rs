#![cfg(any(feature = "test_tier_required", feature = "test_tier_full"))]
use oasis7::node_points_observation::measure_directory_storage_bytes;

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oasis7::runtime::{NodePointsConfig, NodePointsRuntimeCollector, NodePointsRuntimeHeuristics};
use oasis7_node::{NodeConfig, NodeReplicationConfig, NodeRole, NodeRuntime};

#[test]
fn node_runtime_multi_node_points_closure_settles_rewards() {
    let world_id = unique_id("world");
    let seq_root = temp_dir("np-seq");
    let storage_root = temp_dir("np-storage");
    let observer_root = temp_dir("np-observer");

    let mut sequencer = build_runtime(
        unique_id("sequencer"),
        world_id.clone(),
        NodeRole::Sequencer,
        seq_root.clone(),
    );
    let mut storage = build_runtime(
        unique_id("storage"),
        world_id.clone(),
        NodeRole::Storage,
        storage_root.clone(),
    );
    let mut observer = build_runtime(
        unique_id("observer"),
        world_id,
        NodeRole::Observer,
        observer_root.clone(),
    );

    sequencer.start().expect("sequencer start");
    storage.start().expect("storage start");
    observer.start().expect("observer start");

    let config = NodePointsConfig {
        epoch_duration_seconds: 60,
        epoch_pool_points: 300,
        min_self_sim_compute_units: 1,
        ..NodePointsConfig::default()
    };
    let mut collector =
        NodePointsRuntimeCollector::new(config, NodePointsRuntimeHeuristics::default());

    for _ in 0..18 {
        sample_runtime(&mut collector, &sequencer, seq_root.as_path());
        sample_runtime(&mut collector, &storage, storage_root.as_path());
        sample_runtime(&mut collector, &observer, observer_root.as_path());
        thread::sleep(Duration::from_millis(80));
    }

    let epoch0 = collector
        .force_settle()
        .expect("force settle")
        .expect("epoch0 report");
    assert_eq!(epoch0.pool_points, 300);
    assert_eq!(epoch0.distributed_points, 300);
    assert_eq!(epoch0.settlements.len(), 3);
    assert_eq!(epoch0.epoch_index, 0);

    // Keep sampling to build the next epoch and verify cumulative accounting.
    for _ in 0..15 {
        sample_runtime(&mut collector, &sequencer, seq_root.as_path());
        sample_runtime(&mut collector, &storage, storage_root.as_path());
        sample_runtime(&mut collector, &observer, observer_root.as_path());
        thread::sleep(Duration::from_millis(70));
    }

    let epoch1 = collector
        .force_settle()
        .expect("force settle")
        .expect("epoch1 report");
    assert_eq!(epoch1.pool_points, 300);
    assert_eq!(epoch1.distributed_points, 300);
    assert_eq!(epoch1.settlements.len(), 3);
    assert_eq!(epoch1.epoch_index, 1);

    let mut positive_award_nodes = 0_u64;
    let mut total_cumulative = 0_u64;
    for settlement in &epoch1.settlements {
        if settlement.awarded_points > 0 {
            positive_award_nodes = positive_award_nodes.saturating_add(1);
        }
        assert!(settlement.cumulative_points >= settlement.awarded_points);
        total_cumulative = total_cumulative.saturating_add(settlement.cumulative_points);
    }
    assert!(positive_award_nodes >= 2);
    assert_eq!(total_cumulative, 600);

    sequencer.stop().expect("sequencer stop");
    storage.stop().expect("storage stop");
    observer.stop().expect("observer stop");

    let _ = fs::remove_dir_all(seq_root);
    let _ = fs::remove_dir_all(storage_root);
    let _ = fs::remove_dir_all(observer_root);
}

fn sample_runtime(
    collector: &mut NodePointsRuntimeCollector,
    runtime: &NodeRuntime,
    replication_root: &std::path::Path,
) {
    let snapshot = runtime.snapshot();
    let storage_bytes = measure_directory_storage_bytes(replication_root);
    let _ = collector
        .observe(
            oasis7::node_points_observation::node_points_observation_from_snapshot(
                &snapshot,
                storage_bytes,
                now_unix_ms(),
            ),
        )
        .expect("observe snapshot");
}

fn build_runtime(
    node_id: String,
    world_id: String,
    role: NodeRole,
    replication_root: PathBuf,
) -> NodeRuntime {
    let replication =
        NodeReplicationConfig::new(replication_root).expect("replication config from path");
    let config = NodeConfig::new(node_id, world_id, role)
        .expect("node config")
        .with_tick_interval(Duration::from_millis(20))
        .expect("tick")
        .with_replication(replication);
    NodeRuntime::new(config)
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_millis() as i64
}

fn unique_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    format!("{prefix}-{nanos}")
}

fn temp_dir(prefix: &str) -> PathBuf {
    let path = std::env::temp_dir().join(unique_id(prefix));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}
