#![allow(improper_ctypes_definitions)]

use oasis7_wasm_sdk::{
    LifecycleStage, WasmModuleLifecycle, export_wasm_module,
    wire::{
        GeoPosCm, ModuleCallInput, ModuleEmit, ModuleOutput, decode_input, empty_output,
        encode_output, parse_json_geo_pos_cm,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const MODULE_ID: &str = "m1.power.storage";
const RULE_DECISION_EMIT_KIND: &str = "rule.decision";
const CM_PER_KM: i64 = 100_000;

type GeoPos = GeoPosCm;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct AgentPowerState {
    pos: GeoPos,
    level: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct PowerState {
    #[serde(default)]
    agents: BTreeMap<String, AgentPowerState>,
    #[serde(default)]
    last_settled_event_id: Option<u64>,
}

fn decode_power_state(state_bytes: Option<&[u8]>) -> PowerState {
    let Some(state_bytes) = state_bytes else {
        return PowerState::default();
    };
    if state_bytes.is_empty() {
        return PowerState::default();
    }
    serde_cbor::from_slice(state_bytes).unwrap_or_default()
}

fn encode_power_state(state: &PowerState) -> Option<Vec<u8>> {
    serde_cbor::to_vec(state).ok()
}

fn action_envelope(input: &ModuleCallInput) -> Option<(u64, Value)> {
    let action_bytes = input.action.as_deref()?;
    if action_bytes.is_empty() {
        return None;
    }
    let action: Value = serde_cbor::from_slice(action_bytes).ok()?;
    let action_id = action.get("id")?.as_u64()?;
    let action_payload = action.get("action")?.clone();
    Some((action_id, action_payload))
}

fn parse_geo_pos(value: &Value) -> Option<GeoPos> {
    parse_json_geo_pos_cm(value)
}

fn space_distance_cm(a: GeoPos, b: GeoPos) -> i64 {
    let dx_m = (a.x_cm - b.x_cm) as f64 / 100.0;
    let dy_m = (a.y_cm - b.y_cm) as f64 / 100.0;
    let dz_m = (a.z_cm - b.z_cm) as f64 / 100.0;
    ((dx_m * dx_m + dy_m * dy_m + dz_m * dz_m).sqrt() * 100.0)
        .round()
        .max(0.0) as i64
}

fn decode_domain_event(event_bytes: &[u8]) -> Option<(u64, String, Value)> {
    let event: Value = serde_cbor::from_slice(event_bytes).ok()?;
    if event
        .get("body")
        .and_then(|body| body.get("kind"))
        .and_then(Value::as_str)
        != Some("Domain")
    {
        return None;
    }
    let payload = event.get("body")?.get("payload")?;
    let event_type = payload.get("type")?.as_str()?.to_string();
    let data = payload.get("data")?.clone();
    Some((event.get("id")?.as_u64()?, event_type, data))
}

fn movement_cost(distance_cm: i64, per_km_cost: i64) -> i64 {
    if distance_cm <= 0 || per_km_cost <= 0 {
        return 0;
    }
    let km = (distance_cm + CM_PER_KM - 1) / CM_PER_KM;
    km.saturating_mul(per_km_cost)
}

fn radiation_harvest_per_tick(pos: GeoPos) -> i64 {
    if 1 <= 0 {
        return 0;
    }
    let axis_sum_cm = pos
        .x_cm
        .saturating_abs()
        .saturating_add(pos.y_cm.saturating_abs())
        .saturating_add(pos.z_cm.saturating_abs());
    let bonus = axis_sum_cm / 800_000.max(1);
    let bounded_bonus = bonus.clamp(0, 1.max(0));
    1_i64.saturating_add(bounded_bonus)
}

fn update_storage_state_from_event(
    state: &mut PowerState,
    event_id: u64,
    event_type: &str,
    data: &Value,
) -> bool {
    let mut changed = match event_type {
        "AgentRegistered" => {
            let Some(agent_id) = data.get("agent_id").and_then(Value::as_str) else {
                return false;
            };
            let Some(pos) = data.get("pos").and_then(parse_geo_pos) else {
                return false;
            };
            let initial_level = 6.min(12).max(0);
            match state.agents.get_mut(agent_id) {
                Some(entry) => {
                    if entry.pos != pos {
                        entry.pos = pos;
                        true
                    } else {
                        false
                    }
                }
                None => {
                    state.agents.insert(
                        agent_id.to_string(),
                        AgentPowerState {
                            pos,
                            level: initial_level,
                        },
                    );
                    true
                }
            }
        }
        "AgentMoved" => {
            if state
                .last_settled_event_id
                .is_some_and(|last| event_id <= last)
            {
                return false;
            }
            let Some(agent_id) = data.get("agent_id").and_then(Value::as_str) else {
                return false;
            };
            let Some(to) = data.get("to").and_then(parse_geo_pos) else {
                return false;
            };
            let Some(from) = data.get("from").and_then(parse_geo_pos) else {
                return false;
            };
            let Some(agent_state) = state.agents.get_mut(agent_id) else {
                return false;
            };
            // Only a fresh authoritative move from our actual position settles.
            // Journal recovery restores recorded state updates without calling us.
            if agent_state.pos != from || from == to {
                return false;
            }
            let cost = movement_cost(space_distance_cm(from, to), 3);
            assert!(
                cost <= agent_state.level,
                "movement outcome exceeds admitted battery"
            );
            agent_state.level -= cost;
            state.last_settled_event_id = Some(event_id);
            agent_state.pos = to;
            true
        }
        _ => false,
    };

    for agent_state in state.agents.values_mut() {
        if agent_state.level > 12 {
            agent_state.level = 12;
            changed = true;
        }
        if agent_state.level < 0 {
            agent_state.level = 0;
            changed = true;
        }
    }

    changed
}

fn apply_storage_harvest(state: &mut PowerState) -> bool {
    let mut changed = false;
    for agent_state in state.agents.values_mut() {
        let harvested = radiation_harvest_per_tick(agent_state.pos);
        if harvested <= 0 {
            continue;
        }
        let next_level = agent_state.level.saturating_add(harvested).min(12.max(0));
        if next_level != agent_state.level {
            agent_state.level = next_level;
            changed = true;
        }
    }
    changed
}

fn build_storage_action_output(input: &ModuleCallInput, mut state: PowerState) -> Vec<u8> {
    let changed = apply_storage_harvest(&mut state);
    let Some((action_id, action)) = action_envelope(input) else {
        return encode_output(empty_output());
    };
    let Some(action_type) = action.get("type").and_then(Value::as_str) else {
        return encode_output(empty_output());
    };
    if action_type != "MoveAgent" {
        let new_state = if changed {
            encode_power_state(&state)
        } else {
            None
        };
        return encode_output(ModuleOutput {
            new_state,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        });
    }

    let Some(agent_id) = action
        .get("data")
        .and_then(|data| data.get("agent_id"))
        .and_then(Value::as_str)
    else {
        return encode_output(empty_output());
    };
    let Some(to) = action
        .get("data")
        .and_then(|data| data.get("to"))
        .and_then(parse_geo_pos)
    else {
        return encode_output(empty_output());
    };

    let mut decision = json!({
        "action_id": action_id,
        "verdict": "allow",
        "cost": { "entries": {} },
        "notes": [],
    });

    let Some(agent_state) = state.agents.get_mut(agent_id) else {
        decision["verdict"] = json!("deny");
        decision["notes"] = json!(["agent power state missing"]);
        let new_state = if changed {
            encode_power_state(&state)
        } else {
            None
        };
        return encode_output(ModuleOutput {
            new_state,
            effects: Vec::new(),
            emits: vec![ModuleEmit {
                kind: RULE_DECISION_EMIT_KIND.to_string(),
                payload: decision,
            }],
            tick_lifecycle: None,
            output_bytes: 0,
        });
    };

    let distance_cm = space_distance_cm(agent_state.pos, to);
    let move_cost = movement_cost(distance_cm, 3);
    if move_cost > agent_state.level {
        decision["verdict"] = json!("deny");
        decision["notes"] = json!([format!(
            "storage insufficient for move: need {move_cost}, have {}",
            agent_state.level
        )]);
    }

    let new_state = if changed {
        encode_power_state(&state)
    } else {
        None
    };
    encode_output(ModuleOutput {
        new_state,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: RULE_DECISION_EMIT_KIND.to_string(),
            payload: decision,
        }],
        tick_lifecycle: None,
        output_bytes: 0,
    })
}

fn build_storage_event_output(input: &ModuleCallInput, mut state: PowerState) -> Vec<u8> {
    let Some(event_bytes) = input.event.as_deref() else {
        return encode_output(empty_output());
    };
    let Some((event_id, event_type, data)) = decode_domain_event(event_bytes) else {
        return encode_output(empty_output());
    };
    let changed = update_storage_state_from_event(&mut state, event_id, &event_type, &data);
    let new_state = if changed {
        encode_power_state(&state)
    } else {
        None
    };

    encode_output(ModuleOutput {
        new_state,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    })
}

fn build_storage_power_module_output(input: &ModuleCallInput) -> Vec<u8> {
    let state = decode_power_state(input.state.as_deref());

    if input.action.is_some() {
        return build_storage_action_output(input, state);
    }
    if input.event.is_some() {
        return build_storage_event_output(input, state);
    }
    encode_output(empty_output())
}

fn read_input_bytes(input_ptr: i32, input_len: i32) -> Vec<u8> {
    if input_ptr > 0 && input_len > 0 {
        let ptr = input_ptr as *const u8;
        let len = input_len as usize;
        // SAFETY: host guarantees valid wasm linear memory pointer/len for the call.
        return unsafe { std::slice::from_raw_parts(ptr, len).to_vec() };
    }
    Vec::new()
}

fn write_bytes_to_memory(bytes: &[u8]) -> (i32, i32) {
    let len = i32::try_from(bytes.len()).unwrap_or(0);
    if len <= 0 {
        return (0, 0);
    }
    let ptr = oasis7_wasm_sdk::default_alloc(len);
    if ptr <= 0 {
        return (0, 0);
    }
    // SAFETY: alloc returns a writable wasm linear memory region with at least len bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, len as usize);
    }
    (ptr, len)
}

