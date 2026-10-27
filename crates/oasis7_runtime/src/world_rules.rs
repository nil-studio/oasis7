pub use oasis7_world_state::world_rules::*;
mod action_results;
pub mod industrial;
pub mod substitution_read;

impl crate::World {
    pub fn world_rule_status(&self, reader: Option<&str>) -> Option<serde_json::Value> {
        let binding = self.world_rule_binding();
        let recent_results = self.recent_world_rule_action_results(reader);
        let mut status = match self
            .state()
            .world_rule_status_with_binding(reader, binding.as_deref())
        {
            Some(status) => status,
            None if reader
                .and_then(|actor| self.current_agent_identity(actor))
                .is_some() =>
            {
                serde_json::json!({})
            }
            None => return None,
        };
        if reader
            .and_then(|actor| {
                self.state()
                    .resolved_agent_owner(actor)
                    .map(|owner| (actor, owner))
            })
            .is_some_and(|(actor, owner)| {
                self.industrial_owner_identity_conflicts(actor, &owner.player_id)
            })
        {
            status["manufacturing_quotes"] = serde_json::json!({});
        }
        if let Some(rules) = &self.state().world_rules {
            for (id, order) in &rules.agent_orders {
                let Some(value) = status["agent_orders"].get_mut(id) else {
                    continue;
                };
                let readable = self.can_read_agent_industrial_order(order, reader);
                value["current_owner"] = serde_json::Value::Null;
                value["current_control"] = if readable {
                    serde_json::json!({"status":"blocked","reason":"current_authority_unavailable"})
                } else {
                    serde_json::Value::Null
                };
                if readable
                    && let (Some(buyer), Some(owner), Some(identity)) = (
                        self.current_agent_owner(&order.owner_id),
                        self.current_agent_owner(&order.target_agent_id),
                        self.current_agent_identity(&order.target_agent_id),
                    )
                    && owner.primary_agent_id == buyer.primary_agent_id
                    && owner.player_id == buyer.player_id
                    && identity.owner_binding == owner.player_id
                {
                    value["current_owner"] = serde_json::json!({
                        "primary_agent_id":owner.primary_agent_id,
                        "player_id":owner.player_id,
                    });
                    value["current_control"] = serde_json::json!({
                        "status":"active",
                        "target_agent_id":order.target_agent_id,
                        "source_id":owner.source_id,
                        "source_generation":self.state().agent_claims.get(&order.target_agent_id).and_then(|claim|claim.control_source.as_ref()).map(|source|source.acquisition_sequence),
                        "claim_owner_id":self.state().agent_claims.get(&order.target_agent_id).map(|claim|&claim.claim_owner_id),
                        "generation":owner.generation,
                        "identity_generation":identity.generation,
                    });
                }
                let blocker = readable
                    .then(|| self.industrial_order_blocker(order, self.state().time))
                    .flatten();
                value["blocker"] = serde_json::json!(blocker);
                if order.refund_pending {
                    value["next_step"] = serde_json::json!(if blocker.as_deref()
                        == Some("refund_pending_authoritative_release")
                    {
                        "await_authoritative_refund_release"
                    } else {
                        "restore_original_owner_source_for_frozen_refund"
                    });
                }
            }
        }
        status["recent_action_results"] = recent_results;
        status["recent_action_results_history"] = if self
            .chain_resource_manifest()
            .genesis_ref
            .is_none()
        {
            serde_json::json!({"status":"unavailable","reason":"protocol_genesis_binding_unavailable","missing_result":"unknown"})
        } else {
            serde_json::json!({"status":"bounded","event_window":2048,"result_limit":16,"missing_result":"unknown"})
        };
        Some(status)
    }
    pub fn can_read_agent_industrial_order(
        &self,
        order: &AgentIndustrialOrderV1,
        reader: Option<&str>,
    ) -> bool {
        let binding = self.world_rule_binding();
        !self.industrial_owner_identity_conflicts(&order.owner_id, &order.quote.owner_player_id)
            && self
                .state()
                .can_read_agent_industrial_order(order, reader, binding.as_deref())
    }
}
#[cfg(test)]
use crate::*;
#[cfg(test)]
use oasis7_agent_api::world_action::WorldRuleCommandV1;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn emergency_authorize_fixture() -> (crate::World, oasis7_agent_api::world_action::Action)
{
    tests::emergency_authorize_fixture()
}
#[cfg(test)]
#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) fn essential_recipe_fixture() -> (crate::World, oasis7_agent_api::world_action::Action) {
    tests::essential_recipe_fixture()
}
#[cfg(test)]
pub mod industrial_work_admission;
#[cfg(test)]
pub mod recipe_substitution;

#[cfg(test)]
use std::collections::BTreeMap;

/// Same read-only capability facts for authenticated Service and Viewer adapters.
/// `execution_actor` is supplied only after the entry point verifies its current
/// owner authority. Merely observing the personal subject never enables actions.
impl crate::World {
    pub fn gameplay_capability_query(
        &self,
        subject: oasis7_world_state::gameplay_capability::GameplaySubject,
        goal_id: &str,
        execution_actor: Option<&str>,
    ) -> oasis7_world_state::gameplay_capability::GameplayCapabilityQuery {
        use oasis7_world_state::gameplay_capability::RecoveryAssessment;
        let mut query = self.state().gameplay_capability_query(subject, goal_id);
        if goal_id != oasis7_world_state::gameplay_capability::CURRENT_INDUSTRIAL_CAPABILITY_GOAL {
            return query;
        }
        let owner = query.subject.owner();
        let factory = query.primary_factory_id.as_deref();
        let recipe = query.current_recipe_id.as_deref();
        let mut candidate = RecoveryAssessment {
            kind: "current_recipe_run".into(),
            status: "unknown".into(),
            reason_code: "current_line_evidence_unavailable".into(),
            action: None,
            evidence: None,
            recheck: Some(
                "fresh_authenticated_read_after_authority_resource_or_line_change".into(),
            ),
        };
        if let (Some(owner), Some(factory), Some(recipe)) = (owner, factory, recipe) {
            if execution_actor != Some(owner) {
                candidate.reason_code = "current_execution_authority_unavailable".into();
            } else {
                match self.builtin_recipe_quote(owner, factory, recipe, 1) {
                    Ok(quote) => {
                        candidate.status = if quote.rejection.is_some() {
                            "blocked"
                        } else {
                            "available"
                        }
                        .into();
                        candidate.reason_code = if quote.rejection.is_some() {
                            "canonical_recipe_admission_rejected"
                        } else {
                            "canonical_recipe_quote_available"
                        }
                        .into();
                        candidate.action = Some(
                            serde_json::to_value(&quote.action)
                                .expect("serializable recipe action"),
                        );
                        candidate.evidence = Some(serde_json::json!({
                            "quoted_at_tick":quote.quoted_at_tick, "rejection":quote.rejection,
                            "preview":quote.preview, "authorization":quote.authorization,
                        }));
                    }
                    Err(_) => {
                        // A missing grant or unsupported builtin plan is not proof
                        // of a domain repair, nor permission to create one.
                        candidate.reason_code = "canonical_recipe_quote_unavailable".into();
                    }
                }
            }
        }
        query.recovery.push(candidate);
        query
    }
}
