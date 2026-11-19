//! Fixed original scope predicate. Borrowed state is DATA, never an admission cap.
use super::*;
use crate::Action;
use oasis7_agent_api::world_action::GeoPos;
use oasis7_client_api::safe_alternative::SafeAlternativeCommandV1;
use serde_json::{Value, json};
pub(super) struct SafeAlternativeScopeStateV1<'a> {
    pub identity: &'a crate::CapabilityAgentIdentity,
    pub runtime_world: &'a str,
    pub runtime_branch: &'a str,
    pub runtime_reorg: u64,
    pub owner_binding: &'a str,
    pub owner_public_key: Option<&'a str>,
    pub position: Option<&'a GeoPos>,
    pub forbidden: &'a [String],
}
pub(super) fn validate_scope_state_v1(
    r: &Value,
    candidate: &Value,
    c: &SafeAlternativeCommandV1,
    state: &SafeAlternativeScopeStateV1<'_>,
    now: u64,
) -> Result<(), String> {
    if state.identity.owner_binding != c.owner_binding
        || state.identity.generation != c.owner_generation
        || state.owner_binding != c.owner_binding
        || state.owner_public_key != r["owner_public_key"].as_str()
        || state.runtime_world != c.world_id
        || state.runtime_branch != c.branch_id
        || state.runtime_reorg != c.reorg_epoch
        || state
            .position
            .is_none_or(|pos| json!(pos) != r["initial_position"])
        || state.forbidden.contains(&format!(
            "{}:{}:{}",
            candidate["to"]["x_cm"], candidate["to"]["y_cm"], candidate["to"]["z_cm"]
        ))
    {
        return Err("authority_changed".into());
    }
    if now >= r["valid_until_tick"].as_u64().unwrap_or(0) || candidate["cost_upper_bound"] != 0 {
        return Err("candidate_stale".into());
    }
    Ok(())
}
pub(super) fn matching_candidate_v1(r: &Value, now: u64) -> Result<&Value, String> {
    if r["state"] != "execution_bound"
        || r["authorization"]["state"] != "bound"
        || now >= r["confirmation"]["valid_until_tick"].as_u64().unwrap_or(0)
    {
        return Err("authorization_expired_or_revoked".into());
    }
    r["candidates"]
        .as_array()
        .and_then(|v| {
            v.iter()
                .find(|v| v["candidate_id"] == r["confirmation"]["candidate_id"])
        })
        .ok_or_else(|| "outcome_unknown".into())
}
pub(super) fn validate_matching_action_v1(
    r: &Value,
    candidate: &Value,
    envelope: &ActionEnvelope,
) -> Result<(), String> {
    let expected = json!({"MoveAgent":{"agent_id":r["agent_id"],"to":candidate["to"]}});
    match &envelope.action {
        Action::MoveAgent { agent_id, to }
            if json!(agent_id) == r["agent_id"] && json!(to) == candidate["to"] =>
        {
            Ok(())
        }
        _ => Err(format!("identity_conflict:{expected}")),
    }
}
