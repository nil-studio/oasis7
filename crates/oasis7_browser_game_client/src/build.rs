//! Typed construction keeps Runtime artifacts and correlation out of JavaScript.
use super::{
    CapturedTaskContextV1, DeserializeOwned, JsValue, PreparedTaskContextV1, Value, encode, parse,
    ws,
};
use oasis7_client_api::world_service::task_context::{
    cognition_intent, outcome_intent, public_prefix,
};
use oasis7_client_api::world_service_wire::{TaskSchedulerIntentV1, WorldServicePayloadV1};
use serde::Deserialize;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrelationInput {
    world: ws::WorldIdentity,
    #[serde(alias = "payload_json", deserialize_with = "raw_or_typed")]
    payload: WorldServicePayloadV1,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionInput {
    #[serde(alias = "captured_json", deserialize_with = "raw_or_typed")]
    captured: CapturedTaskContextV1,
    action: oasis7_agent_api::world_action::Action,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutcomeInput {
    #[serde(alias = "captured_json", deserialize_with = "raw_or_typed")]
    captured: CapturedTaskContextV1,
    reason: String,
}
fn raw_or_typed<'de, D: serde::Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<T, D::Error> {
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(raw) => parse(&raw).map_err(serde::de::Error::custom),
        other => serde_json::from_value(other).map_err(serde::de::Error::custom),
    }
}
pub(super) fn build(kind: &str, input: &str) -> Result<String, &'static str> {
    match kind {
        "retire_context" => crate::retire::retire(input),
        "replace" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Replace {
                json: String,
                path: Vec<String>,
                value_json: String,
            }
            let input: Replace = parse(input)?;
            if input.path.is_empty() || input.path.len() > 16 {
                return Err("input_limit");
            }
            let mut value: Value = parse(&input.json)?;
            let replacement: Value = parse(&input.value_json)?;
            let mut current = &mut value;
            for key in &input.path {
                current = current.get_mut(key).ok_or("field_missing")?;
            }
            *current = replacement;
            encode(&value)
        }
        "field" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Field {
                json: String,
                path: Vec<String>,
            }
            let input: Field = parse(input)?;
            if input.path.len() > 16 {
                return Err("input_limit");
            }
            let mut value: Value = parse(&input.json)?;
            for key in input.path {
                value = value.get(&key).ok_or("field_missing")?.clone();
            }
            encode(&value)
        }
        "fence" => {
            let control: oasis7_agent_api::game::GameTaskControlV1 = parse(input)?;
            let fence = oasis7_agent_api::game::GameTaskFenceV1::from_control(&control);
            fence.validate().map_err(|_| "contract_mismatch")?;
            encode(&fence)
        }
        "correlation" => {
            let input: CorrelationInput = parse(input)?;
            encode(
                &oasis7_client_api::world_service_correlation::derive_correlation(
                    input.world,
                    &input.payload,
                )
                .map_err(|_| "contract_mismatch")?,
            )
        }
        "prefix" => {
            let prepared: PreparedTaskContextV1 = parse(input)?;
            let prefix: TaskSchedulerIntentV1 =
                public_prefix(&prepared).map_err(|_| "contract_mismatch")?;
            encode(&prefix)
        }
        "action" => {
            let input: ActionInput = parse(input)?;
            encode(
                &cognition_intent(&input.captured, input.action)
                    .map_err(|_| "contract_mismatch")?,
            )
        }
        "outcome" => {
            let input: OutcomeInput = parse(input)?;
            let outcome: TaskSchedulerIntentV1 =
                outcome_intent(&input.captured, &input.reason).map_err(|_| "contract_mismatch")?;
            encode(&outcome)
        }
        _ => Err("unsupported_contract"),
    }
}
/// # Errors
/// Returns a fixed error classification when the input exceeds protocol bounds,
/// names an unsupported builder, or fails canonical contract validation.
#[wasm_bindgen]
pub fn world_build(kind: &str, value_json: &str) -> Result<String, JsValue> {
    build(kind, value_json).map_err(JsValue::from_str)
}
