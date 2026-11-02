//! Real runtime admission under a held hook, with bounded cleanup even on old code.
use super::*;
use crate::{NodeExecutionCommitContext, NodeExecutionCommitResult, NodeRole};
use std::sync::mpsc;
use std::time::Duration;
struct ClosedHook;
impl NodeExecutionHook for ClosedHook {
    fn guarded_local_execution(&self) -> bool {
        true
    }
    fn validate_cleanup_v2_action_payload(&self, q: &TaskCleanupSubmitV2) -> Result<(), String> {
        q.verify_owner_signature()
    }
    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("admission-only test never commits".into())
    }
}
#[test]
fn closed_task_preflight_contention_returns_not_queued_before_hook_release() {
    let config = NodeConfig::new("node", "world", NodeRole::Observer).unwrap();
    let runtime = NodeRuntime::new(config).with_execution_hook(ClosedHook);
    let original = close_catalog(request());
    runtime
        .submit_consensus_action_payload(70, original.clone())
        .unwrap();
    let hook = runtime.execution_hook.as_ref().unwrap().clone();
    let held = hook.lock().unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let result = runtime.submit_consensus_action_payload(71, original);
        tx.send(result).unwrap();
        runtime
    });
    let before_release = rx.recv_timeout(Duration::from_millis(150));
    // Always release/join before asserting, so the old blocking code produces
    // a target failure instead of leaving a deadlocked test process.
    drop(held);
    let runtime = worker.join().unwrap();
    runtime
        .submit_consensus_action_payload(72, close_catalog(request()))
        .unwrap();
    assert!(
        matches!(before_release, Ok(Err(_))),
        "closed admission must refuse hook contention before waiting for a tick holding the hook"
    );
}

#[test]
fn closed_task_bft_v1_selected_config_refuses_before_hook_admission() {
    let validator = ed25519_dalek::SigningKey::from_bytes(&[91; 32]);
    let bft = crate::NodeBftConfigV1 {
        scope: oasis7_proto::distributed_bft::BftScopeV1 {
            world_id: "world".into(),
            chain_id: "chain".into(),
            genesis_hash: "aa".repeat(32),
        },
        authority_epoch: 1,
        validators: vec![oasis7_consensus::tendermint_v1::BftValidatorV1 {
            validator_id: "validator".into(),
            public_key_hex: hex::encode(validator.verifying_key().to_bytes()),
            voting_power: 1,
        }],
        local_validator_id: "validator".into(),
        timeouts: oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
            propose_ms: 100,
            prevote_ms: 100,
            precommit_ms: 100,
            round_delta_ms: 10,
        },
    };
    let runtime =
        NodeRuntime::new(NodeConfig::new("validator", "world", NodeRole::Sequencer).unwrap())
            .with_execution_hook(ClosedHook)
            .with_tendermint_v1(bft)
            .unwrap();
    let err = runtime
        .submit_consensus_action_payload(81, close_catalog(request()))
        .unwrap_err();
    assert!(
        matches!(err,crate::NodeError::Consensus {reason} if reason=="closed task mutation requires non-BFT Formal profile; not queued")
    );
}
