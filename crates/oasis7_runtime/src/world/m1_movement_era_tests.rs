use super::*;
use crate::Action;
use oasis7_agent_api::world_action::GeoPos;
use oasis7_wasm_executor::WasmExecutor;

fn position(x_cm: i64) -> GeoPos {
    GeoPos {
        x_cm,
        y_cm: 0,
        z_cm: 0,
    }
}

fn prepared_world() -> (World, WasmExecutor) {
    let mut world = World::new();
    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .unwrap();
    let mut sandbox = WasmExecutor::new(crate::tests::test_wasm_executor_config()).unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "a".into(),
        pos: position(0),
    });
    world.step_with_modules(&mut sandbox).unwrap();
    world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: position(100_000),
    });
    world.step_with_modules(&mut sandbox).unwrap();
    (world, sandbox)
}

fn assert_unsupported_era_preserves_move(era: u64, next_id: u64) {
    let (mut world, mut sandbox) = prepared_world();
    world.next_event_id_era = era;
    world.next_event_id = next_id;
    world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: position(200_000),
    });
    let before = world.snapshot();
    let result = world.step_with_modules(&mut sandbox);
    assert!(
        result.is_err(),
        "unsupported M1 outcome era must not publish a move"
    );
    assert!(format!("{:?}", result.unwrap_err()).contains("M1 storage 0.2.0 requires event era 0"));
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.state.agents["a"].state.pos, position(100_000));
    assert_eq!(world.state.module_states, before.state.module_states);
}

#[test]
fn movement_settlement_v2_rejects_existing_nonzero_event_era_without_mutation() {
    assert_unsupported_era_preserves_move(1, 1);
}

#[test]
fn movement_settlement_v2_rejects_preaction_event_era_rollover_without_mutation() {
    // Actual pre-action event allocation crosses MAX before domain publication.
    assert_unsupported_era_preserves_move(0, u64::MAX - 1);
}

#[test]
fn movement_settlement_unrelated_world_keeps_event_id_rollover() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "a".into(),
        pos: position(0),
    });
    world.step().unwrap();
    world.next_event_id = u64::MAX;
    world.submit_action(Action::MoveAgent {
        agent_id: "a".into(),
        to: position(100_000),
    });
    world.step().unwrap();
    assert_eq!(world.state.agents["a"].state.pos, position(100_000));
    assert_eq!(world.next_event_id_era, 1);
}
