use super::*;
use oasis7_agent_api::world_action::GeoPos;
use oasis7_client_api::safe_alternative::*;
use serde_json::Value;

fn signed(
    identity: &WorldIdentity,
    owner: &str,
    operation_id: &str,
    operation: SafeAlternativeOperationV1,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let command = SafeAlternativeCommandV1 {
        api_rev: 1,
        client_operation_id: operation_id.into(),
        world_id: identity.world_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
        branch_id: "main".into(),
        reorg_epoch: 0,
        agent_id: "agent-a".into(),
        owner_binding: "owner-a".into(),
        owner_generation: 1,
        operation,
    };
    let proof = sign_read_request(command.signing_domain(), command, owner).unwrap();
    request(identity, WorldServicePayloadV1::SafeAlternative(proof))
}
fn receipt(
    driver: &NodeRuntimeExecutionDriver,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Value {
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let result: CanonicalIntentResultV1 = serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&key]
            .clone(),
    )
    .unwrap();
    assert_eq!(result.rejected, None);
    result.receipt
}
#[test]
fn safe_alternative_canonical_driver_preview_confirmation_execution_and_retry() {
    let (dir, mut driver, identity, owner, _) =
        super::canonical_agent_chat::driver_fixture_without_provider();
    let before = driver.execution_world.state().agents["agent-a"].state.pos;
    let proposal = signed(
        &identity,
        &owner,
        "proposal",
        SafeAlternativeOperationV1::Propose {
            request_id: "unsupported-waypoints".into(),
            original: FineGrainedMoveRequestV1 {
                goal: "Visit both waypoints in order".into(),
                waypoints: vec![GeoPos::new(10, 0, 0), GeoPos::new(20, 0, 0)],
                arrival_tick: None,
            },
            valid_until_tick: 100,
            predecessor_request_id: None,
        },
    );
    commit(&mut driver, 3, proposal.clone());
    let proposed = receipt(&driver, &proposal);
    assert_eq!(proposed["state"], "awaiting_confirmation");
    assert_eq!(proposed["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(
        driver.execution_world.state().agents["agent-a"].state.pos,
        before
    );
    let candidate = &proposed["candidates"][0];
    let confirmation = signed(
        &identity,
        &owner,
        "confirmation",
        SafeAlternativeOperationV1::Confirm {
            request_id: "unsupported-waypoints".into(),
            generation: 1,
            candidate_id: candidate["candidate_id"].as_str().unwrap().into(),
            candidate_digest: candidate["candidate_digest"].as_str().unwrap().into(),
            confirmation_id: "explicit-owner-confirmation".into(),
            valid_until_tick: 90,
        },
    );
    commit(&mut driver, 4, confirmation.clone());
    let confirmed = receipt(&driver, &confirmation);
    assert_eq!(confirmed["authorization"]["state"], "available");
    assert_eq!(
        driver.execution_world.state().agents["agent-a"].state.pos,
        before
    );
    let execution = signed(
        &identity,
        &owner,
        "execution",
        SafeAlternativeOperationV1::Execute {
            request_id: "unsupported-waypoints".into(),
            authorization_id: confirmed["authorization"]["authorization_id"]
                .as_str()
                .unwrap()
                .into(),
            execution_id: "single-execution".into(),
        },
    );
    commit(&mut driver, 5, execution.clone());
    // Administrative admission binds the action without advancing simulation
    // time. The next canonical world tick consumes that original binding.
    assert_eq!(
        driver.execution_world.state().agents["agent-a"].state.pos,
        before
    );
    assert_eq!(receipt(&driver, &execution)["state"], "execution_bound");
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height: 6,
            slot: 6,
            epoch: 0,
            node_block_hash: "node-h6".into(),
            action_root: compute_consensus_action_root(&[]).unwrap(),
            committed_actions: vec![],
            committed_at_unix_ms: 6000,
        })
        .unwrap();
    assert_eq!(
        driver.execution_world.state().agents["agent-a"].state.pos,
        GeoPos::new(10, 0, 0)
    );
    let records = driver
        .execution_world
        .capability_revocation_state()
        .safe_alternatives
        .clone();
    let completed = records.values().next().unwrap();
    assert_eq!(completed["authorization"]["state"], "consumed");
    assert_eq!(completed["outcome"]["status"], "applied");
    let action_id = completed["outcome"]["action_id"].as_u64().unwrap();
    commit(&mut driver, 7, execution);
    assert_eq!(
        driver
            .execution_world
            .capability_revocation_state()
            .safe_alternatives,
        records
    );
    let applied: Vec<_> = driver
        .execution_world
        .journal()
        .events
        .iter()
        .filter(|event| {
            event.caused_by == Some(oasis7_runtime::CausedBy::Action(action_id))
                && matches!(
                    event.body,
                    oasis7_runtime::WorldEventBody::Domain(
                        oasis7_runtime::DomainEvent::AgentMoved { .. }
                    )
                )
        })
        .collect();
    assert_eq!(applied.len(), 1);
    // Snapshot recovery retains both binding and terminal receipt, independent of UI memory.
    let restored = oasis7_runtime::World::from_snapshot(
        driver.execution_world.snapshot(),
        driver.execution_world.journal().clone(),
    )
    .unwrap();
    assert_eq!(
        restored.capability_revocation_state().safe_alternatives,
        records
    );
    std::fs::remove_dir_all(dir).unwrap();
}
