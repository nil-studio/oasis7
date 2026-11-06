use super::*;

fn pending_world() -> World {
    let mut world = World::new();
    world
        .bind_cognition_runtime("bootstrap-epoch-world", "main", 0, None, "pending", 0)
        .unwrap();
    world
}

#[test]
fn current_pending_native_bootstrap_survives_tick_epoch_rollover() {
    let mut world = pending_world();
    for _ in 0..361 {
        world.step().unwrap();
    }
    assert_eq!(
        world
            .latest_tick_consensus_record()
            .unwrap()
            .block
            .header
            .epoch,
        1
    );
    let base = world.current_cognition_base_binding().unwrap();
    assert_eq!(base.finality_epoch, 0);
    assert!(validate_native_task_base_authority(&world, &base).is_ok());
}

#[test]
fn pending_bootstrap_rejects_changed_world_branch_epoch_hash_and_reorg() {
    let world = pending_world();
    let base = world.current_cognition_base_binding().unwrap();
    for field in 0..6 {
        let mut changed = base.clone();
        match field {
            0 => changed.world_id = "different-world".into(),
            1 => changed.branch_id = "different-branch".into(),
            2 => changed.finality_epoch = 1,
            3 => changed.finality_block_hash = Some("11".repeat(32)),
            4 => changed.reorg_epoch = 1,
            _ => changed.finality_status = "verified".into(),
        }
        assert!(
            validate_native_task_base_authority(&world, &changed).is_err(),
            "field {field}"
        );
    }
}

struct NoModules;
impl oasis7_wasm_abi::ModuleSandbox for NoModules {
    fn call(
        &mut self,
        _: &oasis7_wasm_abi::ModuleCallRequest,
    ) -> Result<oasis7_wasm_abi::ModuleOutput, oasis7_wasm_abi::ModuleCallFailure> {
        panic!("bootstrap fixture has no modules")
    }
}
#[test]
fn pending_native_bootstrap_does_not_confuse_chain_schedule_with_finality() {
    let mut world = pending_world();
    for (height, epoch) in [(1, 30), (2, 31)] {
        world
            .step_with_modules_for_committed_context(
                &mut NoModules,
                &crate::RuntimeCommittedTickContext {
                    height,
                    slot: height,
                    epoch,
                    node_block_hash: format!("node-{height}"),
                    action_root: "".into(),
                    authority_node_id: "test-authority".into(),
                    committed_at_unix_ms: 0,
                },
            )
            .unwrap();
        let latest = world.latest_tick_consensus_record().unwrap();
        assert_eq!(latest.block.header.chain_epoch, Some(epoch));
        let base = world.current_cognition_base_binding().unwrap();
        assert_eq!(base.finality_epoch, 0);
        assert!(validate_native_task_base_authority(&world, &base).is_ok());
    }
}

#[test]
fn pending_bootstrap_still_rejects_retained_reorg_contradictions() {
    for epochs in [vec![1], vec![0, 1]] {
        let mut state = crate::WorldState::default();
        for (index, epoch) in epochs.into_iter().enumerate() {
            let id = format!("intent-{index}");
            state.agent_intent_ledger.insert(id.clone(), serde_json::from_value(serde_json::json!({
                "schema_version":2,"agent_id":"agent","intent_id":id,"kind":"move","summary":"","status":"committed","source":"test","logical_time":0,"event_seq":0,"updated_at":0,"reorg_epoch":epoch
            })).unwrap());
        }
        let mut world = World::new_with_state(state);
        world
            .bind_cognition_runtime("bootstrap-epoch-world", "main", 0, None, "pending", 0)
            .unwrap();
        let base = world.current_cognition_base_binding().unwrap();
        assert!(validate_native_task_base_authority(&world, &base).is_err());
    }
}

#[test]
fn pending_bootstrap_does_not_override_real_capability_authority() {
    let mut world = pending_world();
    world.submit_action(crate::Action::RegisterAgent {
        agent_id: "unrelated-provider-agent".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world
        .install_test_provider_capability_fixture_without_cognition_balance(
            "unrelated-provider-agent",
        )
        .unwrap();
    assert!(
        !world
            .capability_revocation_state()
            .authority_records
            .is_empty()
    );
    let base = world.current_cognition_base_binding().unwrap();
    assert!(validate_native_task_base_authority(&world, &base).is_err());
}

#[test]
fn pending_bootstrap_cannot_bypass_missing_verified_retained_history() {
    let mut world = pending_world();
    world.initialize_empty_formal_history().unwrap();
    let base = world.current_cognition_base_binding().unwrap();
    assert_eq!(
        validate_native_task_base_authority(&world, &base)
            .unwrap_err()
            .code(),
        "recovery_pending"
    );
}
