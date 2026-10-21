//! Bounded business outcomes, distinct from cognition submission receipts.
use crate::{CapabilityAuthorizationEvent, CausedBy, DomainEvent, World, WorldEventBody};
use serde_json::{Value, json};

const JOURNAL_WINDOW: usize = 2048;
const RESULT_LIMIT: usize = 16;

impl World {
    pub fn recent_world_rule_action_results(&self, reader: Option<&str>) -> Value {
        let Some(reader) = reader else {
            return json!([]);
        };
        let Some(identity) = self.current_agent_identity(reader) else {
            return json!([]);
        };
        let Ok(public_key) =
            crate::service_admission::agent_authority::owner_public_key(self, reader)
        else {
            return json!([]);
        };
        let manifest = self.chain_resource_manifest();
        let events: Vec<_> = self
            .journal()
            .events
            .iter()
            .rev()
            .take(JOURNAL_WINDOW)
            .collect();
        let mut results = Vec::new();
        for event in &events {
            let WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::WorldServiceIntentRecorded { record, .. },
            ) = &event.body
            else {
                continue;
            };
            let intent = &record["request"]["signed_payload"];
            let request = &intent["payload"]["request"];
            let action = &request["action"];
            let fence = &request["task_fence"];
            let lineage = &record["receipt"]["lineage"];
            if intent["kind"] != "cognition"
                || action["type"] != "WorldRule"
                || action["data"]["actor_id"] != reader
                || lineage["agent_id"] != reader
                || lineage["status"] != "committed"
                || fence["owner_binding"] != identity.owner_binding
                || fence["owner_generation"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    != Some(identity.generation)
                || fence["owner_public_key"] != public_key
                || fence["world_id"] != manifest.world_id
                || fence["genesis_digest"].as_str() != manifest.genesis_ref.as_deref()
            {
                continue;
            }
            let Ok(signed) = serde_json::from_value::<
                crate::service_admission::SignedReadRequest<
                    crate::service_admission::CognitionIntentV1,
                >,
            >(intent["payload"].clone()) else {
                continue;
            };
            // The current owner may inspect an old executor's result after key
            // rotation. Verify its original signature without reauthorizing execution.
            if crate::service_admission::verify_read_request("cognition", &signed).is_err() {
                continue;
            }
            let action_digest = crate::cognition_recovery::cognition_digest_v1(
                "oasis7.cognition.action.v1",
                &request["action"],
            );
            let committed_action = self.cognition()["commit_records"]
                .as_array()
                .and_then(|rows| {
                    rows.iter().rev().take(JOURNAL_WINDOW).find(|row| {
                        row["status"] == "committed"
                            && row["receipt_id"] == lineage["receipt_id"]
                            && row["action_id"] == lineage["action_id"]
                            && row["envelope_digest"] == lineage["envelope_digest"]
                    })
                });
            // A task fence is outside the action digest. Bind it to the
            // original, pre-commit signed provider prefix, never a later owner.
            if !events.iter().any(|prefix| {
                if prefix.id >= event.id
                    || committed_action.is_none_or(|commit| {
                        prefix.time > commit["parent_tick"].as_u64().unwrap_or(0)
                    })
                {
                    return false;
                }
                let WorldEventBody::CapabilityAuthorization(
                    CapabilityAuthorizationEvent::WorldServiceIntentRecorded {
                        record: prefix_record,
                        ..
                    },
                ) = &prefix.body
                else {
                    return false;
                };
                let payload = &prefix_record["request"]["signed_payload"];
                if prefix_record["rejected"].is_string() || payload["kind"] != "scheduler" {
                    return false;
                }
                let Ok(prefix_signed) = serde_json::from_value::<
                    crate::service_admission::SignedReadRequest<
                        crate::service_admission::SchedulerIntentV1,
                    >,
                >(payload["payload"].clone()) else {
                    return false;
                };
                let crate::service_admission::SchedulerOperationV1::TaskProviderPrefix { context } =
                    &prefix_signed.request.operation
                else {
                    return false;
                };
                prefix_signed.subject_public_key == signed.subject_public_key
                    && crate::service_admission::verify_read_request("scheduler", &prefix_signed)
                        .is_ok()
                    && serde_json::to_value(&context.fence).ok().as_ref() == Some(fence)
                    && context.agent_session_id == signed.request.request.agent_session_id
                    && context.agent_turn_id == signed.request.request.agent_turn_id
                    && context.decision_request_id == signed.request.request.decision_request_id
                    && context.request_commitment == signed.request.request.request_digest
                    && context.context_commitment == signed.request.request.context_digest
                    && context.retry_seq == signed.request.request.retry_seq
                    && context.transport_attempt == signed.request.request.transport_attempt
            }) {
                continue;
            }
            if !committed_action
                .and_then(|row| row["commit_id"].as_str())
                .is_some_and(|id| self.cognition()["action_digests"][id] == action_digest)
            {
                continue;
            }
            if lineage["request_digest"] != request["request"]["request_digest"]
                || lineage["decision_request_id"] != request["request"]["decision_request_id"]
                || lineage["agent_turn_id"] != request["request"]["agent_turn_id"]
                || lineage["agent_session_id"] != request["request"]["agent_session_id"]
                || !self.cognition()["receipt_lineage_registry"]
                    .as_array()
                    .is_some_and(|rows| {
                        rows.iter()
                            .rev()
                            .take(JOURNAL_WINDOW)
                            .any(|row| row == lineage)
                    })
            {
                continue;
            }
            let Some(action_id) = lineage["action_id"]
                .as_str()
                .and_then(|s| s.strip_prefix("action:"))
                .and_then(|s| s.parse::<u64>().ok())
            else {
                continue;
            };
            if results
                .iter()
                .any(|row: &Value| row["action_id"] == action_id)
            {
                continue;
            }
            let mut result = json!({"action_id":action_id,"business_request_id":action["data"]["request_id"],"receipt_id":lineage["receipt_id"],"status":"unknown","reason":null,"event_id":null,"tick":null,"next_step":"read_current_business_state_before_retry"});
            for business in &events {
                if business.caused_by != Some(CausedBy::Action(action_id)) {
                    continue;
                }
                match &business.body {
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id: rejected,
                        reason,
                    }) if *rejected == action_id => {
                        result["status"] = json!("rejected");
                        result["reason"] = json!(reason);
                        result["next_step"] = json!(
                            if matches!(reason, crate::RejectReason::RuleDenied { notes } if notes.iter().any(|s| s == "ResourceBalanceInvalid { reason: \"manufacturing_quote_expired\" }"))
                            {
                                "refresh_quote_and_submit_new_request"
                            } else {
                                "resolve_rejection_before_new_request"
                            }
                        );
                    }
                    WorldEventBody::Domain(DomainEvent::WorldRuleCommitted {
                        action_id: committed,
                        actor_id,
                        request_id,
                        command,
                        world_binding,
                        ..
                    }) if *committed == action_id
                        && actor_id == reader
                        && action["data"]["request_id"] == *request_id
                        && world_binding == &self.world_rule_binding()
                        && serde_json::to_value(command).ok().as_ref()
                            == Some(&action["data"]["command"]) =>
                    {
                        result["status"] = json!("committed");
                        result["next_step"] = json!("read_current_business_state");
                    }
                    _ => continue,
                }
                result["event_id"] = json!(business.id);
                result["tick"] = json!(business.time);
                break;
            }
            results.push(result);
            if results.len() == RESULT_LIMIT {
                break;
            }
        }
        json!(results)
    }
}