fn reduce_impl(input_ptr: i32, input_len: i32) -> (i32, i32) {
    let input = read_input_bytes(input_ptr, input_len);
    let Ok(mut decoded) = decode_input(&input) else {
        return write_bytes_to_memory(&encode_output(empty_output()));
    };
    decoded.ctx.module_id = MODULE_ID.to_string();
    let output = build_storage_power_module_output(&decoded);
    write_bytes_to_memory(&output)
}

#[derive(Default)]
struct BuiltinWasmModule;

impl WasmModuleLifecycle for BuiltinWasmModule {
    fn module_id(&self) -> &'static str {
        MODULE_ID
    }

    fn alloc(&mut self, len: i32) -> i32 {
        oasis7_wasm_sdk::default_alloc(len)
    }

    fn on_init(&mut self, _stage: LifecycleStage) {}

    fn on_teardown(&mut self, _stage: LifecycleStage) {}

    fn on_reduce(&mut self, input_ptr: i32, input_len: i32) -> (i32, i32) {
        reduce_impl(input_ptr, input_len)
    }

    fn on_call(&mut self, input_ptr: i32, input_len: i32) -> (i32, i32) {
        reduce_impl(input_ptr, input_len)
    }
}

export_wasm_module!(BuiltinWasmModule);

#[cfg(test)]
mod settlement_tests {
    use super::*;

