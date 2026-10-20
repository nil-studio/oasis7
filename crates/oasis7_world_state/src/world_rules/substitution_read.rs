//! One authorized projection for native API, Agent and Viewer consumers.
use super::{WorldRulesStateV1, industrial_work, recipe_substitution};
use crate::WorldState;
use oasis7_agent_api::world_action::SubstitutionPreviewRequestV1;
use oasis7_agent_api::world_action::{
    IndustrialWorkCommandV1, RecipeSubstitutionCommandV1, WorldRuleCommandV1,
};

impl WorldRulesStateV1 {
    /// Reacquiring an Agent identity does not grant access to its former owner's records.
    pub fn for_current_reader(&self, state: &WorldState, reader: Option<&str>) -> Self {
        let mut view = self.for_reader(reader);
        let Some((actor, binding)) = reader.and_then(|actor| {
            super::industrial_work_admission::industrial_owner_binding(state, actor)
                .ok()
                .map(|binding| (actor, binding))
        }) else {
            view.requests.retain(|_, r| {
                !matches!(
                    r.command,
                    WorldRuleCommandV1::IndustrialWork { .. }
                        | WorldRuleCommandV1::RecipeSubstitution { .. }
                )
            });
            return view;
        };
        let mut work = industrial_work::for_reader(&self.industrial_work, actor);
        work.roots.retain(|_, r| r.owner_binding_digest == binding);
        work.lots.retain(|id, _| {
            work.imported_lot_authorities
                .get(id)
                .is_some_and(|a| a.owner_binding_digest == binding)
        });
        work.imported_lot_authorities
            .retain(|id, _| work.lots.contains_key(id));
        work.receipts.retain(|_, receipt| {
            work.roots
                .get(&receipt.root_ref.id)
                .is_some_and(|root| root.root_ref == receipt.root_ref)
        });
        work.bindings.retain(|plan| {
            work.roots
                .get(&plan.root_ref.id)
                .is_some_and(|root| root.root_ref == plan.root_ref)
        });
        work.dispositions
            .retain(|id, _| work.roots.contains_key(id));
        let mut substitutions = recipe_substitution::for_reader(&self.substitution, actor);
        substitutions.authorities.retain(|_, a| {
            a.owner_binding_digest == binding
                && work
                    .roots
                    .get(&a.root_ref.id)
                    .is_some_and(|r| r.root_ref == a.root_ref)
        });
        substitutions.decisions.retain(|_, d| {
            d.candidate.authority.owner_binding_digest == binding
                && work
                    .roots
                    .get(&d.intent.preview.request.root_ref.id)
                    .is_some_and(|r| r.root_ref == d.intent.preview.request.root_ref)
        });
        substitutions
            .settlements
            .retain(|id, _| substitutions.decisions.contains_key(id));
        substitutions.conversion_domains.retain(|key| {
            substitutions.decisions.values().any(|d| {
                d.candidate
                    .plan
                    .as_ref()
                    .is_some_and(|plan| plan.domain_key == *key)
            })
        });
        substitutions
            .terminal_used
            .retain(|id, _| substitutions.authorities.contains_key(id));
        substitutions.retained_residuals.retain(|_, residual| {
            substitutions
                .settlements
                .values()
                .any(|settlement| settlement.receipt_ref == residual.settlement_ref)
        });
        view.requests = self
            .requests
            .iter()
            .filter(|(_, receipt)| receipt.actor_id == actor)
            .map(|(id, receipt)| (id.clone(), receipt.clone()))
            .collect();
        view.requests.retain(|_, receipt| match &receipt.command {
            WorldRuleCommandV1::IndustrialWork { command } => match command.as_ref() {
                IndustrialWorkCommandV1::ImportQualifiedLot { facts, .. } => {
                    facts.owner_binding_digest == binding
                }
                IndustrialWorkCommandV1::AcceptRoot { authority, .. } => {
                    authority.owner_binding_digest == binding
                }
                IndustrialWorkCommandV1::ReleaseLot { lot_ref } => work
                    .lots
                    .get(&lot_ref.id)
                    .is_some_and(|l| l.lot_ref == *lot_ref),
                IndustrialWorkCommandV1::StartRoot { root_ref, .. }
                | IndustrialWorkCommandV1::TerminateRoot { root_ref, .. } => work
                    .roots
                    .get(&root_ref.id)
                    .is_some_and(|r| r.root_ref == *root_ref),
            },
            WorldRuleCommandV1::RecipeSubstitution { command } => match command.as_ref() {
                RecipeSubstitutionCommandV1::PublishAuthority { authority, .. } => {
                    authority.owner_binding_digest == binding
                }
                RecipeSubstitutionCommandV1::Confirm { confirmation } => work
                    .roots
                    .get(&confirmation.preview.request.root_ref.id)
                    .is_some_and(|r| r.root_ref == confirmation.preview.request.root_ref),
                RecipeSubstitutionCommandV1::Convert { decision_ref }
                | RecipeSubstitutionCommandV1::ApplyBinding { decision_ref }
                | RecipeSubstitutionCommandV1::Cutover { decision_ref, .. } => substitutions
                    .decisions
                    .get(&decision_ref.id)
                    .is_some_and(|d| d.decision_ref == *decision_ref),
            },
            _ => true,
        });
        view.industrial_work = work;
        view.substitution = substitutions;
        view
    }
}

impl WorldState {
    pub fn recipe_substitution_status(
        &self,
        reader: Option<&str>,
        world_binding: Option<&str>,
    ) -> Option<serde_json::Value> {
        let actor = reader?;
        let rules = self
            .world_rules
            .as_ref()?
            .for_current_reader(self, Some(actor));
        let work = rules.industrial_work;
        let substitutions = rules.substitution;
        let mut previews = Vec::new();
        let mut truncated = false;
        for root in work.roots.values() {
            for slot in root.slots.keys() {
                if previews.len() == 128 {
                    truncated = true;
                    break;
                }
                let request = SubstitutionPreviewRequestV1 {
                    root_ref: root.root_ref.clone(),
                    slot_id: slot.clone(),
                    authority_refs: Vec::new(),
                };
                let result = world_binding
                    .ok_or_else(|| super::invalid("substitution_world_binding_missing"))
                    .and_then(|world| {
                        recipe_substitution::preview_substitution(
                            self, actor, &request, world, self.time,
                        )
                    });
                previews.push(match result {
                    Ok(preview) => serde_json::json!({"preview":preview}),
                    Err(error) => serde_json::json!({
                        "request":request,"status":"unknown","blocker":format!("{error:?}"),
                        "next_action":"restore_authority_and_preview_again"
                    }),
                });
            }
            if truncated {
                break;
            }
        }
        Some(serde_json::json!({
            "schema":"recipe-substitution/v1","reader_agent_id":actor,"source_tick":self.time,
            "roots":work.roots,"previews":previews,"decisions":substitutions.decisions,
            "settlements":substitutions.settlements,"residuals":substitutions.retained_residuals,"truncated":truncated,
            "decision_effect":"selection_only","conversion_effect":"linked_settlement_only"
        }))
    }
}
