//! Confidential owner current-goal subset, not the complete TaskSnapshot.
use super::authority::request_digest;
use super::private_goal::PreparedGoalReceiptV1;
use super::private_goal::SignedGoalPrepareV1;
use super::private_goal::{StrictOwnerSignatureScope, verify_owner_signature_with_scope};
use super::*;
use oasis7_agent_api::game::{GameGoalPublishV1, GameTaskControlV1};
use serde::{Deserialize, Serialize};
pub const GOAL_PREPARE_PATH: &str = "/v1/world/private-goal/prepare";
pub const GOAL_CURRENT_PATH: &str = "/v1/world/private-goal/current";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentGoalRequest {
    pub schema_revision: u32,
    pub world: WorldIdentity,
    pub agent_id: String,
    pub task_id: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentGoalResponse {
    pub schema_revision: u32,
    pub control: GameTaskControlV1,
    pub read_commit: CommitRef,
    pub activation_commit: Option<CommitRef>,
    pub active_goal: Option<PreparedGoalReceiptV1>,
}
pub fn verify_current_request(
    request: &SignedReadRequest<CurrentGoalRequest>,
) -> Result<(), String> {
    verify_current_request_with_scope(request, None)
}
fn verify_current_request_with_scope(
    request: &SignedReadRequest<CurrentGoalRequest>,
    scope: Option<&mut StrictOwnerSignatureScope>,
) -> Result<(), String> {
    let r = &request.request;
    if r.schema_revision != 1
        || [
            &r.world.world_id,
            &r.world.genesis_digest,
            &r.agent_id,
            &r.task_id,
        ]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 256)
    {
        return Err("invalid current goal request".into());
    }
    verify_owner_signature_with_scope(
        GOAL_CURRENT_PATH,
        r,
        &request.subject_public_key,
        &request.signature_hex,
        scope,
    )
}
pub fn verify_prepare_response(
    original: &SignedGoalPrepareV1,
    response: &SignedServiceResponse<PreparedGoalReceiptV1>,
    configured_service_key: &str,
) -> Result<(), String> {
    verify_prepare_response_with_scope(original, response, configured_service_key, None)
}
pub fn verify_prepare_response_with_signature_scope(
    original: &SignedGoalPrepareV1,
    response: &SignedServiceResponse<PreparedGoalReceiptV1>,
    configured_service_key: &str,
    scope: &mut StrictOwnerSignatureScope,
) -> Result<(), String> {
    verify_prepare_response_with_scope(original, response, configured_service_key, Some(scope))
}
fn verify_prepare_response_with_scope(
    original: &SignedGoalPrepareV1,
    response: &SignedServiceResponse<PreparedGoalReceiptV1>,
    configured_service_key: &str,
    mut scope: Option<&mut StrictOwnerSignatureScope>,
) -> Result<(), String> {
    verify_private_response(
        GOAL_PREPARE_PATH,
        &request_digest(GOAL_PREPARE_PATH, original)?,
        response,
        configured_service_key,
        scope.as_deref_mut(),
    )?;
    response
        .payload
        .validate_against_with_scope(original, scope)
        .map_err(|e| e.to_string())
}
pub fn publication(
    original: &SignedGoalPrepareV1,
    receipt: &PreparedGoalReceiptV1,
    publish_operation_id: String,
) -> Result<GameGoalPublishV1, String> {
    publication_with_scope(original, receipt, publish_operation_id, None)
}
fn publication_with_scope(
    original: &SignedGoalPrepareV1,
    receipt: &PreparedGoalReceiptV1,
    publish_operation_id: String,
    scope: Option<&mut StrictOwnerSignatureScope>,
) -> Result<GameGoalPublishV1, String> {
    receipt
        .validate_against_with_scope(original, scope)
        .map_err(|e| e.to_string())?;
    let r = &original.request;
    let publish = GameGoalPublishV1 {
        api_rev: r.api_rev,
        world_id: r.world_id.clone(),
        genesis_digest: r.genesis_digest.clone(),
        task_id: r.task_id.clone(),
        agent_id: r.agent_id.clone(),
        owner_public_key: r.owner_public_key.clone(),
        owner_binding: r.owner_binding.clone(),
        owner_generation: r.owner_generation,
        branch_id: r.branch_id.clone(),
        reorg_epoch: r.reorg_epoch,
        expected_task_revision: r.expected_task_revision,
        expected_latest_goal_revision: r.expected_latest_goal_revision,
        goal_revision: r.goal_revision,
        client_operation_id: publish_operation_id,
        prepare_operation_id: r.client_operation_id.clone(),
        goal_ref: receipt.goal_ref.clone(),
    };
    publish.validate()?;
    Ok(publish)
}
pub fn verify_current_response(
    original: &SignedReadRequest<CurrentGoalRequest>,
    response: &SignedServiceResponse<CurrentGoalResponse>,
    configured_service_key: &str,
) -> Result<(), String> {
    verify_current_response_with_scope(original, response, configured_service_key, None)
}
pub fn verify_current_response_with_signature_scope(
    original: &SignedReadRequest<CurrentGoalRequest>,
    response: &SignedServiceResponse<CurrentGoalResponse>,
    configured_service_key: &str,
    scope: &mut StrictOwnerSignatureScope,
) -> Result<(), String> {
    verify_current_response_with_scope(original, response, configured_service_key, Some(scope))
}
fn verify_current_response_with_scope(
    original: &SignedReadRequest<CurrentGoalRequest>,
    response: &SignedServiceResponse<CurrentGoalResponse>,
    configured_service_key: &str,
    mut scope: Option<&mut StrictOwnerSignatureScope>,
) -> Result<(), String> {
    verify_current_request_with_scope(original, scope.as_deref_mut())?;
    verify_private_response(
        GOAL_CURRENT_PATH,
        &request_digest(GOAL_CURRENT_PATH, original)?,
        response,
        configured_service_key,
        scope.as_deref_mut(),
    )?;
    let r = &original.request;
    let p = &response.payload;
    let c = &p.control;
    p.read_commit.validate().map_err(|e| e.to_string())?;
    if p.schema_revision != 1
        || c.owner_generation.0 == 0
        || c.task_revision.0 == 0
        || c.owner_binding.trim().is_empty()
        || c.owner_binding.len() > 256
        || p.read_commit.binding.provider_world_id != c.world_id
        || c.schema_revision != 1
        || c.task_id != r.task_id
        || c.agent_id != r.agent_id
        || c.world_id != r.world.world_id
        || c.genesis_digest != r.world.genesis_digest
        || c.owner_public_key != original.subject_public_key
        || p.read_commit.world != r.world
        || p.read_commit.binding.branch_id != c.branch_id
        || p.read_commit.binding.reorg_generation != c.reorg_epoch.0
    {
        return Err("current goal response authority mismatch".into());
    }
    match (
        &c.active_goal_reference,
        &p.activation_commit,
        &p.active_goal,
    ) {
        (None, None, None) if c.active_goal_revision.0 == 0 && c.latest_goal_revision.0 == 0 => {
            Ok(())
        }
        (Some(reference), Some(commit), Some(goal)) => {
            reference.validate()?;
            commit.validate().map_err(|e| e.to_string())?;
            let proof = &goal.owner_proof.request;
            let public = publication_with_scope(
                &goal.owner_proof,
                goal,
                reference.publish_operation_id.clone(),
                scope.as_deref_mut(),
            )?;
            goal.validate_publish_with_scope(&public, scope)
                .map_err(|e| e.to_string())?;
            if reference.goal_ref != goal.goal_ref
                || reference.goal_revision != proof.goal_revision
                || reference.prepare_operation_id != proof.client_operation_id
                || reference.goal_revision != c.active_goal_revision
                || commit.world != r.world
                || commit.binding.provider_world_id != p.read_commit.binding.provider_world_id
                || commit.binding.branch_id != p.read_commit.binding.branch_id
                || commit.binding.reorg_generation != p.read_commit.binding.reorg_generation
                || commit.position != reference.activation_committed_height.0
                || commit.position > p.read_commit.position
                || c.latest_goal_revision != c.active_goal_revision
                || proof
                    .expected_task_revision
                    .0
                    .checked_add(1)
                    .is_none_or(|revision| revision > c.task_revision.0)
                || proof.expected_latest_goal_revision.0.checked_add(1)
                    != Some(c.active_goal_revision.0)
                || proof.world_id != c.world_id
                || proof.genesis_digest != c.genesis_digest
                || proof.task_id != c.task_id
                || proof.agent_id != c.agent_id
                || proof.owner_public_key != c.owner_public_key
                || proof.owner_binding != c.owner_binding
                || proof.owner_generation != c.owner_generation
                || proof.branch_id != c.branch_id
                || proof.reorg_epoch != c.reorg_epoch
            {
                return Err("current goal activation mismatch".into());
            }
            Ok(())
        }
        _ => Err("current goal response incomplete".into()),
    }
}

fn verify_private_response<T: Serialize>(
    path: &str,
    digest: &str,
    response: &SignedServiceResponse<T>,
    key: &str,
    scope: Option<&mut StrictOwnerSignatureScope>,
) -> Result<(), String> {
    if response.request_digest != digest {
        return Err("private response request mismatch".into());
    }
    verify_owner_signature_with_scope(
        path,
        &(&response.request_digest, &response.payload),
        key,
        &response.signature_hex,
        scope,
    )
}
