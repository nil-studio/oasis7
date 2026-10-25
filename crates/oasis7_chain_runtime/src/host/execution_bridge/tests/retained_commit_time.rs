use super::super::driver::NodeRuntimeExecutionDriver;
use super::driver_authoritative_recovery::{replay_context, simulator_committed_action};
use super::*;
use oasis7_node::NodeExecutionHook;

#[test]
fn retained_local_commit_time_recovers_original_and_rejects_conflicts() {
    let dir = temp_dir("retained-local-commit-time");
    let mut driver = NodeRuntimeExecutionDriver::new(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
    )
    .expect("driver");
    let original = replay_context("node-a", "node-h1", vec![simulator_committed_action(1, 1)]);
    assert_eq!(
        driver
            .retained_local_commit_time(&original)
            .expect("new decision"),
        None
    );
    driver
        .on_commit(original.clone())
        .expect("durable execution before node acknowledgement");
    drop(driver);
    let mut driver = NodeRuntimeExecutionDriver::new(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
    )
    .expect("reopen durable driver");
    let mut retry = original.clone();
    retry.committed_at_unix_ms += 60_000;
    assert_eq!(
        driver
            .retained_local_commit_time(&retry)
            .expect("recover exact decision"),
        Some(original.committed_at_unix_ms)
    );
    assert!(
        driver.on_commit(retry.clone()).is_err(),
        "direct replay still checks timestamp"
    );
    for field in ["block", "slot", "epoch", "actions"] {
        let mut conflict = retry.clone();
        match field {
            "block" => conflict.node_block_hash.push_str("-changed"),
            "slot" => conflict.slot += 1,
            "epoch" => conflict.epoch += 1,
            _ => conflict.committed_actions.clear(),
        }
        assert!(
            driver.retained_local_commit_time(&conflict).is_err(),
            "must reject {field} conflict"
        );
    }
    retry.committed_at_unix_ms = driver.retained_local_commit_time(&retry).unwrap().unwrap();
    driver
        .on_commit(retry)
        .expect("finish original without executing twice");
    let record_path =
        super::super::checkpoint::execution_bridge_record_path(&dir.join("records"), 1);
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    for legacy in [true, false] {
        let mut invalid = record.clone();
        if legacy {
            invalid["schema_version"] = serde_json::json!(2);
        } else {
            invalid["checkpoint_ref"] = serde_json::json!("fixture-checkpoint");
            invalid["proposer_id"] = serde_json::Value::Null;
            invalid["action_root"] = serde_json::Value::Null;
        }
        // Bypass the migration-aware writer to preserve a genuinely old or
        // checkpoint-install record at the recovery read boundary.
        fs::write(&record_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(driver.retained_local_commit_time(&original).is_err());
    }
    let _ = fs::remove_dir_all(dir);
}
