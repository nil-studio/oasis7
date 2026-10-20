//! Certified, read-only selection and explicitly authorized atomic conversion.
use super::{PreparedWorldRule, industrial_work as work, invalid, verify_world_rule_authority};
use crate::{MaterialLedgerId, WorldError, WorldState, WorldTime};
use oasis7_agent_api::world_action::*;
pub use oasis7_agent_api::world_action::{SubstitutionPreviewV1, SubstitutionStateV1};
use oasis7_wasm_abi::SubstitutionConversionV1;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct SubstitutionContext<'a> {
    actor: &'a str,
    world: &'a str,
    now: u64,
}
fn qualified(r: &IndustrialRefV1, world: &str) -> Result<(), WorldError> {
    if !r.is_qualified() || r.world_binding != world {
        return Err(invalid("substitution_reference_invalid"));
    }
    Ok(())
}
mod evaluation;
use evaluation::candidate;
/// No identities, reservations, receipts or other state are allocated by preview.
pub fn preview_substitution(
    state: &WorldState,
    actor: &str,
    request: &SubstitutionPreviewRequestV1,
    world: &str,
    now: u64,
) -> Result<SubstitutionPreviewV1, WorldError> {
    let rules = state
        .world_rules
        .as_ref()
        .ok_or_else(|| invalid("missing_authority"))?;
    preview_with_rules(
        state,
        &rules.substitution,
        &rules.industrial_work,
        actor,
        request,
        world,
        now,
    )
}
fn preview_with_rules(
    state: &WorldState,
    rules: &SubstitutionStateV1,
    industrial: &IndustrialWorkStateV1,
    actor: &str,
    request: &SubstitutionPreviewRequestV1,
    world: &str,
    now: u64,
) -> Result<SubstitutionPreviewV1, WorldError> {
    qualified(&request.root_ref, world)?;
    let root = industrial
        .roots
        .get(&request.root_ref.id)
        .filter(|r| r.root_ref == request.root_ref && r.owner_id == actor)
        .ok_or_else(|| invalid("substitution_root_missing_or_unauthorized"))?;
    if root.owner_binding_digest
        != super::industrial_work_admission::industrial_owner_binding(state, actor)?
    {
        return Err(invalid("substitution_root_owner_stale"));
    }
    if !root.slots.contains_key(&request.slot_id) {
        return Err(invalid("substitution_slot_missing"));
    }
    let expected = rules
        .authorities
        .values()
        .filter(|a| {
            a.root_ref == request.root_ref && a.slot_id == request.slot_id && a.owner_id == actor
        })
        .map(|a| a.authority_ref.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let supplied = request
        .authority_refs
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if expected.len() > 32
        || (!supplied.is_empty()
            && (supplied != expected || supplied.len() != request.authority_refs.len()))
    {
        return Err(invalid("substitution_incomplete_authority_set"));
    }
    let mut candidates = Vec::new();
    let mut blocker = None;
    for reference in &expected {
        match candidate(
            state,
            rules,
            industrial,
            request,
            reference,
            SubstitutionContext { actor, world, now },
        ) {
            Ok(c) => candidates.push(c),
            Err(e) => {
                blocker = Some(format!("{e:?}"));
            }
        }
    }
    if expected.is_empty() {
        blocker = Some("missing_authority".into());
    }
    let no_rules = state
        .recipe_profiles
        .get(&root.pinned_recipe_id)
        .is_some_and(|p| {
            p.substitution_rules.is_empty()
                && crate::util::hash_json(p).ok().as_ref() == Some(&root.pinned_profile_digest)
        });
    let status = if !candidates.is_empty() {
        SubstitutionPreviewStatusV1::Available
    } else {
        let possible_source = industrial.lots.values().any(|lot| {
            lot.owner_id == actor
                && lot.remaining > 0
                && root
                    .slots
                    .get(&request.slot_id)
                    .is_some_and(|slot| slot.material == lot.material)
        });
        let possible_recipe = industrial.roots.values().any(|other| {
            other.root_ref != root.root_ref
                && other.owner_id == actor
                && other.factory_id == root.factory_id
                && other.work == 0
                && !other.first_sink
        });
        if no_rules && expected.is_empty() && !possible_source && !possible_recipe {
            blocker = None;
            SubstitutionPreviewStatusV1::NoLegalSubstitute
        } else {
            SubstitutionPreviewStatusV1::Unknown
        }
    };
    let mut preview = SubstitutionPreviewV1 {
        preview_digest: String::new(),
        status,
        blocker,
        next_action: if candidates.is_empty() {
            "resolve_blocker".into()
        } else {
            "explicit_confirmation".into()
        },
        next_recheck: candidates
            .iter()
            .map(|c| c.authority.next_recheck)
            .min()
            .unwrap_or(now),
        request: request.clone(),
        candidates,
        evaluated_at: now,
    };
    preview.preview_digest = crate::util::hash_json(&preview)?;
    if serde_json::to_vec(&preview)?.len() > 48 * 1024 {
        return Err(invalid("substitution_preview_capacity_exceeded"));
    }
    Ok(preview)
}
impl PreparedWorldRule {
    pub fn recipe_substitution(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        command: &RecipeSubstitutionCommandV1,
        now: WorldTime,
        world_binding: Option<&str>,
    ) -> Result<String, WorldError> {
        let world = world_binding.ok_or_else(|| invalid("substitution_world_binding_missing"))?;
        match command {
            RecipeSubstitutionCommandV1::PublishAuthority {
                authority,
                consensus_height,
                finality_certificate,
            } => {
                qualified(&authority.authority_ref, world)?;
                let hash = crate::util::hash_json(&(
                    "recipe-substitution-authority/v1",
                    world,
                    authority,
                ))?;
                verify_world_rule_authority(state, &hash, *consensus_height, finality_certificate)?;
                if authority.valuation.unit.is_empty()
                    || authority.valuation.unit.len() > 256
                    || authority.valuation.unit.trim() != authority.valuation.unit
                    || authority.valuation.consumed != 0
                    || authority.risks.is_empty()
                    || authority.risks.len() > 32
                    || authority
                        .risks
                        .iter()
                        .any(|r| r.is_empty() || r.len() > 256 || r.trim() != r)
                    || authority.next_recheck < now
                    || authority.next_recheck > authority.valid_until
                {
                    return Err(invalid("substitution_risk_evidence_invalid"));
                }
                if authority.owner_id != actor
                    || authority.valid_until < now
                    || authority.costs.len() > 32
                {
                    return Err(invalid("substitution_authority_invalid"));
                }
                let id = &authority.authority_ref.id;
                if let Some(old) = self.rules.substitution.authorities.get(id) {
                    if old != authority.as_ref() {
                        return Err(invalid("substitution_authority_conflict"));
                    }
                } else {
                    if self.rules.substitution.authorities.len() >= 4096 {
                        return Err(invalid("substitution_authority_capacity_exceeded"));
                    }
                    self.rules
                        .substitution
                        .authorities
                        .insert(id.clone(), authority.as_ref().clone());
                }
                Ok(id.clone())
            }
            RecipeSubstitutionCommandV1::Confirm { confirmation } => {
                self.confirm_substitution(state, actor, key, confirmation, now, world)
            }
            RecipeSubstitutionCommandV1::Convert { decision_ref } => {
                self.convert_substitution(state, actor, decision_ref, now, world)
            }
            RecipeSubstitutionCommandV1::ApplyBinding { decision_ref } => {
                self.bind_substitution(state, actor, decision_ref, now, world)
            }
            RecipeSubstitutionCommandV1::Cutover {
                decision_ref,
                disposition,
                candidate_root,
            } => self.cutover_substitution(
                state,
                decision_ref,
                disposition,
                candidate_root,
                SubstitutionContext { actor, world, now },
            ),
        }
    }
    fn decision(
        &self,
        actor: &str,
        r: &IndustrialRefV1,
        world: &str,
    ) -> Result<SubstitutionDecisionV1, WorldError> {
        qualified(r, world)?;
        self.rules
            .substitution
            .decisions
            .get(&r.id)
            .filter(|d| d.decision_ref == *r && d.owner_id == actor)
            .cloned()
            .ok_or_else(|| invalid("substitution_decision_missing"))
    }
    fn confirm_substitution(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        intent: &ConfirmSubstitutionV1,
        now: u64,
        world: &str,
    ) -> Result<String, WorldError> {
        let fresh = preview_with_rules(
            state,
            &self.rules.substitution,
            &self.rules.industrial_work,
            actor,
            &intent.preview.request,
            world,
            intent.preview.evaluated_at,
        )?;
        if fresh != intent.preview || intent.preview.evaluated_at > now {
            return Err(invalid("stale_candidate"));
        }
        let selected = fresh
            .candidates
            .iter()
            .find(|c| c.candidate_digest == intent.candidate_digest)
            .ok_or_else(|| invalid("stale_candidate"))?
            .clone();
        let current = candidate(
            state,
            &self.rules.substitution,
            &self.rules.industrial_work,
            &intent.preview.request,
            &selected.authority.authority_ref,
            SubstitutionContext { actor, world, now },
        )?;
        let candidate = selected;
        if current != candidate {
            return Err(invalid("stale_candidate"));
        }
        if candidate.authority.valid_until < now
            || candidate.source_required() > intent.maximum_source
            || candidate.authority.power_cost > intent.maximum_power
            || candidate
                .authority
                .costs
                .iter()
                .any(|(k, v)| intent.maximum_costs.get(k).is_none_or(|max| v > max))
        {
            return Err(invalid("substitution_confirmation_limit"));
        }
        if intent.cutover_plan_digest.is_some()
            && intent.cutover_plan_digest != candidate.cutover_plan_digest
        {
            return Err(invalid("substitution_cutover_plan_stale"));
        }
        if !candidate.causal_equivalent
            && intent.scope == SubstitutionScopeV1::DecisionAndExactConversion
            && (candidate.cutover_plan_digest.is_none()
                || intent.cutover_plan_digest != candidate.cutover_plan_digest)
        {
            return Err(invalid("causal_change_requires_cutover"));
        }
        let id = crate::util::hash_json(&(world, actor, key, "substitution_decision"))?;
        let r = work::industrial_ref(
            world,
            "industrial",
            "recipe-substitution/v1",
            &id,
            1,
            intent,
        )?;
        let decision = SubstitutionDecisionV1 {
            decision_ref: r.clone(),
            owner_id: actor.into(),
            confirmation_key: key.into(),
            intent: intent.clone(),
            candidate,
            confirmed_at: now,
        };
        if let Some(old) = self.rules.substitution.decisions.get(&id) {
            if old == &decision {
                return Ok(id);
            }
            return Err(invalid("idempotency_conflict"));
        }
        if self.rules.substitution.decisions.len() >= 16384 {
            return Err(invalid("substitution_decision_capacity_exceeded"));
        }
        work::record_receipt(
            &mut self.rules.industrial_work,
            IndustrialReceiptV1 {
                receipt_ref: r.clone(),
                root_ref: intent.preview.request.root_ref.clone(),
                child_ref: r,
                effect_role: "substitution_decision".into(),
                payload_digest: crate::util::hash_json(&decision)?,
                input: vec![],
                output: vec![],
            },
            world,
        )?;
        self.rules
            .substitution
            .decisions
            .insert(id.clone(), decision);
        Ok(id)
    }
}
mod execution;

/// Owner-scoped projection; it grants no mutation authority.
pub fn for_reader(state: &SubstitutionStateV1, reader: &str) -> SubstitutionStateV1 {
    let mut view = state.clone();
    view.authorities.retain(|_, a| a.owner_id == reader);
    view.decisions.retain(|_, d| d.owner_id == reader);
    view.settlements
        .retain(|id, _| view.decisions.contains_key(id));
    view.conversion_domains.retain(|key| {
        view.decisions.values().any(|d| {
            d.candidate
                .plan
                .as_ref()
                .is_some_and(|plan| &plan.domain_key == key)
        })
    });
    view.terminal_used
        .retain(|id, _| view.authorities.contains_key(id));
    view.retained_residuals.retain(|_, r| r.owner_id == reader);
    view
}
