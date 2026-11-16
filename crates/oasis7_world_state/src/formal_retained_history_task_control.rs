//! Original signed task-control scope and immutable result matching.
use super::*;

/// Borrowed task scope fields used by the signed-result identity comparison.
pub type GameTaskResultScopeFieldsV1<'a> = (
    &'a str,
    &'a str,
    u64,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    u64,
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskControlLookupV1 {
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
    pub fn canonicalized(&self) -> Result<Self, RetainedHistoryReadError> {
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

pub fn game_task_result_matches(
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
pub fn game_task_result_lookup(
    result: &CanonicalIntentResultV1,
) -> Result<Option<GameTaskControlLookupV1>, String> {
    let Some((agent, owner, generation, key, world, genesis, branch, epoch)) =
        game_task_result_scope_data_v1(result)
    else {
        return Ok(None);
    };
    Ok(Some(GameTaskControlLookupV1 {
        agent_id: agent.to_owned(),
        owner_binding: owner.to_owned(),
        owner_generation: generation,
        owner_public_key_digest: digest_text("owner-public-key", key)?,
        world_id: world.to_owned(),
        genesis_digest: genesis.to_owned(),
        branch_id: branch.to_owned(),
        reorg_epoch: epoch,
    }))
}

/// Borrowed original signed task scope. This is DATA; it performs no receipt,
/// Current owner or archive admission. Both original owned lookup and paid
/// consumers use this one variant dispatcher.
pub fn game_task_result_scope_data_v1(
    result: &CanonicalIntentResultV1,
) -> Option<GameTaskResultScopeFieldsV1<'_>> {
    use crate::service_admission::WorldServicePayloadV1;
    let fields: GameTaskResultScopeFieldsV1<'_> = match &result.request.signed_payload {
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
        _ => return None,
    };
    Some(fields)
}
