//! Exercises the exact live V3 queue selection seam. World authority and actual
//! quorum completion remain covered by chain_runtime's four-Node integration.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
fn context() -> crate::NodeBftExecutionContextV3 {
    crate::NodeBftExecutionContextV3 {
        scope: oasis7_proto::distributed_bft::v2::BftScopeV2 {
            world_id: "world".into(),
            chain_id: "chain".into(),
            genesis_hash: "a".repeat(64),
        },
        activation_digest: "b".repeat(64),
        height: 44,
        parent_commit_ref: "c".repeat(64),
        authority_epoch: 1,
        parent_state_root: "d".repeat(64),
        parent_tick: 43,
        active_validator_set_hash: "e".repeat(64),
        schedule_policy_hash: "f".repeat(64),
        manifest_hash: "0".repeat(64),
    }
}
fn governance(id: u64, nonce: u64, parent: u64) -> NodeConsensusAction {
    NodeConsensusAction::from_payload(id,"queued-node",serde_cbor::to_vec(&serde_json::json!({
        "version":1,"auth":{"type":"governance_action","data":{
            "schema_version":1,"world_id":"world","chain_id":"chain","genesis_digest":"a".repeat(64),
            "parent_position":parent,"parent_record_id":"c".repeat(64),"parent_state_root":"d".repeat(64),
            "policy_digest":"e".repeat(64),"controller_account_id":"controller","nonce":nonce,
            "valid_from_position":parent+1,"valid_through_position":parent+1,"signatures":{"1".repeat(64):"2".repeat(128)}
        }},"body":{"type":"runtime_action","data":{"action":{"type":"SubmitGovernanceValidatorAdmission","data":{"controller_account_id":"controller","original":"unchanged typed request"}}}}
    })).unwrap()).unwrap()
}
fn ordinary(id: u64) -> NodeConsensusAction {
    NodeConsensusAction::from_payload(id,"queued-node",serde_cbor::to_vec(&serde_json::json!({"type":"RegisterAgent","data":{"agent_id":"legal-followup","pos":{"x":0,"y":0,"z":0}}})).unwrap()).unwrap()
}
#[test]
fn v3_queued_stale_duplicate_and_descending_governance_do_not_block_followup() {
    let current = governance(2, 7, 43);
    let next = ordinary(5);
    let mut pending = vec![
        next.clone(),
        governance(4, 6, 43),
        governance(1, 1, 41),
        governance(3, 7, 43),
        current.clone(),
    ];
    let bytes = AtomicUsize::new(pending.iter().map(|a| a.payload_cbor.len()).sum());
    let selected = select_current(&mut pending, &bytes, &context(), 1024).unwrap();
    assert_eq!(selected, vec![current.clone(), next.clone()]);
    assert_eq!(pending, selected);
    assert_eq!(
        bytes.load(Ordering::Acquire),
        current.payload_cbor.len() + next.payload_cbor.len()
    );
    // The original request bytes, auth, parent and signatures are forwarded
    // exactly. Filtering creates no completion/receipt for discarded commands.
    assert_eq!(selected[0].payload_cbor, current.payload_cbor);
    let forwarded_ids = selected.iter().map(|a| a.action_id).collect::<Vec<_>>();
    assert_eq!(forwarded_ids, [2, 5]);
    let mut advanced = context();
    advanced.height = 45;
    advanced.parent_tick = 44;
    advanced.parent_commit_ref = "9".repeat(64);
    let selected = select_current(&mut pending, &bytes, &advanced, 1024).unwrap();
    assert_eq!(selected, vec![next.clone()]);
    assert_eq!(bytes.load(Ordering::Acquire), next.payload_cbor.len());
    let fresh = governance(6, 8, 43);
    pending.push(fresh); // stale at the advanced parent, cannot be refreshed in place
    bytes.store(
        pending.iter().map(|a| a.payload_cbor.len()).sum(),
        Ordering::Release,
    );
    assert_eq!(
        select_current(&mut pending, &bytes, &advanced, 1024).unwrap(),
        vec![next]
    );
}

#[test]
fn v3_queue_checks_every_frozen_parent_field_and_preserves_other_reservations() {
    let original = governance(1, 1, 43);
    for (field, value) in [
        ("world_id", serde_json::json!("other")),
        ("chain_id", serde_json::json!("other")),
        ("genesis_digest", serde_json::json!("9".repeat(64))),
        ("parent_position", serde_json::json!(42)),
        ("parent_record_id", serde_json::json!("9".repeat(64))),
        ("parent_state_root", serde_json::json!("9".repeat(64))),
        ("valid_from_position", serde_json::json!(45)),
        ("valid_through_position", serde_json::json!(43)),
    ] {
        let mut value_wire: serde_json::Value =
            serde_cbor::from_slice(&original.payload_cbor).unwrap();
        value_wire["auth"]["data"][field] = value;
        let bad = NodeConsensusAction::from_payload(
            1,
            "queued-node",
            serde_cbor::to_vec(&value_wire).unwrap(),
        )
        .unwrap();
        let good = ordinary(2);
        let mut queue = vec![bad, good.clone()];
        let held_reservation = 117;
        let bytes = AtomicUsize::new(
            queue.iter().map(|a| a.payload_cbor.len()).sum::<usize>() + held_reservation,
        );
        let selected = select_current(&mut queue, &bytes, &context(), 1024).unwrap();
        assert_eq!(
            selected.iter().map(|a| a.action_id).collect::<Vec<_>>(),
            [2],
            "{field}"
        );
        assert_eq!(
            bytes.load(Ordering::Acquire),
            good.payload_cbor.len() + held_reservation,
            "{field}"
        );
    }
    // A current request still reaches the original World batch validator. The
    // queue must not interpret an unavailable hook or storage as stale input.
    let mut queue = vec![original.clone()];
    let bytes = AtomicUsize::new(original.payload_cbor.len());
    assert_eq!(
        select_current(&mut queue, &bytes, &context(), 1024).unwrap(),
        vec![original]
    );
}
