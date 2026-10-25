//! Expired Context retirement uses only a fresh public observation.
use super::{Value, encode, parse, ws};
use oasis7_agent_api::game::{
    GameTaskControlV1, GameTaskFenceV1, GameTaskStatusV1, TaskContextIdentityV1,
};
use oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1;
use oasis7_client_api::world_service::task_context::public_outcome_intent;
use oasis7_client_api::world_service_wire::TaskSchedulerIntentV1;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    context_json: String,
    view_json: String,
}
pub(super) fn retire(json: &str) -> Result<String, &'static str> {
    let input: Input = parse(json)?;
    let context: TaskContextIdentityV1 = parse(&input.context_json)?;
    context.validate().map_err(|_| "contract_mismatch")?;
    let view: ws::ReadWorldViewResponse<Value> = parse(&input.view_json)?;
    view.version
        .commit
        .validate()
        .map_err(|_| "contract_mismatch")?;
    let task: GameTaskControlV1 = serde_json::from_value(
        view.view
            .get("game_task_control")
            .ok_or("contract_mismatch")?
            .clone(),
    )
    .map_err(|_| "contract_mismatch")?;
    let fence = GameTaskFenceV1::from_control(&task);
    let binding: oasis7_agent_api::RuntimeBindingV1 = serde_json::from_value(
        view.view
            .get("runtime_binding")
            .ok_or("contract_mismatch")?
            .clone(),
    )
    .map_err(|_| "contract_mismatch")?;
    binding.validate().map_err(|_| "contract_mismatch")?;
    if fence != context.fence
        || fence.task_status != GameTaskStatusV1::Active
        || view.logical_tick <= context.valid_until_tick.0
        || view.logical_tick != binding.base_tick
        || binding.world_id != fence.world_id
        || binding.branch_id != fence.branch_id
        || binding.reorg_epoch != fence.reorg_epoch.0
        || view.version.commit.world.world_id != fence.world_id
        || view.version.commit.world.genesis_digest != fence.genesis_digest
        || view.version.commit.binding.branch_id != fence.branch_id
        || view.version.commit.binding.reorg_generation != fence.reorg_epoch.0
    {
        return Err("contract_mismatch");
    }
    let base = RuntimeCognitionBaseBindingV1 {
        world_id: binding.world_id,
        branch_id: binding.branch_id,
        finality_epoch: binding.finality_epoch,
        finality_block_hash: binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string),
        finality_status: binding.finality_status,
        base_tick: binding.base_tick,
        base_world_hash: binding.base_world_hash.to_string(),
        reorg_epoch: binding.reorg_epoch,
        runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
    };
    let outcome: TaskSchedulerIntentV1 = public_outcome_intent(&context, base, "context_expired")
        .map_err(|_| "contract_mismatch")?;
    encode(&outcome)
}
