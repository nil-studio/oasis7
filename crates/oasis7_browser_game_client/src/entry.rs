//! Owner entry codecs only. Authentication and explicit consent remain with callers.
use super::{encode, parse, ws};
use oasis7_agent_api::game::{
    DecimalU64, GameGoalPrepareV1, GameParticipantRegisterV1, GameTaskControlV1, GameTaskCreateV1,
    GameTaskFenceV1, GoalContentV1, game_goal_version_digest,
};
use oasis7_client_api::world_service::{
    participant_discovery::ParticipantOwnerControlResponseV1,
    private_goal::{PreparedGoalReceiptV1, SignedGoalPrepareV1},
};
use serde::Deserialize;

pub(super) fn validate_prepared(original: &str, receipt: &str) -> Result<(), &'static str> {
    parse::<PreparedGoalReceiptV1>(receipt)?
        .validate_against(&parse::<SignedGoalPrepareV1>(original)?)
        .map_err(|_| "contract_mismatch")
}

pub(super) fn build(kind: &str, input: &str) -> Result<String, &'static str> {
    match kind {
        "participant_agent_id" => {
            let r: GameParticipantRegisterV1 = parse(input)?;
            r.validate().map_err(|_| "contract_mismatch")?;
            let account = ws::request_digest(
                "participant-account-world",
                &(
                    &r.identity_namespace.service_id,
                    &r.identity_namespace.environment_id,
                    &r.account_id,
                    &r.world_id,
                    &r.genesis_digest,
                ),
            )
            .map_err(|_| "encoding_failed")?;
            encode(&format!(
                "participant:{}",
                account.strip_prefix("blake3:").ok_or("encoding_failed")?
            ))
        }
        "task_id" => {
            let correlation: ws::RequestCorrelation = parse(input)?;
            encode(&format!(
                "game-task-{}",
                ws::request_digest("game-task-id", &correlation.key)
                    .map_err(|_| "encoding_failed")?
            ))
        }
        "task_create" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                owner_control_json: String,
                client_operation_id: String,
            }
            let input: Input = parse(input)?;
            let control: ParticipantOwnerControlResponseV1 = parse(&input.owner_control_json)?;
            if control.schema_revision != 1 || control.view.game_task_control.is_some() {
                return Err("task_already_present");
            }
            let owner = control.view.canonical_agent_owner;
            if owner.world_id != control.world.world_id {
                return Err("contract_mismatch");
            }
            let request = GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: input.client_operation_id,
                world_id: control.world.world_id,
                genesis_digest: control.world.genesis_digest,
                branch_id: owner.canonical_authority.branch_id,
                reorg_epoch: DecimalU64(owner.reorg_epoch),
                agent_id: owner.agent_id,
                owner_binding: owner.player_id,
                owner_generation: DecimalU64(owner.canonical_authority.agent_identity_generation),
            };
            request.validate().map_err(|_| "contract_mismatch")?;
            encode(&request)
        }
        "goal_prepare" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                task_json: String,
                client_operation_id: String,
                client_nonce: String,
                content: GoalContentV1,
            }
            let input: Input = parse(input)?;
            let task: GameTaskControlV1 = parse(&input.task_json)?;
            GameTaskFenceV1::from_control(&task)
                .validate()
                .map_err(|_| "contract_mismatch")?;
            let revision = DecimalU64(
                task.latest_goal_revision
                    .0
                    .checked_add(1)
                    .ok_or("revision_overflow")?,
            );
            let request = GameGoalPrepareV1 {
                api_rev: 1,
                goal_digest: game_goal_version_digest(&task.task_id, revision, &input.content)
                    .map_err(|_| "contract_mismatch")?,
                world_id: task.world_id,
                genesis_digest: task.genesis_digest,
                task_id: task.task_id,
                agent_id: task.agent_id,
                owner_public_key: task.owner_public_key,
                owner_binding: task.owner_binding,
                owner_generation: task.owner_generation,
                branch_id: task.branch_id,
                reorg_epoch: task.reorg_epoch,
                expected_task_revision: task.task_revision,
                expected_latest_goal_revision: task.latest_goal_revision,
                goal_revision: revision,
                client_operation_id: input.client_operation_id,
                client_nonce: input.client_nonce,
                content: input.content,
            };
            request.validate().map_err(|_| "contract_mismatch")?;
            encode(&request)
        }
        "goal_publish" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                original_json: String,
                receipt_json: String,
                client_operation_id: String,
            }
            let input: Input = parse(input)?;
            let original: SignedGoalPrepareV1 = parse(&input.original_json)?;
            let receipt: PreparedGoalReceiptV1 = parse(&input.receipt_json)?;
            encode(
                &ws::private_goal_read::publication(&original, &receipt, input.client_operation_id)
                    .map_err(|_| "contract_mismatch")?,
            )
        }
        _ => Err("unsupported_contract"),
    }
}

#[cfg(test)]
mod tests;
