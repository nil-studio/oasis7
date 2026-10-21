//! Pure receipt identities shared by legacy clients and Runtime.
use crate::legacy_world_service::*;
use crate::world_service::private_goal::verify_owner_signature_strict;
use crate::world_service::*;
use oasis7_agent_api::game::*;
pub fn create_control(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<GameTaskControlV1, String> {
    let WorldServicePayloadV1::GameTaskCreate(signed) = &request.signed_payload else {
        return Err("not a Game task create".into());
    };
    let value = &signed.request;
    Ok(GameTaskControlV1 {
        schema_revision: 1,
        task_id: format!(
            "game-task-{}",
            request_digest("game-task-id", &request.correlation.key)?
        ),
        world_id: value.world_id.clone(),
        genesis_digest: value.genesis_digest.clone(),
        branch_id: value.branch_id.clone(),
        reorg_epoch: value.reorg_epoch,
        agent_id: value.agent_id.clone(),
        owner_binding: value.owner_binding.clone(),
        owner_generation: value.owner_generation,
        owner_public_key: signed.subject_public_key.clone(),
        task_revision: DecimalU64(1),
        task_status: GameTaskStatusV1::Suspended,
        latest_goal_revision: DecimalU64(0),
        active_goal_revision: DecimalU64(0),
        active_goal_reference: None,
        executor_epoch: DecimalU64(0),
        executor_id: None,
        executor_binding: None,
        created_operation_id: value.client_operation_id.clone(),
    })
}

/// Public metadata only. No local private-body availability predicate.
pub fn verify_publish(signed: &SignedReadRequest<GameGoalPublishV1>) -> Result<(), String> {
    signed.request.validate()?;
    if signed.subject_public_key != signed.request.owner_public_key {
        return Err("goal publish signer differs from owner".into());
    }
    verify_owner_signature_strict(
        GAME_GOAL_PUBLISH_DOMAIN,
        &signed.request,
        &signed.subject_public_key,
        &signed.signature_hex,
    )
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
    let WorldServicePayloadV1::GameGoalPublish(signed) = &request.signed_payload else {
        return Err("not a goal publication".into());
    };
    verify_publish(signed)?;
    if height == 0 {
        return Err("goal activation height must be committed".into());
    }
    let r = &signed.request;
    Ok(PublishReceipt {
        task_id: r.task_id.clone(),
        task_revision: DecimalU64(
            r.expected_task_revision
                .0
                .checked_add(1)
                .ok_or("task revision overflow")?,
        ),
        goal_reference: GameGoalReferenceV1 {
            goal_revision: r.goal_revision,
            goal_ref: r.goal_ref.clone(),
            prepare_operation_id: r.prepare_operation_id.clone(),
            publish_operation_id: r.client_operation_id.clone(),
            activation_committed_height: DecimalU64(height),
        },
    })
}
pub fn validate_result(result: &CanonicalIntentResultV1) -> Result<(), String> {
    if let WorldServicePayloadV1::GameStarterClaim(signed) = &result.request.signed_payload {
        if result.rejected.is_some() {
            if !result.receipt.is_null() {
                return Err("rejected starter action has success receipt".into());
            }
        } else {
            let claim = result
                .receipt
                .get("claim")
                .ok_or("starter result missing actual claim")?;
            if claim.get("agent_id").and_then(serde_json::Value::as_str)
                != Some(signed.request.agent_id.as_str())
                || claim.get("player_id").and_then(serde_json::Value::as_str)
                    != Some(signed.request.owner_binding.as_str())
                || claim.get("public_key").and_then(serde_json::Value::as_str)
                    != Some(signed.subject_public_key.as_str())
                || claim
                    .get("amount")
                    .and_then(serde_json::Value::as_u64)
                    .is_none_or(|amount| amount == 0)
            {
                return Err("starter result scope invalid".into());
            }
        }
    }
    if let WorldServicePayloadV1::GameParticipantRegister { owner, .. } =
        &result.request.signed_payload
    {
        if result.rejected.is_some() {
            if !result.receipt.is_null() {
                return Err("rejected participant operation has success receipt".into());
            }
        } else {
            let receipt: oasis7_agent_api::game::GameParticipantReceiptV1 =
                serde_json::from_value(result.receipt.clone()).map_err(|e| e.to_string())?;
            let r = &owner.request;
            let account = crate::world_service::request_digest(
                "participant-account-world",
                &(
                    &r.identity_namespace.service_id,
                    &r.identity_namespace.environment_id,
                    &r.account_id,
                    &r.world_id,
                    &r.genesis_digest,
                ),
            )?;
            let agent = format!(
                "participant:{}",
                account.strip_prefix("blake3:").unwrap_or(&account)
            );
            if receipt.api_rev != 1
                || !receipt
                    .identity_namespace
                    .same_authority(&r.identity_namespace)
                || receipt.account_id != r.account_id
                || receipt.world_id != r.world_id
                || receipt.genesis_digest != r.genesis_digest
                || receipt.owner_public_key != r.owner_public_key
                || receipt.owner_generation.0 == 0
                || receipt.owner_binding.is_empty()
                || receipt.agent_id.is_empty()
                || (!receipt.imported
                    && (receipt.owner_generation.0 != 1
                        || receipt.owner_binding != account
                        || receipt.agent_id != agent))
                || receipt.client_operation_id.is_empty()
                || receipt.business_digest.is_empty()
            {
                return Err("participant receipt scope invalid".into());
            }
        }
    }
    let expected = match &result.request.signed_payload {
        WorldServicePayloadV1::GameTaskCreate(_) => Some(
            serde_json::to_value(create_control(&result.request)?).map_err(|e| e.to_string())?,
        ),
        WorldServicePayloadV1::GameExecutorControl(_) => Some(
            serde_json::to_value(executor_receipt(&result.request)?).map_err(|e| e.to_string())?,
        ),
        WorldServicePayloadV1::GameGoalPublish(_) => Some(
            serde_json::to_value(publish_receipt(&result.request, result.committed_height)?)
                .map_err(|e| e.to_string())?,
        ),
        _ => None,
    };
    if let Some(expected) = expected {
        if result.rejected.is_some() {
            if !result.receipt.is_null() {
                return Err("rejected task operation contains success receipt".into());
            }
        } else if result.receipt != expected {
            return Err("invalid canonical task operation receipt".into());
        }
    }
    Ok(())
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
    signed.request.validate()?;
    if let GameExecutorOperationV1::Bind {
        executor_public_key,
        ..
    } = &signed.request.operation
    {
        strong_key(executor_public_key)?;
    }
    if signed.subject_public_key != signed.request.fence.owner_public_key {
        return Err("executor control requires owner signature".into());
    }
    verify_owner_signature_strict(
        GAME_EXECUTOR_CONTROL_DOMAIN,
        &signed.request,
        &signed.subject_public_key,
        &signed.signature_hex,
    )
}
pub fn strong_key(key: &str) -> Result<(), String> {
    let bytes: [u8; 32] = hex::decode(key)
        .map_err(|_| "invalid executor key")?
        .try_into()
        .map_err(|_| "invalid executor key")?;
    let key =
        ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid executor key")?;
    if key.is_weak() {
        return Err("weak executor key".into());
    }
    Ok(())
}
pub fn executor_receipt(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<ExecutorReceipt, String> {
    let WorldServicePayloadV1::GameExecutorControl(signed) = &request.signed_payload else {
        return Err("not executor control".into());
    };
    verify_executor_control(signed)?;
    let r = &signed.request;
    let epoch = DecimalU64(
        r.fence
            .executor_epoch
            .0
            .checked_add(1)
            .ok_or("executor epoch overflow")?,
    );
    let binding = match &r.operation {
        GameExecutorOperationV1::Bind {
            executor_id,
            executor_public_key,
            delegation_generation,
        } => {
            strong_key(executor_public_key)?;
            Some(GameExecutorBindingV1 {
                executor_id: executor_id.clone(),
                executor_public_key: executor_public_key.clone(),
                delegation_generation: *delegation_generation,
                executor_epoch: epoch,
            })
        }
        GameExecutorOperationV1::Stop => None,
    };
    Ok(ExecutorReceipt {
        task_id: r.fence.task_id.clone(),
        task_revision: DecimalU64(
            r.fence
                .task_revision
                .0
                .checked_add(1)
                .ok_or("task revision overflow")?,
        ),
        executor_epoch: epoch,
        task_status: if binding.is_some() {
            GameTaskStatusV1::Active
        } else {
            GameTaskStatusV1::Suspended
        },
        executor_binding: binding,
    })
}
