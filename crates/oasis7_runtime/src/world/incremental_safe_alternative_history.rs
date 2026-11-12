//! Fixed original SafeAlternative signed-history predicate, no authority mint.
use super::*;
use crate::Action;
use crate::service_admission::SignedReadRequest;
use oasis7_client_api::safe_alternative::{
    SafeAlternativeCommandV1, SafeAlternativeOperationV1 as Op,
};
use serde_json::{Value, json};
pub(super) fn validate_safe_alternative_record_history_v1(
    key: &str,
    r: &Value,
    pending: Option<&ActionEnvelope>,
    action_ids: &mut BTreeSet<u64>,
) -> Result<(), WorldError> {
    let fail = || super::safe_alternative::error("invalid_safe_alternative_history");
    let signed: SignedReadRequest<SafeAlternativeCommandV1> =
        serde_json::from_value(r["propose_signed"].clone()).map_err(|_| fail())?;
    signed.request.validate().map_err(|_| fail())?;
    crate::service_admission::verify_read_request(signed.request.signing_domain(), &signed)
        .map_err(|_| fail())?;
    let Op::Propose { original, .. } = &signed.request.operation else {
        return Err(fail());
    };
    if super::safe_alternative::lineage_key(&signed.request).as_str() != key
        || r["binding"] != json!(signed.request)
        || r["owner_public_key"] != signed.subject_public_key
        || r["original"] != json!(original)
        || r["original_digest"] != super::safe_alternative::digest(&json!(original))
        || r["original_no_effect"] != true
        || r["generation"].as_u64().is_none_or(|g| g == 0)
        || r["agent_id"] != signed.request.agent_id
    {
        return Err(fail());
    }
    let candidates = r["candidates"].as_array().ok_or_else(fail)?;
    if candidates.len() > 8 {
        return Err(fail());
    }
    for candidate in candidates {
        let mut contents = candidate.clone();
        let claimed = contents
            .as_object_mut()
            .ok_or_else(fail)?
            .remove("candidate_digest")
            .ok_or_else(fail)?;
        let to: oasis7_agent_api::world_action::GeoPos =
            serde_json::from_value(candidate["to"].clone()).map_err(|_| fail())?;
        if claimed != super::safe_alternative::digest(&contents)
            || !original.waypoints.contains(&to)
            || candidate["scope"]["agent_id"] != signed.request.agent_id
            || candidate["scope"]["from"] != r["initial_position"]
            || candidate["scope"]["to"] != candidate["to"]
            || candidate["scope"]["maximum_effects"] != 1
            || candidate["cost_upper_bound"] != 0
            || candidate["valid_until_tick"] != r["valid_until_tick"]
        {
            return Err(fail());
        }
    }
    if !r["confirmation"].is_null() {
        let confirmation: SignedReadRequest<SafeAlternativeCommandV1> =
            serde_json::from_value(r["confirmation"]["signed"].clone()).map_err(|_| fail())?;
        crate::service_admission::verify_read_request(
            confirmation.request.signing_domain(),
            &confirmation,
        )
        .map_err(|_| fail())?;
        let Op::Confirm {
            generation,
            candidate_id,
            candidate_digest,
            confirmation_id,
            valid_until_tick,
            ..
        } = &confirmation.request.operation
        else {
            return Err(fail());
        };
        if super::safe_alternative::lineage_key(&confirmation.request).as_str() != key
            || confirmation.subject_public_key != signed.subject_public_key
            || Some(*generation) != r["generation"].as_u64()
            || r["confirmation"]["confirmation_id"] != *confirmation_id
            || r["confirmation"]["candidate_id"] != *candidate_id
            || r["confirmation"]["candidate_digest"] != *candidate_digest
            || r["confirmation"]["valid_until_tick"] != *valid_until_tick
            || !candidates.iter().any(|v| {
                v["candidate_id"] == *candidate_id && v["candidate_digest"] == *candidate_digest
            })
            || r["authorization"]["authorization_id"]
                != format!(
                    "alternative-auth-{}",
                    super::safe_alternative::digest(&r["confirmation"])
                )
        {
            return Err(fail());
        }
    } else if !r["authorization"].is_null() {
        return Err(fail());
    }
    if let Some(action_id) = r["authorization"]["action_id"].as_u64() {
        if !action_ids.insert(action_id) {
            return Err(fail());
        }
        let execution: SignedReadRequest<SafeAlternativeCommandV1> =
            serde_json::from_value(r["execution_signed"].clone()).map_err(|_| fail())?;
        crate::service_admission::verify_read_request(
            execution.request.signing_domain(),
            &execution,
        )
        .map_err(|_| fail())?;
        let Op::Execute {
            authorization_id,
            execution_id,
            ..
        } = &execution.request.operation
        else {
            return Err(fail());
        };
        if super::safe_alternative::lineage_key(&execution.request).as_str() != key
            || execution.subject_public_key != signed.subject_public_key
            || r["authorization"]["execution_id"] != *execution_id
            || r["authorization"]["authorization_id"] != *authorization_id
        {
            return Err(fail());
        }
        if r["outcome"].is_null() {
            let queued = pending.filter(|a| a.id == action_id).ok_or_else(fail)?;
            let candidate = candidates
                .iter()
                .find(|v| v["candidate_id"] == r["confirmation"]["candidate_id"])
                .ok_or_else(fail)?;
            if !matches!(&queued.action,Action::MoveAgent{agent_id,to} if json!(agent_id)==r["agent_id"]&&json!(to)==candidate["to"])
            {
                return Err(fail());
            }
        } else if pending.is_some_and(|a| a.id == action_id)
            || r["outcome"]["action_id"] != action_id
            || r["outcome"]["event_id"].as_u64().is_none()
        {
            return Err(fail());
        }
    } else if !r["authorization"]["execution_id"].is_null() || !r["outcome"].is_null() {
        return Err(fail());
    }

    Ok(())
}
