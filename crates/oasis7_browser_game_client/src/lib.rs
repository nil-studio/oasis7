//! Bounded protocol-only bridge: no credentials, network, storage or world execution.
use oasis7_client_api::world_service::{
    self as ws,
    task_context::{
        CapturedTaskContextV1, GoalExecutionSnapshotV1, PreparedTaskContextV1,
        TASK_CONTEXT_PREPARE_PATH, TASK_CONTEXT_READ_PATH, TASK_SNAPSHOT_PATH,
        TaskGoalSnapshotRequestV1,
    },
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use wasm_bindgen::prelude::*;

mod build;
mod entry;
mod participant;
mod retire;
pub use build::world_build;

const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
fn parse<T: DeserializeOwned>(input: &str) -> Result<T, &'static str> {
    if input.len() > MAX_JSON_BYTES {
        return Err("input_limit");
    }
    serde_json::from_str(input).map_err(|_| "invalid_json")
}
fn encode<T: Serialize>(value: &T) -> Result<String, &'static str> {
    let output = serde_json::to_string(value).map_err(|_| "encoding_failed")?;
    if output.len() > MAX_JSON_BYTES {
        return Err("output_limit");
    }
    Ok(output)
}
fn domain(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > 256 || !value.is_ascii() {
        return Err("invalid_domain");
    }
    Ok(())
}
fn signing(domain_value: &str, json: &str) -> Result<Vec<u8>, &'static str> {
    domain(domain_value)?;
    ws::signing_bytes(domain_value, &parse::<Value>(json)?).map_err(|_| "encoding_failed")
}
fn digest(domain_value: &str, json: &str) -> Result<String, &'static str> {
    domain(domain_value)?;
    ws::request_digest(domain_value, &parse::<Value>(json)?).map_err(|_| "encoding_failed")
}
fn verify(
    domain_value: &str,
    request: &str,
    response: &str,
    key: &str,
) -> Result<String, &'static str> {
    fn typed<T: Serialize + DeserializeOwned>(
        domain: &str,
        expected: &str,
        json: &str,
        key: &str,
    ) -> Result<String, &'static str> {
        let response: ws::SignedServiceResponse<T> = parse(json)?;
        ws::verify_service_response(domain, expected, &response, key)
            .map_err(|_| "response_verification_failed")?;
        encode(&response.payload)
    }
    let expected = digest(domain_value, request)?;
    if key.len() != 64 {
        return Err("invalid_service_key");
    }
    // Private routes sign their typed CBOR, whose map keys may not be JSON strings.
    // Match the gateway's normalized/typed profile split exactly.
    match domain_value {
        ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH => {
            typed::<ws::participant_discovery::ParticipantOwnerControlResponseV1>(
                domain_value,
                &expected,
                response,
                key,
            )
        }
        ws::private_goal_read::GOAL_PREPARE_PATH => {
            let original: ws::private_goal::SignedGoalPrepareV1 = parse(request)?;
            let response: ws::SignedServiceResponse<ws::private_goal::PreparedGoalReceiptV1> =
                parse(response)?;
            ws::private_goal_read::verify_prepare_response(&original, &response, key)
                .map_err(|_| "response_verification_failed")?;
            encode(&response.payload)
        }
        ws::private_goal_read::GOAL_CURRENT_PATH => typed::<
            ws::private_goal_read::CurrentGoalResponse,
        >(
            domain_value, &expected, response, key
        ),
        TASK_CONTEXT_PREPARE_PATH => {
            typed::<PreparedTaskContextV1>(domain_value, &expected, response, key)
        }
        TASK_CONTEXT_READ_PATH => {
            typed::<CapturedTaskContextV1>(domain_value, &expected, response, key)
        }
        TASK_SNAPSHOT_PATH => {
            typed::<GoalExecutionSnapshotV1>(domain_value, &expected, response, key)
        }
        ws::task_recipe_quote::TASK_RECIPE_QUOTE_PATH => typed::<
            ws::task_recipe_quote::TaskRecipeQuoteV1,
        >(
            domain_value, &expected, response, key
        ),
        ws::task_factory_quote::TASK_FACTORY_QUOTE_PATH => typed::<
            ws::task_factory_quote::TaskFactoryQuoteV1,
        >(
            domain_value, &expected, response, key
        ),
        _ => typed::<Value>(domain_value, &expected, response, key),
    }
}
fn verify_gateway(
    domain: &str,
    request: &str,
    reply: &str,
    key: &str,
) -> Result<String, &'static str> {
    let reply: Value = parse(reply)?;
    if reply.get("ok") != Some(&Value::Bool(true)) || reply.get("upstream_read_error").is_some() {
        return Err("gateway_rejected");
    }
    let response = reply
        .get("world_response")
        .ok_or("gateway_response_missing")?;
    verify(domain, request, &encode(response)?, key)
}
/// # Errors
/// Returns a fixed error classification for invalid, oversized, unsupported, or
/// unverified protocol data.
#[wasm_bindgen]
pub fn world_verify_gateway_response(
    domain: &str,
    request_json: &str,
    gateway_reply_json: &str,
    service_public_key: &str,
) -> Result<String, JsValue> {
    verify_gateway(domain, request_json, gateway_reply_json, service_public_key)
        .map_err(JsValue::from_str)
}

