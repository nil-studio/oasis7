//! Failure-only facts; omit paths, peer/key identities and arbitrary error text.
use super::*;

pub(super) fn summary(runtime: &NodeRuntime, root: &std::path::Path) -> serde_json::Value {
    let snapshot = runtime.snapshot();
    let consensus = &snapshot.consensus;
    let gossip = runtime.gossip_traffic_snapshot();
    let files = LocalCasStore::new(root.join("store")).list_files();
    serde_json::json!({
        "tick_count": snapshot.tick_count,
        "last_error_present": snapshot.last_error.is_some(),
        "slot": consensus.slot,
        "last_observed_slot": consensus.last_observed_slot,
        "missed_slot_count": consensus.missed_slot_count,
        "committed_height": consensus.committed_height,
        "network_committed_height": consensus.network_committed_height,
        "replication_persisted_height": consensus.replication_persisted_height,
        "replication_traffic": gossip.as_ref().and_then(|g| g.by_kind.get("replication")),
        "gossip_error_counts": gossip.as_ref().map(|g| &g.by_error_kind),
        "store_list_failed": files.is_err(),
        "commit_file_count": files.as_ref().ok().map(|items| items.iter()
            .filter(|item| item.path.starts_with("consensus/commits/")).count()),
        "remote_guard_exists": root.join("replication_remote_guards.json").exists(),
    })
}

#[test]
fn signed_gossip_failure_summary_omits_error_text_and_runtime_identity() {
    let root = temp_dir("signed-gossip-summary");
    let runtime = NodeRuntime::new(
        NodeConfig::new(
            "private-node-identity",
            "private-world-identity",
            NodeRole::Observer,
        )
        .unwrap(),
    );
    runtime.state.lock().unwrap().last_error = Some("private-error-payload".into());
    let facts = summary(&runtime, &root);
    assert_eq!(facts["last_error_present"], true);
    assert_eq!(facts["tick_count"], 0);
    assert_eq!(facts["remote_guard_exists"], false);
    assert!(facts["replication_traffic"].is_null());
    let rendered = facts.to_string();
    for private in [
        "private-node-identity",
        "private-world-identity",
        "private-error-payload",
    ] {
        assert!(!rendered.contains(private));
    }
    let _ = fs::remove_dir_all(root);
}
