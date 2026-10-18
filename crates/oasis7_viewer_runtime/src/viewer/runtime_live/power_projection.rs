use crate::runtime::{M1_POWER_STORAGE_CAPACITY, M1_STORAGE_POWER_MODULE_ID, WorldState};
use oasis7_simulator::{AgentPowerStatus, PowerConfig};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
struct RuntimeStoragePowerState {
    #[serde(default)]
    agents: BTreeMap<String, RuntimeStoragePowerAgentState>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeStoragePowerAgentState {
    level: i64,
}

/// Project the optional, persisted M1 storage-power reducer state into the
/// simulator compatibility model. The reducer is the only source of truth
/// here: resource balances are intentionally not used as a power fallback.
pub(super) fn runtime_storage_power_statuses(
    state: &WorldState,
) -> BTreeMap<String, AgentPowerStatus> {
    let mut state_keys = state
        .module_instances
        .iter()
        .filter(|(_, instance)| instance.active && instance.module_id == M1_STORAGE_POWER_MODULE_ID)
        .map(|(instance_id, _)| instance_id.as_str())
        .collect::<Vec<_>>();
    if state_keys.is_empty() && state.module_states.contains_key(M1_STORAGE_POWER_MODULE_ID) {
        state_keys.push(M1_STORAGE_POWER_MODULE_ID);
    }
    state_keys.sort_unstable();

    let power_config = PowerConfig::default();
    let mut statuses = BTreeMap::new();
    for state_key in state_keys {
        let Some(bytes) = state.module_states.get(state_key) else {
            continue;
        };
        let Ok(power_state) = serde_cbor::from_slice::<RuntimeStoragePowerState>(bytes) else {
            continue;
        };
        for (agent_id, agent_state) in power_state.agents {
            // A malformed or future-version reducer state must not fabricate
            // a shutdown/over-capacity status in the compatibility snapshot.
            if !(0..=M1_POWER_STORAGE_CAPACITY).contains(&agent_state.level) {
                continue;
            }
            let mut status = AgentPowerStatus::new(M1_POWER_STORAGE_CAPACITY, agent_state.level);
            status.update_state(&power_config);
            statuses.entry(agent_id).or_insert(status);
        }
    }
    statuses
}
