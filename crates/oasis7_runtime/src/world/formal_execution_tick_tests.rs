//! Explicit cfg(test) parent tokens exercise execution sequencing, not trust.
use super::*;
use crate::RuntimeCommittedTickContext;
use oasis7_wasm_abi::{ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox};
struct Sandbox;
impl ModuleSandbox for Sandbox {
    fn call(&mut self, _: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        Ok(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}
fn parent_world() -> World {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();
    world.step().unwrap();
    let bytes = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test_snapshot_at_tick(
        world.state.retained_history_root.clone().unwrap(),
        &bytes,
        world.state.time,
    );
    world
        .attach_verified_formal_history(parent, Arc::new(SharedMemoryNodes::default()))
        .unwrap();
    world
}
fn context(world: &World) -> RuntimeCommittedTickContext {
    RuntimeCommittedTickContext {
        height: world.state.time + 1,
        slot: world.state.time,
        epoch: 0,
        node_block_hash: "a".repeat(64),
        action_root: "fixture".into(),
        authority_node_id: world.tick_consensus_authority_source().into(),
        committed_at_unix_ms: 0,
    }
}
#[test]
fn formal_tick_qualified_gate_allows_exact_original_and_rejects_rewrite_atomically() {
    let mut world = parent_world();
    let original = world.latest_tick_consensus_record().unwrap().clone();
    let before = serde_cbor::to_vec(&world.snapshot()).unwrap();
    world
        .install_prepared_tick_consensus_record(original.clone())
        .unwrap();
    assert_eq!(before, serde_cbor::to_vec(&world.snapshot()).unwrap());
    let mut forged = original;
    forged.certificate.signatures.clear();
    assert!(
        world
            .install_prepared_tick_consensus_record(forged)
            .is_err()
    );
    assert_eq!(before, serde_cbor::to_vec(&world.snapshot()).unwrap());
    assert!(
        world
            .append_event(
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::AgentRegistered {
                        agent_id: "new".into(),
                        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0)
                    }
                ),
                None
            )
            .is_err()
    );
    assert_eq!(before, serde_cbor::to_vec(&world.snapshot()).unwrap());
}
#[test]
fn formal_tick_successor_executes_prologue_once_and_keeps_qualified_original() {
    let mut world = parent_world();
    let original = world.latest_tick_consensus_record().unwrap().clone();
    let ctx = context(&world);
    world.begin_formal_execution_tick(&ctx).unwrap();
    let begun = serde_cbor::to_vec(&world.snapshot()).unwrap();
    assert!(world.step().is_err());
    assert!(world.step_with_modules(&mut Sandbox).is_err());
    assert_eq!(begun, serde_cbor::to_vec(&world.snapshot()).unwrap());
    world.submit_action(crate::Action::RegisterAgent {
        agent_id: "successor".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world
        .step_with_modules_for_committed_context(&mut Sandbox, &ctx)
        .unwrap();
    let once = serde_cbor::to_vec(&world.snapshot()).unwrap();
    assert!(
        world
            .step_with_modules_for_committed_context(&mut Sandbox, &ctx)
            .is_err()
    );
    assert_eq!(once, serde_cbor::to_vec(&world.snapshot()).unwrap());
    world.finish_formal_execution_tick(&ctx).unwrap();
    assert_eq!(world.tick_consensus_records()[0], original);
    assert_eq!(world.state.time, ctx.height);
    assert!(world.formal_execution_tick.is_none());
    assert!(world.begin_formal_execution_tick(&ctx).is_err());
    world.verify_tick_consensus_chain().unwrap();
}
#[test]
fn formal_tick_administrative_successor_has_final_certificate_and_no_durable_phase() {
    let mut world = parent_world();
    let original = world.latest_tick_consensus_record().unwrap().clone();
    let ctx = context(&world);
    world.begin_formal_execution_tick(&ctx).unwrap();
    world
        .append_event(
            crate::WorldEventBody::Domain(crate::DomainEvent::AgentRegistered {
                agent_id: "admin".into(),
                pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
            }),
            None,
        )
        .unwrap();
    world.finish_formal_administrative_tick(&ctx).unwrap();
    assert_eq!(world.tick_consensus_records()[0], original);
    assert_eq!(
        world
            .latest_tick_consensus_record()
            .unwrap()
            .block
            .header
            .tick,
        ctx.height
    );
    assert_eq!(
        world
            .latest_tick_consensus_record()
            .unwrap()
            .block
            .header
            .chain_height,
        Some(ctx.height)
    );
    assert_eq!(
        world
            .latest_tick_consensus_record()
            .unwrap()
            .block
            .header
            .node_block_hash
            .as_deref(),
        Some(ctx.node_block_hash.as_str())
    );
    assert!(world.formal_execution_tick.is_none());
    assert!(
        !String::from_utf8(serde_json::to_vec(&world.snapshot()).unwrap())
            .unwrap()
            .contains("formal_execution_tick")
    );
    assert!(world.finish_formal_administrative_tick(&ctx).is_err());
}
