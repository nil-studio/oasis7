//! Modeled IPC replies test pending classification, not process-stop durability.
use super::*;

fn reply() -> formal_live_runtime::FormalRuntimeOwnerSnapshotV1 {
    formal_live_runtime::FormalRuntimeOwnerSnapshotV1 {
        api_version: "fixture".into(),
        instance: formal_live_runtime::FormalRuntimeInstanceV1 {
            api_version: "fixture".into(),
            operation_id: "same-operation".into(),
            request_digest: "a".repeat(64),
            incarnation: "b".repeat(64),
            config_digest: "c".repeat(64),
            node_id: "successor".into(),
            world_id: "world".into(),
            chain_id: "chain".into(),
            authority_id: "d".repeat(64),
            authority_epoch: 2,
            profile_version: 2,
            process_id: 42,
        },
        state: "unknown".into(),
        last_error: Some(OWNER_STOP_PENDING.into()),
    }
}

#[test]
fn exact_owner_stop_accepts_only_documented_pending_then_same_instance_stopped() {
    let mut snapshot = reply();
    let expected = snapshot.instance.clone();
    assert!(!stop_owner_snapshot_is_terminal(&expected, &snapshot).unwrap());
    snapshot.state = "stopping".into();
    assert!(!stop_owner_snapshot_is_terminal(&expected, &snapshot).unwrap());
    snapshot.state = "stopped".into();
    assert!(stop_owner_snapshot_is_terminal(&expected, &snapshot).unwrap());
    snapshot.instance.incarnation = "e".repeat(64);
    assert!(stop_owner_snapshot_is_terminal(&expected, &snapshot).is_err());
}

#[test]
fn exact_owner_stop_rejects_other_unknown_errors_and_bounds_diagnostics() {
    let mut snapshot = reply();
    let expected = snapshot.instance.clone();
    for reason in [
        None,
        Some("execution hook lock poisoned".into()),
        Some(format!("{OWNER_STOP_PENDING}; unrecognized extra failure")),
    ] {
        snapshot.last_error = reason;
        assert!(stop_owner_snapshot_is_terminal(&expected, &snapshot).is_err());
    }
    snapshot.state = "failed".into();
    snapshot.last_error = Some(OWNER_STOP_PENDING.into());
    assert!(stop_owner_snapshot_is_terminal(&expected, &snapshot).is_err());
    snapshot.state = "x".repeat(10_000);
    snapshot.last_error = Some("y".repeat(10_000));
    let error = stop_owner_snapshot_is_terminal(&expected, &snapshot).unwrap_err();
    assert!(error.len() < 800);
}