    fn position(x_cm: i64) -> GeoPos {
        GeoPos {
            x_cm,
            y_cm: 0,
            z_cm: 0,
        }
    }

    fn state() -> PowerState {
        PowerState {
            agents: BTreeMap::from([(
                "a".into(),
                AgentPowerState {
                    pos: position(0),
                    level: 6,
                },
            )]),
            last_settled_event_id: None,
        }
    }

    #[test]
    fn preaction_keeps_actual_position_and_only_harvests() {
        let input: ModuleCallInput = serde_json::from_value(json!({
            "ctx": { "module_id": MODULE_ID, "time": 1 },
            "action": serde_cbor::to_vec(&json!({ "id": 1, "action": { "type": "MoveAgent", "data": { "agent_id": "a", "to": position(100_000) } } })).unwrap()
        })).unwrap();
        let output: Value =
            serde_cbor::from_slice(&build_storage_action_output(&input, state())).unwrap();
        let bytes: Vec<u8> = serde_json::from_value(output["new_state"].clone()).unwrap();
        let after = decode_power_state(Some(&bytes));
        assert_eq!(after.agents["a"].pos, position(0));
        assert_eq!(after.agents["a"].level, 7);
    }

    #[test]
    fn successful_outcome_debits_once_and_duplicate_is_noop() {
        let mut state = state();
        let moved = json!({ "agent_id": "a", "from": position(0), "to": position(100_000) });
        assert!(update_storage_state_from_event(
            &mut state,
            10,
            "AgentMoved",
            &moved
        ));
        assert_eq!(state.agents["a"].level, 3);
        assert_eq!(state.agents["a"].pos, position(100_000));
        assert!(!update_storage_state_from_event(
            &mut state,
            10,
            "AgentMoved",
            &moved
        ));
        assert_eq!(state.agents["a"].level, 3);
    }
    #[test]
    fn stale_duplicate_after_return_to_origin_does_not_charge_again() {
        let mut state = state();
        state.agents.get_mut("a").unwrap().level = 12;
        let outward = json!({ "agent_id": "a", "from": position(0), "to": position(100_000) });
        let returning = json!({ "agent_id": "a", "from": position(100_000), "to": position(0) });
        assert!(update_storage_state_from_event(
            &mut state,
            10,
            "AgentMoved",
            &outward
        ));
        assert!(update_storage_state_from_event(
            &mut state,
            20,
            "AgentMoved",
            &returning
        ));
        let restored = decode_power_state(encode_power_state(&state).as_deref());
        assert_eq!(restored, state);
        assert!(!update_storage_state_from_event(
            &mut state,
            10,
            "AgentMoved",
            &outward
        ));
        assert_eq!(state.agents["a"].pos, position(0));
        assert_eq!(state.agents["a"].level, 6);
    }
}
