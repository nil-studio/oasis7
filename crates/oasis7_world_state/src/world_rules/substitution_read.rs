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
        let owner = reader.and_then(|actor| state.resolved_agent_owner(actor));
        self.for_current_reader_from_owner_v1(reader, owner.as_ref())
    }
    /// Same visibility kernel over actual readonly owner facts. A caller cannot
    /// supply a canRead verdict or an owner-binding digest.
    pub fn for_current_reader_from_owner_v1(
        &self,
        reader: Option<&str>,
        owner: Option<&crate::ResolvedAgentOwnerV1>,
    ) -> Self {
        let mut view = self.for_reader(reader);
        let Some((actor, binding)) = reader.and_then(|actor| {
            super::industrial_work_admission::industrial_owner_binding_from_owner_v1(owner)
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
            super::substitution_visibility::substitution_authority_visible_with_work_v1(
                a,
                actor,
                &binding,
                &mut |_, _| Ok::<(), std::convert::Infallible>(()),
                &mut |reference| {
                    Ok(work
                        .roots
                        .get(&reference.id)
                        .is_some_and(|root| root.root_ref == *reference))
                },
            )
            .unwrap()
        });
        substitutions.decisions.retain(|_, d| {
            super::substitution_visibility::substitution_decision_visible_with_work_v1(
                d,
                actor,
                &binding,
                &mut |_, _| Ok::<(), std::convert::Infallible>(()),
                &mut |reference| {
                    Ok(work
                        .roots
                        .get(&reference.id)
                        .is_some_and(|root| root.root_ref == *reference))
                },
            )
            .unwrap()
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
        view.requests.retain(|_, receipt| {
            super::substitution_visibility::current_rule_request_visible_with_work_v1(
                receipt,
                Some(actor),
                Some(&binding),
                &mut |_, _| Ok::<(), std::convert::Infallible>(()),
                &mut |reference| {
                    Ok(work
                        .roots
                        .get(&reference.id)
                        .is_some_and(|r| r.root_ref == *reference))
                },
                &mut |reference| {
                    Ok(work
                        .lots
                        .get(&reference.id)
                        .is_some_and(|l| l.lot_ref == *reference))
                },
                &mut |reference| {
                    Ok(substitutions
                        .decisions
                        .get(&reference.id)
                        .is_some_and(|d| d.decision_ref == *reference))
                },
            )
            .unwrap()
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
        match super::readonly_status_substitution::substitution_status_readonly_v1(
            self,
            reader,
            world_binding,
        ) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }
}
