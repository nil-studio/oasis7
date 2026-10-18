use super::super::*;
use crate::geometry::GeoPos;
use crate::models::AgentState;
use crate::runtime::{AgentCell, M1_STORAGE_POWER_MODULE_ID};
use oasis7_simulator::AgentPowerStatus;

#[test]
fn runtime_state_to_simulator_model_projects_authoritative_storage_power_state() {
    let mut state = crate::runtime::WorldState::default();
    for (agent_id, level) in [
        ("agent-critical", 0),
        ("agent-low", 2),
        ("agent-normal", 12),
    ] {
        let pos = GeoPos::new(level, 0, 0);
        state.agents.insert(
            agent_id.to_string(),
            AgentCell::new(AgentState::new(agent_id, pos), 0),
        );
    }
    state.module_states.insert(
        M1_STORAGE_POWER_MODULE_ID.to_string(),
        serde_cbor::to_vec(&serde_json::json!({
            "agents": {
                "agent-critical": { "level": 0 },
                "agent-low": { "level": 2 },
                "agent-normal": { "level": 12 }
            }
        }))
        .expect("encode storage power reducer state"),
    );

    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Script);
    let model = mapping::runtime_state_to_simulator_model(&state, &sidecar, None);

    assert_eq!(model.agents["agent-critical"].power.capacity, 12);
    assert_eq!(model.agents["agent-critical"].power.level, 0);
    assert!(model.agents["agent-critical"].power.is_shutdown());
    assert_eq!(model.agents["agent-low"].power.level, 2);
    assert_eq!(model.agents["agent-low"].power.level_pct(), 16);
    assert_eq!(model.agents["agent-normal"].power.level, 12);
    assert_eq!(model.agents["agent-normal"].power.level_pct(), 100);
}

#[test]
fn runtime_state_to_simulator_model_keeps_default_power_for_missing_or_invalid_state() {
    let mut state = crate::runtime::WorldState::default();
    let pos = GeoPos::new(0, 0, 0);
    state.agents.insert(
        "agent-a".to_string(),
        AgentCell::new(AgentState::new("agent-a", pos), 0),
    );
    state.module_states.insert(
        M1_STORAGE_POWER_MODULE_ID.to_string(),
        serde_cbor::to_vec(&serde_json::json!({
            "agents": { "agent-a": { "level": 999 } }
        }))
        .expect("encode invalid storage power reducer state"),
    );

    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Script);
    let model = mapping::runtime_state_to_simulator_model(&state, &sidecar, None);
    let power = &model.agents["agent-a"].power;
    assert_eq!(power, &AgentPowerStatus::default());
}
