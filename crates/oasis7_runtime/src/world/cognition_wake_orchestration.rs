//! Wake evaluation and state-predicate context enrichment.
//!
//! Scheduler admission owns lease selection; this module owns the pure
//! runtime evidence context that determines whether a continuation is ready.

use super::World;
use crate::cognition::PreconditionSubjectV1;
use crate::cognition_wake::{
    WakeConditionV1, WakeConditionValidator, WakeEvaluation, WakeEvaluationContext,
};
use crate::error::WorldError;
use oasis7_agent_api::world_action::ResourceKind;
use serde_json::Value as JsonValue;

impl World {
    pub fn evaluate_cognition_wake(
        &self,
        conditions: &[WakeConditionV1],
    ) -> Result<WakeEvaluation, WorldError> {
        self.evaluate_cognition_wake_at_tick(conditions, self.state.time)
    }

    pub(in crate::world) fn evaluate_cognition_wake_at_tick(
        &self,
        conditions: &[WakeConditionV1],
        tick: u64,
    ) -> Result<WakeEvaluation, WorldError> {
        let head_digest = self.current_state_root_hash()?;
        let (event_digests, receipt_ids) = self.cognition_committed_evidence()?;
        let mut context = WakeEvaluationContext::at(tick)
            .with_evaluation_head(&head_digest)
            .with_reorg_epoch(self.cognition_reorg_epoch());
        for digest in &event_digests {
            context = context.with_event(digest);
        }
        for receipt_id in &receipt_ids {
            context = context.with_receipt(receipt_id);
        }
        self.evaluate_cognition_wake_with_context(conditions, context)
    }

    pub fn evaluate_cognition_wake_from_committed_projection(
        &self,
        conditions: &[WakeConditionV1],
        event_digest: &str,
        receipt_id: &str,
    ) -> Result<WakeEvaluation, WorldError> {
        let head_digest = self.current_state_root_hash()?;
        let (event_digests, receipt_ids) = self.cognition_committed_evidence()?;
        let mut context = WakeEvaluationContext::at(self.state.time)
            .with_reorg_epoch(self.cognition_reorg_epoch())
            .with_evaluation_head(&head_digest);
        if event_digests.contains(event_digest) {
            context = context.with_event(event_digest);
        }
        if receipt_ids.contains(receipt_id) {
            context = context.with_receipt(receipt_id);
        }
        self.evaluate_cognition_wake_with_context(conditions, context)
    }

    fn evaluate_cognition_wake_with_context(
        &self,
        conditions: &[WakeConditionV1],
        context: WakeEvaluationContext,
    ) -> Result<WakeEvaluation, WorldError> {
        let context = self.enrich_cognition_wake_context(conditions, context)?;
        WakeConditionValidator::evaluate(conditions, &context)
            .map_err(|error| super::cognition_validation_error(error.code()))
    }

