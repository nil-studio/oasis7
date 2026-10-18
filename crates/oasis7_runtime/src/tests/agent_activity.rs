use super::super::*;
use super::pos;
use crate::agent_state::AgentState;
use oasis7_wasm_abi::FactoryModuleSpec;

const AGENT_ID: &str = "agent-activity";
const FACTORY_ID: &str = "factory-activity";
const RECIPE_ID: &str = "recipe-activity";

fn recipe_fixture() -> WorldState {
    let mut state = WorldState::default();
    state.agents.insert(
        AGENT_ID.to_string(),
        AgentCell::new(AgentState::new(AGENT_ID, pos(0, 0)), 0),
    );
    state.factories.insert(
        FACTORY_ID.to_string(),
        FactoryState {
            factory_id: FACTORY_ID.to_string(),
            site_id: "site-activity".to_string(),
            builder_agent_id: AGENT_ID.to_string(),
            spec: FactoryModuleSpec {
                factory_id: FACTORY_ID.to_string(),
                display_name: "Activity Factory".to_string(),
                tier: 1,
                tags: Vec::new(),
                build_cost: Vec::new(),
                build_time_ticks: 1,
                base_power_draw: 0,
                recipe_slots: 1,
                throughput_bps: 10_000,
                maintenance_per_tick: 0,
            },
            input_ledger: MaterialLedgerId::world(),
            output_ledger: MaterialLedgerId::world(),
            durability_ppm: 1_000_000,
            production: FactoryProductionState::default(),
            location_anchor_revision: None,
            site_authority_revision: None,
            site_location_id: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 0,
        },
    );
    state
}

fn recipe_started(job_id: ActionId, now: WorldTime) -> DomainEvent {
    DomainEvent::RecipeStarted {
        committed_recipe_origin: None,
        job_id,
        requester_agent_id: AGENT_ID.to_string(),
        factory_id: FACTORY_ID.to_string(),
        recipe_id: RECIPE_ID.to_string(),
        accepted_batches: 1,
        consume: Vec::new(),
        produce: Vec::new(),
        byproducts: Vec::new(),
        power_required: 0,
        power_owner_agent_id: Some(AGENT_ID.to_string()),
        duration_ticks: 1,
        consume_ledger: MaterialLedgerId::world(),
        output_ledger: MaterialLedgerId::world(),
        bottleneck_tags: Vec::new(),
        market_quotes: Vec::new(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
        ready_at: now + 1,
    }
}

fn recipe_completed(job_id: ActionId) -> DomainEvent {
    DomainEvent::RecipeCompleted {
        committed_recipe_origin: None,
        job_id,
        requester_agent_id: AGENT_ID.to_string(),
        factory_id: FACTORY_ID.to_string(),
        recipe_id: RECIPE_ID.to_string(),
        accepted_batches: 1,
        produce: Vec::new(),
        byproducts: Vec::new(),
        output_ledger: MaterialLedgerId::world(),
        bottleneck_tags: Vec::new(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    }
}

#[test]
fn agent_cell_activity_defaults_to_missing_for_legacy_json() {
    let cell = AgentCell::new(AgentState::new("agent-legacy", pos(0, 0)), 7);
    assert!(cell.activity.is_none());

    let mut value = serde_json::to_value(&cell).expect("encode agent cell");
    value
        .as_object_mut()
        .expect("agent cell object")
        .remove("activity");
    let restored: AgentCell = serde_json::from_value(value).expect("decode legacy agent cell");

    assert!(restored.activity.is_none());
}

#[test]
fn agent_registered_initializes_idle_activity_at_event_time() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    let event = world.journal().events.last().expect("registration event");
    let cell = world
        .state()
        .agents
        .get("agent-1")
        .expect("registered cell");
    let activity: &AgentActivityV1 = cell.activity.as_ref().expect("registered activity");
    assert_eq!(activity.status, AgentActivityStatus::Idle);
    assert_eq!(activity.updated_at, event.time);
}

#[test]
fn snapshot_roundtrip_preserves_agent_activity() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    let snapshot = world.snapshot();
    let restored = Snapshot::from_json(&snapshot.to_json().expect("encode snapshot"))
        .expect("decode snapshot");

    assert_eq!(
        restored.state.agents["agent-1"].activity,
        world.state().agents["agent-1"].activity
    );
}

#[test]
fn missing_activity_is_not_synthesized_as_idle() {
    let cell = AgentCell::new(AgentState::new("agent-missing", pos(0, 0)), 11);
    assert_eq!(cell.activity, None);

    let encoded = serde_json::to_string(&cell).expect("encode missing activity");
    let restored: AgentCell = serde_json::from_str(&encoded).expect("decode missing activity");
    assert_eq!(restored.activity, None);
}

#[test]
fn recipe_started_sets_executing_activity_with_operation_identity() {
    let mut state = recipe_fixture();
    let event = recipe_started(17, 7);
    state.apply_domain_event(&event, 7).expect("start recipe");

    let activity: &AgentActivityV1 = state.agents[AGENT_ID]
        .activity
        .as_ref()
        .expect("recipe activity");
    assert_eq!(activity.status, AgentActivityStatus::Executing);
    assert_eq!(activity.operation_kind.as_deref(), Some("recipe"));
    assert_eq!(activity.operation_id, Some(17));
    assert_eq!(activity.target_id.as_deref(), Some(FACTORY_ID));
    assert_eq!(activity.updated_at, 7);
}

#[test]
fn factory_production_blocked_sets_blocked_activity_reason() {
    let mut state = recipe_fixture();
    state
        .apply_domain_event(&recipe_started(18, 7), 7)
        .expect("start recipe");
    let event = DomainEvent::FactoryProductionBlocked {
        action_id: 18,
        requester_agent_id: AGENT_ID.to_string(),
        factory_id: FACTORY_ID.to_string(),
        recipe_id: RECIPE_ID.to_string(),
        blocker_kind: "power_low".to_string(),
        blocker_detail: "insufficient electricity".to_string(),
    };
    state
        .apply_domain_event(&event, 8)
        .expect("block production");

    let activity = state.agents[AGENT_ID]
        .activity
        .as_ref()
        .expect("blocked activity");
    assert_eq!(activity.status, AgentActivityStatus::Blocked);
    assert_eq!(activity.operation_kind.as_deref(), Some("recipe"));
    assert_eq!(activity.operation_id, Some(18));
    assert_eq!(activity.target_id.as_deref(), Some(FACTORY_ID));
    assert_eq!(activity.reason_code.as_deref(), Some("power_low"));
    assert_eq!(
        activity.reason_summary.as_deref(),
        Some("insufficient electricity")
    );
    assert_eq!(activity.updated_at, 8);
}

#[test]
fn recipe_completed_with_no_active_job_returns_activity_to_idle() {
    let mut state = recipe_fixture();
    state
        .apply_domain_event(&recipe_started(19, 7), 7)
        .expect("start recipe");
    state
        .apply_domain_event(&recipe_completed(19), 8)
        .expect("complete recipe");

    let activity = state.agents[AGENT_ID]
        .activity
        .as_ref()
        .expect("terminal activity");
    assert_eq!(activity.status, AgentActivityStatus::Idle);
    assert_eq!(activity.operation_kind, None);
    assert_eq!(activity.operation_id, None);
    assert_eq!(activity.target_id, None);
    assert_eq!(activity.reason_code, None);
    assert_eq!(activity.reason_summary, None);
    assert_eq!(activity.updated_at, 8);
}
