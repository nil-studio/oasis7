use super::*;

// This oracle tests queue scheduling and revalidation only, not cryptographic authority.
#[derive(Default)]
struct ParentOracle {
    consulted: Mutex<Vec<String>>,
}
impl NodeExecutionHook for ParentOracle {
    fn validate_governance_action_payload(&self, payload: &[u8]) -> Result<(), String> {
        let value: serde_json::Value = serde_cbor::from_slice(payload).unwrap();
        let candidate = value["body"]["data"]["action"]["data"]["candidate_id"]
            .as_str()
            .unwrap()
            .to_string();
        self.consulted.lock().unwrap().push(candidate.clone());
        if candidate == "stale" {
            Err("parent changed".into())
        } else {
            Ok(())
        }
    }
    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        panic!("queue oracle does not execute World")
    }
}

fn governance(id: u64, candidate: &str, nonce: u64) -> NodeConsensusAction {
    let value = serde_json::json!({
        "version": 1,
        "auth": {"type": "governance_action", "data": {
            "schema_version": 1, "controller_account_id": "controller", "nonce": nonce,
            "signatures": {"test-oracle-key": "test-oracle-signature"}
        }},
        "body": {"type": "runtime_action", "data": {"action": {
            "type": "ApproveGovernanceValidatorAdmission", "data": {"candidate_id": candidate}
        }}}
    });
    NodeConsensusAction::from_payload(id, "node-governance", serde_cbor::to_vec(&value).unwrap())
        .unwrap()
}

fn registration(id: u64) -> NodeConsensusAction {
    NodeConsensusAction::from_payload(
        id,
        "node-governance",
        serde_cbor::to_vec(&serde_json::json!({
            "type": "RegisterAgent", "data": {"agent_id": "ordinary", "pos": {"x":0,"y":0,"z":0}}
        }))
        .unwrap(),
    )
    .unwrap()
}

fn queued(actions: Vec<NodeConsensusAction>) -> (NodeConfig, PosNodeEngine) {
    let config =
        NodeConfig::new("node-governance", "world-governance", NodeRole::Sequencer).unwrap();
    let mut engine = PosNodeEngine::new(&config).unwrap();
    merge_pending_consensus_actions_with_budget(
        &mut engine.pending_consensus_actions,
        actions,
        engine.max_pending_consensus_actions,
        engine.pending_consensus_action_queue_bytes.as_ref(),
        engine.max_pending_consensus_action_queue_bytes,
        false,
    )
    .unwrap();
    (config, engine)
}

#[test]
fn proposal_revalidation_drops_stale_duplicate_and_descending_nonces_without_blocking_progress() {
    let first = governance(1, "first", 2);
    let ordinary = registration(5);
    let actions = vec![
        ordinary.clone(),
        governance(4, "stale", 3),
        governance(3, "descending", 1),
        governance(2, "duplicate", 2),
        first.clone(),
    ];
    let before_bytes: usize = actions.iter().map(|a| a.payload_cbor.len()).sum();
    let (config, mut engine) = queued(actions);
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        before_bytes
    );
    let oracle = ParentOracle::default();
    engine.discard_invalid_governance_actions(Some(&oracle));
    assert_eq!(
        *oracle.consulted.lock().unwrap(),
        ["first", "duplicate", "descending", "stale"]
    );
    assert_eq!(
        engine
            .pending_consensus_actions
            .values()
            .cloned()
            .collect::<Vec<_>>(),
        vec![first.clone(), ordinary.clone()]
    );
    let kept_bytes = first.payload_cbor.len() + ordinary.payload_cbor.len();
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        kept_bytes
    );
    let decision = engine
        .propose_next_head(&config.node_id, &config.world_id, 1)
        .unwrap();
    assert_eq!(decision.committed_actions, vec![first, ordinary]);
    assert!(engine.pending_consensus_actions.is_empty());
    // The proposal either owns the retained reservation or has committed and released it.
    let expected = if engine.pending.is_some() {
        kept_bytes
    } else {
        0
    };
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        expected
    );
}

#[test]
fn missing_hook_removes_governance_and_preserves_ordinary_proposal() {
    let ordinary = registration(2);
    let (config, mut engine) = queued(vec![governance(1, "first", 1), ordinary.clone()]);
    engine.discard_invalid_governance_actions(None);
    assert_eq!(
        engine
            .pending_consensus_actions
            .values()
            .cloned()
            .collect::<Vec<_>>(),
        vec![ordinary.clone()]
    );
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        ordinary.payload_cbor.len()
    );
    let decision = engine
        .propose_next_head(&config.node_id, &config.world_id, 1)
        .unwrap();
    assert_eq!(decision.committed_actions, vec![ordinary]);
}

#[test]
fn malformed_raw_admission_and_malformed_proof_never_reach_oracle() {
    let raw = NodeConsensusAction::from_payload(
        1,
        "node-governance",
        serde_cbor::to_vec(&serde_json::json!({
            "type": "ApproveGovernanceValidatorAdmission", "data": {"candidate_id": "raw-forgery"}
        }))
        .unwrap(),
    )
    .unwrap();
    let mut malformed = governance(2, "bad-proof", 1);
    let mut value: serde_json::Value = serde_cbor::from_slice(&malformed.payload_cbor).unwrap();
    value["auth"]["data"]["signatures"] = serde_json::json!([]);
    malformed = NodeConsensusAction::from_payload(
        2,
        "node-governance",
        serde_cbor::to_vec(&value).unwrap(),
    )
    .unwrap();
    let ordinary = registration(3);
    let (_, mut engine) = queued(vec![raw, malformed, ordinary.clone()]);
    let oracle = ParentOracle::default();
    engine.discard_invalid_governance_actions(Some(&oracle));
    assert!(oracle.consulted.lock().unwrap().is_empty());
    assert_eq!(
        engine
            .pending_consensus_actions
            .values()
            .cloned()
            .collect::<Vec<_>>(),
        vec![ordinary.clone()]
    );
    assert_eq!(
        engine
            .pending_consensus_action_queue_bytes
            .load(Ordering::Acquire),
        ordinary.payload_cbor.len()
    );
}