fn validate(kind: &str, request: &str, payload: &str) -> Result<String, &'static str> {
    // Validation is separate from service authentication; callers must perform both.
    match kind {
        "participant_admission" => participant::admission(request, payload)?,
        "participant_receipt" => participant::receipt(request, payload)?,
        "goal_prepared" => entry::validate_prepared(request, payload)?,
        "describe" => parse::<ws::DescribeWorldResponse>(payload)?
            .validate(&parse(request)?)
            .map_err(|_| "contract_mismatch")?,
        "view" => parse::<ws::ReadWorldViewResponse<Value>>(payload)?
            .validate(&parse(request)?)
            .map_err(|_| "contract_mismatch")?,
        "intent" | "lookup" => parse::<ws::IntentResponse<Value>>(payload)?
            .validate(&parse(request)?)
            .map_err(|_| "contract_mismatch")?,
        "context_prepare" => parse::<PreparedTaskContextV1>(payload)?
            .validate_against(&parse(request)?)
            .map_err(|_| "contract_mismatch")?,
        "context_read" => parse::<CapturedTaskContextV1>(payload)?
            .validate_against(&parse(request)?)
            .map_err(|_| "contract_mismatch")?,
        "task_snapshot" => {
            let original: TaskGoalSnapshotRequestV1 = parse(request)?;
            let snapshot: GoalExecutionSnapshotV1 = parse(payload)?;
            snapshot.validate().map_err(|_| "contract_mismatch")?;
            if oasis7_agent_api::game::GameTaskFenceV1::from_control(&snapshot.control)
                != original.fence
                || snapshot.schema_revision != original.schema_revision
                || snapshot.authority_mode().map_err(|_| "contract_mismatch")?
                    != original.authority_mode
            {
                return Err("contract_mismatch");
            }
        }
        "recipe_quote" => parse::<ws::task_recipe_quote::TaskRecipeQuoteV1>(payload)?
            .validate_against(&parse(request)?)
            .map_err(|_| "contract_mismatch")?,
        "factory_quote" => parse::<ws::task_factory_quote::TaskFactoryQuoteV1>(payload)?
            .validate_against(&parse(request)?)
            .map_err(|_| "contract_mismatch")?,
        "owner_control" => {
            parse::<ws::participant_discovery::ParticipantOwnerControlResponseV1>(payload)?
                .validate(&parse(request)?)
                .map_err(|_| "contract_mismatch")?;
        }
        "fence" => parse::<oasis7_agent_api::game::GameTaskFenceV1>(payload)?
            .validate()
            .map_err(|_| "contract_mismatch")?,
        _ => return Err("unsupported_contract"),
    }
    encode(&parse::<Value>(payload)?)
}
/// # Errors
/// Returns a fixed error classification for invalid, oversized, unsupported, or
/// unverified protocol data.
#[wasm_bindgen]
pub fn world_signing_bytes(domain: &str, value_json: &str) -> Result<Vec<u8>, JsValue> {
    signing(domain, value_json).map_err(JsValue::from_str)
}
/// # Errors
/// Returns a fixed error classification for invalid, oversized, unsupported, or
/// unverified protocol data.
#[wasm_bindgen]
pub fn world_request_digest(domain: &str, value_json: &str) -> Result<String, JsValue> {
    digest(domain, value_json).map_err(JsValue::from_str)
}
/// # Errors
/// Returns a fixed error classification for invalid, oversized, unsupported, or
/// unverified protocol data.
#[wasm_bindgen]
pub fn world_verify_response(
    domain: &str,
    request_json: &str,
    response_json: &str,
    service_public_key: &str,
) -> Result<String, JsValue> {
    verify(domain, request_json, response_json, service_public_key).map_err(JsValue::from_str)
}
/// # Errors
/// Returns a fixed error classification for invalid, oversized, unsupported, or
/// unverified protocol data.
#[wasm_bindgen]
pub fn world_validate(
    kind: &str,
    request_json: &str,
    payload_json: &str,
) -> Result<String, JsValue> {
    validate(kind, request_json, payload_json).map_err(JsValue::from_str)
}

#[cfg(test)]
mod tests;