    fn enrich_cognition_wake_context(
        &self,
        conditions: &[WakeConditionV1],
        mut context: WakeEvaluationContext,
    ) -> Result<WakeEvaluationContext, WorldError> {
        let world_id = self.cognition_world_id();
        let world_predicate_requested = conditions.iter().any(|condition| {
            condition.kind == "state_predicate"
                && condition.subject.as_ref().is_some_and(|subject| {
                    subject.kind == "world" && world_id.as_deref() == Some(subject.id.as_str())
                })
        });
        if world_predicate_requested {
            let world_subject = PreconditionSubjectV1 {
                kind: "world".to_string(),
                id: world_id.clone().unwrap_or_default(),
            };
            context = context
                .with_subject_predicate_value(
                    &world_subject,
                    "world.logical_tick",
                    &serde_cbor::to_vec(&self.state.time).map_err(WorldError::from)?,
                )
                .with_subject_predicate_value(
                    &world_subject,
                    "world.reorg_epoch",
                    &serde_cbor::to_vec(&self.cognition_reorg_epoch()).map_err(WorldError::from)?,
                );
            let state_root = self.current_state_root_hash()?;
            context = context.with_subject_predicate_value(
                &world_subject,
                "world.state_root",
                &serde_cbor::to_vec(&state_root).map_err(WorldError::from)?,
            );
            let manifest_hash = self.current_manifest_hash()?;
            context = context.with_subject_predicate_value(
                &world_subject,
                "world.runtime_manifest_hash",
                &serde_cbor::to_vec(&manifest_hash).map_err(WorldError::from)?,
            );
        }
        for condition in conditions {
            if condition.kind != "state_predicate" {
                continue;
            }
            let Some(subject) = condition.subject.as_ref() else {
                continue;
            };
            let Some(path) = condition.path_or_rule.as_deref() else {
                continue;
            };
            match (subject.kind.as_str(), path) {
                ("agent", "agent.status")
                | ("agent", "agent.position")
                | ("agent", "agent.inventory_digest")
                | ("agent", "agent.capability_snapshot_hash")
                | ("agent", "agent.resource.electricity")
                | ("agent", "agent.resource.data") => {
                    let Some(cell) = self.state.agents.get(&subject.id) else {
                        continue;
                    };
                    let value = match path {
                        "agent.status" => serde_cbor::to_vec(
                            &cell
                                .activity
                                .as_ref()
                                .map(|activity| activity.status)
                                .unwrap_or(crate::AgentActivityStatus::Idle),
                        ),
                        "agent.position" => {
                            serde_cbor::to_vec(&(cell.state.pos.x_cm, cell.state.pos.y_cm))
                        }
                        "agent.inventory_digest" => serde_cbor::to_vec(
                            &oasis7_wasm_abi::canonical_hash(&cell.state.body_state.cargo_entries)
                                .map_err(|_| {
                                    super::cognition_validation_error("predicate_value_invalid")
                                })?,
                        ),
                        "agent.capability_snapshot_hash" => {
                            serde_cbor::to_vec(&self.capability_authorization_root())
                        }
                        "agent.resource.electricity" => {
                            serde_cbor::to_vec(&cell.state.resources.get(ResourceKind::Electricity))
                        }
                        "agent.resource.data" => {
                            serde_cbor::to_vec(&cell.state.resources.get(ResourceKind::Data))
                        }
                        _ => unreachable!(),
                    }
                    .map_err(WorldError::from)?;
                    context = context.with_subject_predicate_value(subject, path, &value);
                }
                ("intent", "intent.status") => {
                    let Some(intent) = self.state.agent_intent_ledger.get(&subject.id) else {
                        continue;
                    };
                    let value = serde_cbor::to_vec(&intent.status).map_err(WorldError::from)?;
                    context = context.with_subject_predicate_value(subject, path, &value);
                }
                _ => {}
            }
        }
        Ok(context)
    }

    fn cognition_world_id(&self) -> Option<String> {
        self.cognition
            .get("runtime_binding")
            .and_then(|binding| binding.get("world_id"))
            .and_then(JsonValue::as_str)
            .filter(|world_id| !world_id.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                let world_id = self.chain_resource_manifest().world_id.clone();
                (world_id != "unbound").then_some(world_id)
            })
    }

    pub(in crate::world) fn cognition_reorg_epoch(&self) -> u64 {
        let binding_epoch = self
            .cognition
            .get("runtime_binding")
            .and_then(|binding| binding.get("reorg_epoch"))
            .and_then(JsonValue::as_u64)
            .unwrap_or_default();
        let invalidation_epoch = self
            .cognition
            .get("reorg_invalidation_epoch")
            .and_then(JsonValue::as_u64)
            .unwrap_or_default();
        binding_epoch.max(
            invalidation_epoch.max(
                self.state
                    .agent_intent_ledger
                    .values()
                    .filter_map(|intent| intent.reorg_epoch)
                    .max()
                    .unwrap_or(0),
            ),
        )
    }
}
