//! Borrowed fixed OutcomeRecorded lookup for the original World wrapper and
//! staged authorization projection. DATA only; no publication authority.
use crate::{CausedBy, WorldError, WorldEvent};
use serde_json::Value;

pub(super) fn project_outcome_from_originals_v1(
    record: Option<&Value>,
    action_id: u64,
    event_id: u64,
    events: &[WorldEvent],
) -> Result<Value, WorldError> {
    // Preserve the original ordering, including its clone before the action-ID
    // check and its first journal match. No staged whole World is needed.
    let record = record
        .cloned()
        .ok_or_else(|| super::safe_alternative::error("outcome_unknown"))?;
    if record["authorization"]["action_id"].as_u64() != Some(action_id) {
        return Err(super::safe_alternative::error("identity_conflict"));
    }
    let event = events
        .iter()
        .find(|e| {
            e.id == event_id && matches!(e.caused_by,Some(CausedBy::Action(id)) if id==action_id)
        })
        .ok_or_else(|| super::safe_alternative::error("outcome_unknown"))?;
    super::incremental_safe_alternative_outcome::project_outcome_record_v1(record, action_id, event)
}
