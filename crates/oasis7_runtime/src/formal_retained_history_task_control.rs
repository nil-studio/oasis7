//! Original signed task-control scope and immutable result matching.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GameTaskControlLookupV1 {
    pub agent_id: String,
    pub owner_binding: String,
    pub owner_generation: u64,
    pub owner_public_key_digest: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub branch_id: String,
    pub reorg_epoch: u64,
}

impl GameTaskControlLookupV1 {
    pub(super) fn canonicalized(&self) -> Result<Self, RetainedHistoryReadError> {
        if [
            &self.agent_id,
            &self.owner_binding,
            &self.owner_public_key_digest,
            &self.world_id,
            &self.genesis_digest,
            &self.branch_id,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
            || self.owner_generation == 0
            || !is_digest(&self.owner_public_key_digest)
        {
            return Err(RetainedHistoryReadError::Invalid(
                "Game task retained lookup identity".into(),
            ));
        }
        Ok(self.clone())
    }
}

pub(crate) fn game_task_result_matches(
    result: &CanonicalIntentResultV1,
    lookup: &GameTaskControlLookupV1,
) -> Result<bool, RetainedHistoryReadError> {
    if result.rejected.is_some() {
        return Ok(false);
    }
    Ok(game_task_result_lookup(result)
        .map_err(RetainedHistoryReadError::Invalid)?
        .as_ref()
        == Some(lookup))
}

/// Original signed owner/history scope, independent of today's active owner.
/// The authenticated result and reducer validate the original receipt/version chain.
pub(crate) fn game_task_result_lookup(
    result: &CanonicalIntentResultV1,
) -> Result<Option<GameTaskControlLookupV1>, String> {
    use crate::service_admission::WorldServicePayloadV1;
    let (agent, owner, generation, key, world, genesis, branch, epoch) =
        match &result.request.signed_payload {
            WorldServicePayloadV1::GameTaskCreate(s) => {
                let r = &s.request;
                (
                    &r.agent_id,
                    &r.owner_binding,
                    r.owner_generation.0,
                    &s.subject_public_key,
                    &r.world_id,
                    &r.genesis_digest,
                    &r.branch_id,
                    r.reorg_epoch.0,
                )
            }
            WorldServicePayloadV1::GameGoalPublish(s) => {
                let r = &s.request;
                (
                    &r.agent_id,
                    &r.owner_binding,
                    r.owner_generation.0,
                    &s.subject_public_key,
                    &r.world_id,
                    &r.genesis_digest,
                    &r.branch_id,
                    r.reorg_epoch.0,
                )
            }
            WorldServicePayloadV1::GameExecutorControl(s) => {
                let r = &s.request.fence;
                (
                    &r.agent_id,
                    &r.owner_binding,
                    r.owner_generation.0,
                    &s.subject_public_key,
                    &r.world_id,
                    &r.genesis_digest,
                    &r.branch_id,
                    r.reorg_epoch.0,
                )
            }
            _ => return Ok(None),
        };
    Ok(Some(GameTaskControlLookupV1 {
        agent_id: agent.clone(),
        owner_binding: owner.clone(),
        owner_generation: generation,
        owner_public_key_digest: digest_text("owner-public-key", key)?,
        world_id: world.clone(),
        genesis_digest: genesis.clone(),
        branch_id: branch.clone(),
        reorg_epoch: epoch,
    }))
}
