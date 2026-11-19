//! Pure receipt identities shared by legacy clients and Runtime.
use crate::legacy_world_service::*;
use crate::world_service::private_goal::verify_owner_signature_strict;
use crate::world_service::*;
use oasis7_agent_api::game::*;
pub fn create_control(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<GameTaskControlV1, String> {
    create_control_with_work_v1(request, &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
        .map(work::PaidGameTaskControlDataV1::into_legacy)
        .map_err(crate::world_service::legacy_service_work_error_v1)
}

/// Public metadata only. No local private-body availability predicate.
pub fn verify_publish(signed: &SignedReadRequest<GameGoalPublishV1>) -> Result<(), String> {
    verify_publish_with_work_v1(signed, &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
        .map_err(crate::world_service::legacy_service_work_error_v1)
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishReceipt {
    pub task_id: String,
    pub task_revision: DecimalU64,
    pub goal_reference: GameGoalReferenceV1,
}
pub fn publish_receipt(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    height: u64,
) -> Result<PublishReceipt, String> {
    publish_receipt_with_work_v1(
        request,
        height,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .map(work::PaidPublishReceiptDataV1::into_legacy)
    .map_err(crate::world_service::legacy_service_work_error_v1)
}
pub fn validate_result(result: &CanonicalIntentResultV1) -> Result<(), String> {
    validate_result_with_work_v1(result, &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
        .map_err(crate::world_service::legacy_service_work_error_v1)
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorReceipt {
    pub task_id: String,
    pub task_revision: DecimalU64,
    pub executor_epoch: DecimalU64,
    pub executor_binding: Option<GameExecutorBindingV1>,
    pub task_status: GameTaskStatusV1,
}
pub fn verify_executor_control(
    signed: &SignedReadRequest<GameExecutorControlV1>,
) -> Result<(), String> {
    verify_executor_control_with_work_v1(signed, &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
        .map_err(crate::world_service::legacy_service_work_error_v1)
}
pub fn strong_key(key: &str) -> Result<(), String> {
    strong_key_with_work_v1(key, &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
        .map_err(crate::world_service::legacy_service_work_error_v1)
}
pub fn executor_receipt(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<ExecutorReceipt, String> {
    executor_receipt_with_work_v1(request, &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
        .map(executor_work::PaidExecutorReceiptDataV1::into_legacy)
        .map_err(crate::world_service::legacy_service_work_error_v1)
}
#[path = "legacy_task_results_executor_work.rs"]
mod executor_work;
pub use executor_work::{
    PaidExecutorReceiptDataV1, executor_receipt_with_work_v1, strong_key_with_work_v1,
    verify_executor_control_with_work_v1,
};

#[path = "legacy_task_results_work.rs"]
mod work;
pub use work::{
    PaidGameTaskControlDataV1, PaidPublishReceiptDataV1, create_control_with_work_v1,
    publish_receipt_with_work_v1, verify_publish_with_work_v1,
};

#[path = "legacy_task_results_validate_work.rs"]
mod validate_work;
pub use validate_work::validate_result_with_work_v1;
