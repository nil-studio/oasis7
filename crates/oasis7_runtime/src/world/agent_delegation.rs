//! Player delegation is distinct from WASM capabilities. The host binds a
//! quote to an exact decision; only the existing cognition transaction can
//! publish its debit and domain references.
use super::World;
use crate::cognition_recovery::{WorldCommitRecordV1, cognition_digest_v1};
use crate::{Action, DomainEvent, WorldError, WorldEventBody};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[path = "agent_delegation_controls.rs"]
mod controls;
pub use controls::{AgentOwnerControlKindV1, AgentOwnerControlV1};

pub use oasis7_agent_api::world_quote::AgentDelegationGrantV1;

pub use oasis7_agent_api::world_quote::AgentDelegationAuthorizationV1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AgentDecisionCausalContextV1 {
    pub intent_id: Option<String>,
    pub expected_consequence: Value,
    pub alternative: Value,
    pub stakes: Value,
    pub reason: Option<String>,
    pub evidence_refs: Vec<String>,
    pub correction_refs: Vec<String>,
    pub interruption_refs: Vec<String>,
    pub dissent: Option<String>,
    pub override_actor: Option<String>,
    #[serde(default)]
    pub owner_control_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCausalReceiptV1 {
    pub receipt_id: String,
    pub commit_id: String,
    pub agent_id: String,
    pub intent_id: Option<String>,
    pub effect_intent_id: Option<String>,
    pub action_id: u64,
    pub action_kind: String,
    pub domain_event_refs: Vec<u64>,
    pub disposition: String,
    pub primary_reason: Option<String>,
    pub next_step: String,
    pub expected_consequence: Value,
    pub actual_consequence: Value,
    pub alternative: Value,
    pub stakes: Value,
    pub evidence_refs: Vec<String>,
    pub correction_refs: Vec<String>,
    pub interruption_refs: Vec<String>,
    pub authorization: Option<AgentDelegationAuthorizationV1>,
    pub dissent: Option<String>,
    pub override_actor: Option<String>,
    pub hard_boundary: Option<String>,
    #[serde(default)]
    pub owner_control_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentDelegationDecisionV1 {
    pub decision_request_id: String,
    pub agent_id: String,
    pub action_digest: String,
    pub grant_id: Option<String>,
    pub grant_revision: Option<u64>,
    pub quote_id: Option<String>,
    pub quoted_units: u64,
    pub quote_valid_until_tick: u64,
    pub context: AgentDecisionCausalContextV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Ledger {
    #[serde(default)]
    grants: BTreeMap<String, AgentDelegationGrantV1>,
    #[serde(default)]
    spent: BTreeMap<String, u64>,
    #[serde(default)]
    decisions: BTreeMap<String, AgentDelegationDecisionV1>,
    #[serde(default)]
    receipts: BTreeMap<String, AgentCausalReceiptV1>,
    #[serde(default)]
    controls: BTreeMap<String, controls::OwnerControlRecord>,
}

fn denied(reason: &str) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: format!("agent_delegation:{reason}"),
    }
}

fn budget_key(grant: &AgentDelegationGrantV1) -> Result<String, WorldError> {
    serde_json::to_string(&(
        &grant.grant_id,
        &grant.source_id,
        &grant.object_id,
        &grant.period_id,
    ))
    .map_err(WorldError::from)
}

fn action_scope(action: &Action) -> Option<(&'static str, String)> {
    match action {
        Action::MoveAgent { agent_id, .. } => Some(("MoveAgent", agent_id.clone())),
        Action::TransferMaterial {
            from_ledger,
            to_ledger,
            kind,
            ..
        } => Some((
            "TransferMaterial",
            format!("{from_ledger}->{to_ledger}:{kind}"),
        )),
        Action::BuildFactory { spec, .. } => Some(("BuildFactory", spec.factory_id.clone())),
        Action::BuildFactoryWithModule { spec, .. } => {
            Some(("BuildFactoryWithModule", spec.factory_id.clone()))
        }
        Action::ScheduleRecipe { factory_id, .. } => Some(("ScheduleRecipe", factory_id.clone())),
        // Recognize unsupported delegated module actions so admission cannot bypass
        // the authority boundary; issuance and native quotes remain unavailable.
        Action::ScheduleRecipeWithModule { factory_id, .. } => {
            Some(("ScheduleRecipeWithModule", factory_id.clone()))
        }
        _ => None,
    }
}

impl World {
    /// Test bootstrap only: retain the production source/object/budget validation.
    /// This is not an owner-facing grant issuance API.
    #[cfg(any(test, feature = "test_tier_required", feature = "test_tier_full"))]
    pub fn install_test_native_delegation_grant(
        &mut self,
        grant: AgentDelegationGrantV1,
    ) -> Result<(), WorldError> {
        self.install_agent_delegation_grant(grant)
    }

    fn delegation_ledger(&self) -> Result<Ledger, WorldError> {
        match self.cognition.get("agent_delegation") {
            None | Some(Value::Null) => Ok(Ledger::default()),
            Some(value) => serde_json::from_value(value.clone()).map_err(WorldError::from),
        }
    }

    fn save_delegation_ledger(&mut self, ledger: &Ledger) -> Result<(), WorldError> {
        if self.cognition.is_null() {
            self.cognition =
                crate::cognition_recovery::default_cognition_persistence_projection();
        }
        self.cognition["agent_delegation"] =
            serde_json::to_value(ledger).map_err(WorldError::from)?;
        Ok(())
    }

    fn validate_grant_source(&self, grant: &AgentDelegationGrantV1) -> Result<(), WorldError> {
        // Membership is not governance authorization. No organization issuer
        // resolver exists on this native lane; it must fail closed.
        if grant.organization_id.is_some() {
            return Err(denied("organization_source_unsupported"));
        }
        let owner = self
            .state
            .starter_oc_claims
            .get(&grant.agent_id)
            .ok_or_else(|| denied("owner_source_unavailable"))?;
        if owner.player_id != grant.owner_id
            || grant.issuer_id != grant.owner_id
            || grant.source_id != format!("starter_claim:{}:{}", owner.agent_id, owner.claimed_at)
        {
            return Err(denied("owner_source_changed"));
        }
        if !self.state.agents.contains_key(&grant.agent_id) {
            return Err(denied("agent_missing"));
        }
        Ok(())
    }

    /// Trusted authenticated host entrypoint; never provider supplied.
    pub(crate) fn install_agent_delegation_grant(
        &mut self,
        grant: AgentDelegationGrantV1,
    ) -> Result<(), WorldError> {
        let mut transaction = self.clone();
        transaction.install_agent_delegation_grant_inner(grant)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    fn install_agent_delegation_grant_inner(
        &mut self,
        grant: AgentDelegationGrantV1,
    ) -> Result<(), WorldError> {
        self.validate_grant_source(&grant)?;
        if [
            &grant.grant_id,
            &grant.source_id,
            &grant.object_id,
            &grant.period_id,
        ]
        .iter()
        .any(|v| v.trim().is_empty())
            || grant.revision == 0
            || grant.limit_units == 0
            || grant.resource_kind != "electricity"
            || grant.valid_until_tick < grant.valid_from_tick
            || grant.action_kinds.is_empty()
            || grant.action_kinds.iter().any(|kind| {
                !matches!(
                    kind.as_str(),
                    "MoveAgent" | "TransferMaterial" | "ScheduleRecipe" | "BuildFactory"
                )
            })
        {
            return Err(denied("grant_invalid"));
        }
        let mut ledger = self.delegation_ledger()?;
        if let Some(old) = ledger.grants.get(&grant.grant_id) {
            if old == &grant {
                return Ok(());
            }
            // Revisions may revoke/contract scope; a source/object/period or
            // owner change is a new grant, never a reset of this allowance.
            if grant.revision <= old.revision
                || budget_key(old)? != budget_key(&grant)?
                || old.owner_id != grant.owner_id
                || old.agent_id != grant.agent_id
                || old.limit_units != grant.limit_units
                || old.resource_kind != grant.resource_kind
                || old.valid_from_tick != grant.valid_from_tick
                || old.valid_until_tick != grant.valid_until_tick
                || grant
                    .action_kinds
                    .iter()
                    .any(|kind| !old.action_kinds.contains(kind))
            {
                return Err(denied("grant_revision_conflict"));
            }
        }
        ledger.grants.insert(grant.grant_id.clone(), grant);
        self.save_delegation_ledger(&ledger)
    }

    /// Bind an exact host quote and context before the provider decision is
    /// committed. Quoted units are host accounting units, not invented prices.
    pub(crate) fn bind_agent_delegation_decision(
        &mut self,
        decision: AgentDelegationDecisionV1,
        action: &Action,
    ) -> Result<(), WorldError> {
        if decision.decision_request_id.trim().is_empty()
            || action.actor_id() != Some(decision.agent_id.as_str())
            || decision.action_digest
                != cognition_digest_v1("oasis7.cognition.action.v1", &serde_json::to_value(action)?)
        {
            return Err(denied("decision_identity_mismatch"));
        }
        if decision.grant_id.is_some()
            && decision
                .quote_id
                .as_deref()
                .is_none_or(|v| v.trim().is_empty())
        {
            return Err(denied("trusted_quote_required"));
        }
        self.validate_delegation_decision(&decision, action)?;
        let mut ledger = self.delegation_ledger()?;
        if let Some(old) = ledger.decisions.get(&decision.decision_request_id) {
            if serde_json::to_value(old)? == serde_json::to_value(&decision)? {
                return Ok(());
            }
            return Err(denied("decision_idempotency_conflict"));
        }
        ledger
            .decisions
            .insert(decision.decision_request_id.clone(), decision);
        self.save_delegation_ledger(&ledger)
    }

    fn validate_delegation_decision(
        &self,
        decision: &AgentDelegationDecisionV1,
        action: &Action,
    ) -> Result<Option<AgentDelegationAuthorizationV1>, WorldError> {
        if let Some(intent_id) = &decision.context.intent_id {
            let intent = self
                .state
                .agent_intent_ledger
                .get(intent_id)
                .ok_or_else(|| denied("intent_missing"))?;
            if intent.agent_id != decision.agent_id
                || !matches!(intent.status.as_str(), "accepted" | "blocked")
                || self
                    .state
                    .agents
                    .get(&decision.agent_id)
                    .and_then(|c| c.intent.as_ref())
                    .is_none_or(|v| &v.intent_id != intent_id)
            {
                return Err(denied("intent_replan_required"));
            }
        }
        let Some(grant_id) = &decision.grant_id else {
            if self
                .delegation_ledger()?
                .grants
                .values()
                .any(|g| g.agent_id == decision.agent_id)
            {
                return Err(denied("grant_binding_required"));
            }
            if decision.context.override_actor.is_some() {
                return Err(denied("override_requires_grant"));
            }
            return Ok(None);
        };
        let ledger = self.delegation_ledger()?;
        let grant = ledger
            .grants
            .get(grant_id)
            .ok_or_else(|| denied("grant_missing"))?;
        self.validate_grant_source(grant)?;
        if grant.revoked {
            return Err(denied("grant_revoked"));
        }
        if grant.revision != decision.grant_revision.unwrap_or(0) {
            return Err(denied("grant_revision_stale"));
        }
        if self.state.time < grant.valid_from_tick
            || self.state.time > grant.valid_until_tick
            || self.state.time > decision.quote_valid_until_tick
        {
            return Err(denied("grant_or_quote_expired"));
        }
        let (kind, object) = action_scope(action).ok_or_else(|| denied("action_unsupported"))?;
        if grant.agent_id != decision.agent_id
            || grant.object_id != object
            || !grant.action_kinds.iter().any(|v| v == kind)
        {
            return Err(denied("grant_scope_mismatch"));
        }
        if let Action::TransferMaterial { from_ledger, .. } = action
            && from_ledger != &crate::MaterialLedgerId::agent(&decision.agent_id)
        {
            return Err(denied("asset_source_unsupported"));
        }
        if let Some(intent_id) = &decision.context.intent_id
            && self
                .state
                .agent_intent_ledger
                .get(intent_id)
                .is_none_or(|intent| intent.actor_id != grant.owner_id)
        {
            return Err(denied("intent_authority_changed"));
        }
        if let Some(actor) = &decision.context.override_actor {
            if actor != &grant.owner_id {
                return Err(denied("override_owner_mismatch"));
            }
            self.validate_recorded_owner_override(&ledger, grant, &decision.context)?;
        }
        // Dissent is causal evidence, not a new permission gate. A current
        // scoped grant already authorizes the action; an explicit override,
        // when supplied, still requires its authenticated one-shot record.
        let spent = *ledger.spent.get(&budget_key(grant)?).unwrap_or(&0);
        let remaining = grant
            .limit_units
            .checked_sub(spent)
            .ok_or_else(|| denied("budget_corrupt"))?;
        if decision.quoted_units != self.native_delegation_quote(action)? {
            return Err(denied("quote_units_mismatch"));
        }
        if decision.quoted_units > remaining {
            return Err(denied("budget_exhausted"));
        }
        Ok(Some(AgentDelegationAuthorizationV1 {
            grant: grant.clone(),
            cost_units: decision.quoted_units,
            spent_units: spent,
            remaining_units: remaining,
        }))
    }

    fn native_delegation_quote(&self, action: &Action) -> Result<u64, WorldError> {
        let cost = match action {
            Action::MoveAgent { .. } => 0,
            Action::TransferMaterial {
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                priority,
                route_id,
                route_ids,
                auto_reroute,
            } => {
                self.logistics_transfer_quote_with_route_id(
                    requester_agent_id,
                    from_ledger,
                    to_ledger,
                    kind,
                    *amount,
                    *distance_km,
                    *priority,
                    route_id.as_deref(),
                    route_ids,
                    *auto_reroute,
                )
                .map_err(|_| denied("domain_quote_unavailable"))?
                .tariff_electricity_total
            }
            Action::BuildFactory { spec, .. } => {
                let power = self
                    .state
                    .factory_construction_power_profiles
                    .get(&spec.factory_id)
                    .ok_or_else(|| denied("domain_quote_construction_power_missing"))?;
                if power.factory_id != spec.factory_id
                    || !power.active
                    || power.authority_revision == 0
                    || power.electricity_amount < 0
                {
                    return Err(denied("domain_quote_construction_power_invalid"));
                }
                let canonical =
                    crate::builtin_factory_spec(&power.factory_kind, &spec.factory_id)
                        .ok_or_else(|| denied("domain_quote_factory_kind_unsupported"))?;
                if &canonical != spec {
                    return Err(denied("domain_quote_factory_spec_mismatch"));
                }
                let profile = self
                    .state
                    .factory_profiles
                    .get(&spec.factory_id)
                    .ok_or_else(|| denied("domain_quote_factory_profile_missing"))?;
                let tags = |values: &[String]| {
                    values
                        .iter()
                        .map(|v| v.trim().to_ascii_lowercase())
                        .filter(|v| !v.is_empty())
                        .collect::<std::collections::BTreeSet<_>>()
                };
                if profile.factory_id != canonical.factory_id
                    || profile.tier != canonical.tier
                    || profile.recipe_slots != canonical.recipe_slots
                    || tags(&profile.tags) != tags(&canonical.tags)
                {
                    return Err(denied("domain_quote_factory_profile_mismatch"));
                }
                power.electricity_amount
            }
            Action::ScheduleRecipe {
                recipe_id, plan, ..
            } => {
                let canonical =
                    crate::builtin_recipe_plan(recipe_id, plan.accepted_batches)
                        .ok_or_else(|| denied("domain_quote_unsupported"))?;
                if *plan != canonical {
                    return Err(denied("domain_quote_plan_mismatch"));
                }
                canonical.power_required
            }
            // Module-produced plans require their actual native plan at the
            // execution seam. Never trust a desired-batch request as a quote.
            _ => return Err(denied("domain_quote_unsupported")),
        };
        u64::try_from(cost).map_err(|_| denied("domain_quote_invalid"))
    }

    /// Production host binding: selects current scope and derives cost from
    /// the exact native action. No caller/provider accounting is accepted.
    pub(crate) fn bind_agent_causal_decision(
        &mut self,
        request_id: &str,
        agent_id: &str,
        action: &Action,
        context: AgentDecisionCausalContextV1,
    ) -> Result<(), WorldError> {
        let result =
            self.bind_agent_causal_decision_inner(request_id, agent_id, action, context.clone());
        if let Err(error) = &result {
            self.persist_agent_delegation_denial(agent_id, &context, error)?;
        }
        result
    }

    fn bind_agent_causal_decision_inner(
        &mut self,
        request_id: &str,
        agent_id: &str,
        action: &Action,
        mut context: AgentDecisionCausalContextV1,
    ) -> Result<(), WorldError> {
        let ledger = self.delegation_ledger()?;
        let scoped = action_scope(action);
        let grants: Vec<_> = ledger
            .grants
            .values()
            .filter(|g| {
                g.agent_id == agent_id
                    && scoped.as_ref().is_some_and(|(kind, object)| {
                        &g.object_id == object && g.action_kinds.iter().any(|v| v == *kind)
                    })
            })
            .collect();
        if grants.len() > 1 {
            return Err(denied("grant_selection_ambiguous"));
        }
        let grant = grants.first().copied();
        if grant.is_none() && ledger.grants.values().any(|g| g.agent_id == agent_id) {
            return Err(denied("grant_scope_mismatch"));
        }
        let action_digest =
            cognition_digest_v1("oasis7.cognition.action.v1", &serde_json::to_value(action)?);
        self.apply_current_owner_control(&ledger, grant, &mut context)?;
        let decision = AgentDelegationDecisionV1 {
            decision_request_id: request_id.into(),
            agent_id: agent_id.into(),
            action_digest: action_digest.clone(),
            grant_id: grant.map(|g| g.grant_id.clone()),
            grant_revision: grant.map(|g| g.revision),
            quote_id: grant.map(|_| format!("native-electricity:{action_digest}")),
            quoted_units: if grant.is_some() {
                self.native_delegation_quote(action)?
            } else {
                0
            },
            quote_valid_until_tick: self.state.time.saturating_add(1),
            context,
        };
        self.bind_agent_delegation_decision(decision, action)
    }

    pub(super) fn prepare_agent_delegation_commit(
        &mut self,
        request_id: &str,
        action: &Action,
    ) -> Result<(), WorldError> {
        let ledger = self.delegation_ledger()?;
        if let Some(decision) = ledger.decisions.get(request_id) {
            if decision.action_digest
                != cognition_digest_v1("oasis7.cognition.action.v1", &serde_json::to_value(action)?)
            {
                return Err(denied("decision_action_changed"));
            }
            self.validate_delegation_decision(decision, action)?;
            self.cognition["agent_delegation_active_decision"] = json!(request_id);
        } else if ledger
            .grants
            .values()
            .any(|g| action.actor_id() == Some(g.agent_id.as_str()))
        {
            return Err(denied("decision_binding_required"));
        }
        Ok(())
    }

    pub(super) fn delegation_action_admission(&self, action: &Action) -> Result<(), WorldError> {
        let ledger = self.delegation_ledger()?;
        if !ledger
            .grants
            .values()
            .any(|g| action.actor_id() == Some(g.agent_id.as_str()))
        {
            return Ok(());
        }
        let request_id = self
            .cognition
            .get("agent_delegation_active_decision")
            .and_then(Value::as_str)
            .ok_or_else(|| denied("decision_binding_required"))?;
        let decision = ledger
            .decisions
            .get(request_id)
            .ok_or_else(|| denied("decision_binding_required"))?;
        if decision.action_digest
            != cognition_digest_v1("oasis7.cognition.action.v1", &serde_json::to_value(action)?)
        {
            return Err(denied("decision_action_changed"));
        }
        self.validate_delegation_decision(decision, action)?;
        Ok(())
    }

    /// Read model only. Domain journal and commit marker remain effect truth.
    pub fn agent_causal_receipts(
        &self,
        agent_id: &str,
    ) -> Result<Vec<AgentCausalReceiptV1>, WorldError> {
        let ledger = self.delegation_ledger()?;
        let mut receipts: Vec<_> = ledger
            .receipts
            .values()
            .filter(|r| r.agent_id == agent_id)
            .cloned()
            .collect();
        receipts.sort_by_key(|r| r.action_id);
        for receipt in &receipts {
            let lineage = self.read_runtime_receipt_lineage(&receipt.receipt_id)?;
            if lineage.agent_id != receipt.agent_id
                || lineage.action_id != format!("action:{}", receipt.action_id)
            {
                return Err(denied("receipt_lineage_conflict"));
            }
            let actual: Result<Vec<Value>, WorldError> = receipt
                .domain_event_refs
                .iter()
                .map(|id| {
                    let event = self
                        .journal
                        .events
                        .iter()
                        .find(|event| event.id == *id)
                        .ok_or_else(|| denied("domain_reference_unavailable"))?;
                    if event.caused_by != Some(crate::CausedBy::Action(receipt.action_id))
                    {
                        return Err(denied("domain_reference_action_mismatch"));
                    }
                    match &event.body {
                        WorldEventBody::Domain(domain) => {
                            serde_json::to_value(domain).map_err(WorldError::from)
                        }
                        _ => Err(denied("domain_reference_invalid")),
                    }
                })
                .collect();
            if json!(actual?) != receipt.actual_consequence {
                return Err(denied("domain_projection_conflict"));
            }
        }
        Ok(receipts)
    }

    pub fn agent_delegation_authorizations(
        &self,
        agent_id: &str,
    ) -> Result<Vec<AgentDelegationAuthorizationV1>, WorldError> {
        let ledger = self.delegation_ledger()?;
        ledger
            .grants
            .values()
            .filter(|g| g.agent_id == agent_id)
            .map(|grant| {
                let spent = *ledger.spent.get(&budget_key(grant)?).unwrap_or(&0);
                Ok(AgentDelegationAuthorizationV1 {
                    grant: grant.clone(),
                    cost_units: 0,
                    spent_units: spent,
                    remaining_units: grant
                        .limit_units
                        .checked_sub(spent)
                        .ok_or_else(|| denied("budget_corrupt"))?,
                })
            })
            .collect()
    }

    pub(super) fn finish_agent_causal_commit(
        &self,
        next: &mut Value,
        marker: &WorldCommitRecordV1,
        action: &Action,
        events: &[crate::WorldEvent],
    ) -> Result<(), WorldError> {
        let mut ledger = self.delegation_ledger()?;
        let decision = ledger.decisions.get(&marker.decision_request_id).cloned();
        let context = decision
            .as_ref()
            .map(|d| d.context.clone())
            .unwrap_or_default();
        let authorization = decision
            .as_ref()
            .map(|d| self.validate_delegation_decision(d, action))
            .transpose()?
            .flatten();
        let action_id = marker
            .action_id
            .strip_prefix("action:")
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| denied("action_id_invalid"))?;
        let mut references = Vec::new();
        let mut actual = Vec::new();
        let mut rejection = None;
        let mut hard_boundary = None;
        for event in events {
            if event.caused_by != Some(crate::CausedBy::Action(action_id)) {
                continue;
            }
            if let WorldEventBody::Domain(domain) = &event.body {
                let relevant = match domain {
                    DomainEvent::AgentMoved { agent_id, .. } => {
                        action.actor_id() == Some(agent_id.as_str())
                    }
                    DomainEvent::MaterialTransferred { transfer_id, .. } => {
                        *transfer_id == Some(action_id)
                    }
                    DomainEvent::MaterialTransitStarted { job_id, .. } => *job_id == action_id,
                    DomainEvent::RecipeStarted { job_id, .. } => *job_id == action_id,
                    DomainEvent::FactoryBuildStarted { job_id, .. } => *job_id == action_id,
                    DomainEvent::ActionRejected {
                        action_id: id,
                        reason,
                    } if *id == action_id => {
                        rejection = Some(format!("{reason:?}"));
                        if matches!(reason, crate::RejectReason::RuleDenied { .. }) {
                            hard_boundary = rejection.clone();
                        }
                        true
                    }
                    _ => false,
                };
                if relevant {
                    references.push(event.id);
                    actual.push(serde_json::to_value(domain)?);
                }
            }
        }
        // A committed cognition receipt means its transaction settled, not
        // necessarily that a domain action produced the expected effect.
        let applied = rejection.is_none() && !actual.is_empty();
        let supported = matches!(
            action,
            Action::MoveAgent { .. }
                | Action::BuildFactory { .. }
                | Action::BuildFactoryWithModule { .. }
                | Action::TransferMaterial { .. }
                | Action::ScheduleRecipe { .. }
                | Action::ScheduleRecipeWithModule { .. }
        );
        let mut authorization = authorization;
        if applied && let Some(auth) = authorization.as_mut() {
            auth.spent_units = auth
                .spent_units
                .checked_add(auth.cost_units)
                .ok_or_else(|| denied("budget_overflow"))?;
            auth.remaining_units = auth
                .grant
                .limit_units
                .checked_sub(auth.spent_units)
                .ok_or_else(|| denied("budget_exhausted"))?;
            ledger
                .spent
                .insert(budget_key(&auth.grant)?, auth.spent_units);
        }
        let intent = context
            .intent_id
            .as_ref()
            .and_then(|id| self.state.agent_intent_ledger.get(id));
        for control_id in &context.owner_control_refs {
            if let Some(control) = ledger.controls.get_mut(control_id) {
                control.consumed_receipt_id = Some(marker.receipt_id.clone());
            }
        }
        let effect_intent_id = intent.and_then(|i| i.effect_intent_id.clone());
        let receipt = AgentCausalReceiptV1 {
            receipt_id: marker.receipt_id.clone(),
            commit_id: marker.commit_id.clone(),
            agent_id: marker.agent_id.clone(),
            intent_id: context.intent_id,
            effect_intent_id,
            action_id,
            action_kind: serde_json::to_value(action)?
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            domain_event_refs: references,
            disposition: if applied {
                "applied"
            } else if supported || rejection.is_some() {
                "not_applied"
            } else {
                "unavailable"
            }
            .into(),
            primary_reason: rejection.clone().or(context.reason),
            next_step: if applied {
                "observe_domain_result"
            } else {
                "replan_required"
            }
            .into(),
            expected_consequence: context.expected_consequence,
            actual_consequence: json!(actual),
            alternative: context.alternative,
            stakes: context.stakes,
            evidence_refs: context.evidence_refs,
            correction_refs: context.correction_refs,
            interruption_refs: context.interruption_refs,
            authorization,
            dissent: context.dissent,
            override_actor: context.override_actor,
            hard_boundary,
            owner_control_refs: context.owner_control_refs,
        };
        ledger.receipts.insert(marker.receipt_id.clone(), receipt);
        next["agent_delegation"] = serde_json::to_value(ledger)?;
        next.as_object_mut()
            .ok_or_else(|| denied("cognition_invalid"))?
            .remove("agent_delegation_active_decision");
        Ok(())
    }
}
