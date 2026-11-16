//! Original native delegation read model. Work callbacks carry only resource
//! observations; all selected grants, budget keys and allowance predicates are
//! evaluated here, shared by World and the fixed Current projection consumer.
use super::*;
use std::{
    io::{self, Write},
    mem::size_of,
};
pub(in crate::world) mod sealed {
    pub trait Work {}
}
pub(in crate::world) trait Work: sealed::Work {
    type Error: From<WorldError>;
    fn work(&mut self, steps: u64, bytes: usize) -> Result<(), Self::Error>;
    fn ownership(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn before_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>;
}
pub(in crate::world) struct LegacyWork;
impl sealed::Work for LegacyWork {}
impl Work for LegacyWork {
    type Error = WorldError;
    fn work(&mut self, _: u64, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn ownership(&mut self, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_value<T: Serialize + ?Sized>(&mut self, _: &T) -> Result<(), WorldError> {
        Ok(())
    }
}
struct Output<'a, W: Work> {
    work: &'a mut W,
    bytes: Vec<u8>,
    first: Option<W::Error>,
}
impl<W: Work> Write for Output<'_, W> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let result = (|| {
            for part in input.chunks(8192) {
                self.work.work(1, part.len())?;
                let end = self
                    .bytes
                    .len()
                    .checked_add(part.len())
                    .ok_or_else(|| denied("native delegation JSON length"))?;
                if end > self.bytes.capacity() {
                    let capacity = end
                        .checked_next_power_of_two()
                        .ok_or_else(|| denied("native delegation JSON capacity"))?;
                    self.work.ownership(capacity)?;
                    self.bytes
                        .try_reserve_exact(capacity - self.bytes.len())
                        .map_err(|_| denied("native delegation JSON allocation"))?;
                    if self.bytes.capacity() > capacity {
                        return Err(denied("native delegation JSON allocation capacity").into());
                    }
                }
                self.bytes.extend_from_slice(part);
            }
            Ok(())
        })();
        result.map(|()| input.len()).map_err(|error| {
            self.first = Some(error);
            io::ErrorKind::Other.into()
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(in crate::world) fn budget_key_v1<W: Work>(
    grant: &AgentDelegationGrantV1,
    work: &mut W,
) -> Result<String, W::Error> {
    work.ownership(size_of::<String>())?;
    let mut output = Output {
        work,
        bytes: Vec::new(),
        first: None,
    };
    let result = serde_json::to_writer(
        &mut output,
        &(
            &grant.grant_id,
            &grant.source_id,
            &grant.object_id,
            &grant.period_id,
        ),
    );
    if let Some(error) = output.first {
        return Err(error);
    }
    result.map_err(WorldError::from)?;
    // serde_json's writer emits UTF8 from the original String tuple only.
    Ok(String::from_utf8(output.bytes).expect("native original JSON UTF8"))
}
pub(in crate::world) fn authorization_v1<W: Work>(
    ledger: &Ledger,
    grant: &AgentDelegationGrantV1,
    work: &mut W,
) -> Result<AgentDelegationAuthorizationV1, W::Error> {
    let key = budget_key_v1(grant, work)?;
    let comparisons = 1u64 + 11 * (ledger.spent.len().saturating_add(1)).ilog2() as u64;
    let compare_bytes = key
        .len()
        .checked_mul(usize::try_from(comparisons).map_err(|_| denied("budget_key_work"))?)
        .ok_or_else(|| denied("budget_key_work"))?;
    work.work(comparisons, compare_bytes)?;
    let spent = *ledger.spent.get(&key).unwrap_or(&0);
    work.before_value(grant)?;
    // Retain the established clone-before-subtraction error order.
    Ok(AgentDelegationAuthorizationV1 {
        grant: grant.clone(),
        cost_units: 0,
        spent_units: spent,
        remaining_units: grant
            .limit_units
            .checked_sub(spent)
            .ok_or_else(|| denied("budget_corrupt"))?,
    })
}
pub(in crate::world) fn authorizations_v1<W: Work>(
    ledger: &Ledger,
    agent: &str,
    work: &mut W,
) -> Result<Vec<AgentDelegationAuthorizationV1>, W::Error> {
    work.ownership(size_of::<Vec<AgentDelegationAuthorizationV1>>())?;
    let mut out = Vec::new();
    for grant in ledger.grants.values() {
        work.work(1, grant.agent_id.len().saturating_add(agent.len()))?;
        if grant.agent_id != agent {
            continue;
        }
        let value = authorization_v1(ledger, grant, work)?;
        if out.len() == out.capacity() {
            let capacity = out.capacity().checked_mul(2).unwrap_or(usize::MAX).max(4);
            let bytes = capacity
                .checked_mul(size_of::<AgentDelegationAuthorizationV1>())
                .ok_or_else(|| denied("authorization_layout"))?;
            work.ownership(bytes)?;
            out.try_reserve_exact(capacity - out.len())
                .map_err(|_| denied("authorization_allocation"))?;
            if out.capacity() > capacity {
                return Err(denied("authorization_capacity").into());
            }
        }
        out.push(value);
    }
    Ok(out)
}

/// Exact original causal-binding selection. Revoked scoped records remain
/// evidence, but cannot compete with an active grant; expiry/spending are
/// intentionally checked later and cannot trigger selection of another grant.
pub(in crate::world) fn select_causal_grant_v1<'a, W: Work>(
    ledger: &'a Ledger,
    agent: &str,
    action: &Action,
    work: &mut W,
) -> Result<Option<&'a AgentDelegationGrantV1>, W::Error> {
    work.before_value(action)?;
    let scoped = action_scope(action);
    let mut had_scoped = false;
    let mut any_agent = false;
    let mut selected = None;
    let mut count = 0usize;
    for grant in ledger.grants.values() {
        work.work(1, grant.agent_id.len().saturating_add(agent.len()))?;
        if grant.agent_id != agent {
            continue;
        }
        any_agent = true;
        let Some((kind, object)) = &scoped else {
            continue;
        };
        work.work(1, grant.object_id.len().saturating_add(object.len()))?;
        if &grant.object_id != object {
            continue;
        }
        let mut kind_matches = false;
        for entry in &grant.action_kinds {
            work.work(1, entry.len().saturating_add(kind.len()))?;
            if entry.as_str() == *kind {
                kind_matches = true;
                break;
            }
        }
        if !kind_matches {
            continue;
        }
        had_scoped = true;
        if grant.revoked {
            continue;
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| denied("grant_selection_ambiguous"))?;
        if selected.is_none() {
            selected = Some(grant)
        }
    }
    if had_scoped && count == 0 {
        return Err(denied("grant_revoked").into());
    }
    if count > 1 {
        return Err(denied("grant_selection_ambiguous").into());
    }
    if selected.is_none() && any_agent {
        return Err(denied("grant_scope_mismatch").into());
    }
    Ok(selected)
}

pub(in crate::world) trait GrantSource: Work {
    fn owner(&mut self, agent: &str) -> Result<Option<crate::ResolvedAgentOwnerV1>, Self::Error>;
    fn agent_present(&mut self, agent: &str) -> Result<bool, Self::Error>;
}
pub(in crate::world) fn validate_grant_source_v1<W: GrantSource>(
    source: &mut W,
    grant: &AgentDelegationGrantV1,
) -> Result<(), W::Error> {
    if grant.organization_id.is_some() {
        return Err(denied("organization_source_unsupported").into());
    }
    let owner = source
        .owner(&grant.agent_id)?
        .ok_or_else(|| denied("owner_source_unavailable"))?;
    source.work(
        3,
        owner
            .player_id
            .len()
            .saturating_add(grant.owner_id.len())
            .saturating_add(grant.issuer_id.len())
            .saturating_add(grant.source_id.len())
            .saturating_add(owner.source_id.len()),
    )?;
    if owner.player_id != grant.owner_id
        || grant.issuer_id != grant.owner_id
        || grant.source_id != owner.source_id
    {
        return Err(denied("owner_source_changed").into());
    }
    if !source.agent_present(&grant.agent_id)? {
        return Err(denied("agent_missing").into());
    }
    Ok(())
}
pub(super) struct LegacySource<'a>(pub(super) &'a World);
impl sealed::Work for LegacySource<'_> {}
impl Work for LegacySource<'_> {
    type Error = WorldError;
    fn work(&mut self, _: u64, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn ownership(&mut self, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_value<T: Serialize + ?Sized>(&mut self, _: &T) -> Result<(), WorldError> {
        Ok(())
    }
}
impl GrantSource for LegacySource<'_> {
    fn owner(&mut self, agent: &str) -> Result<Option<crate::ResolvedAgentOwnerV1>, WorldError> {
        Ok(self.0.current_agent_owner(agent))
    }
    fn agent_present(&mut self, agent: &str) -> Result<bool, WorldError> {
        Ok(self.0.state.agents.contains_key(agent))
    }
}

/// The built-in quote always uses this exact original empty causal context;
/// ordinary decisions carrying intent, dissent or owner controls keep the
/// complete original validator and cannot enter this fixed preview kernel.
pub(in crate::world) fn is_default_recipe_decision_v1(
    decision: &AgentDelegationDecisionV1,
    action: &Action,
) -> bool {
    let c = &decision.context;
    matches!(action, Action::ScheduleRecipe { .. })
        && c.intent_id.is_none()
        && c.expected_consequence.is_null()
        && c.alternative.is_null()
        && c.stakes.is_null()
        && c.reason.is_none()
        && c.evidence_refs.is_empty()
        && c.correction_refs.is_empty()
        && c.interruption_refs.is_empty()
        && c.dissent.is_none()
        && c.override_actor.is_none()
        && c.owner_control_refs.is_empty()
}
pub(in crate::world) fn validate_default_recipe_decision_v1<S: GrantSource>(
    source: &mut S,
    ledger: &Ledger,
    decision: &AgentDelegationDecisionV1,
    action: &Action,
    now: u64,
) -> Result<Option<AgentDelegationAuthorizationV1>, S::Error> {
    if !is_default_recipe_decision_v1(decision, action) {
        return Err(denied("decision_identity_mismatch").into());
    }
    let Some(grant_id) = &decision.grant_id else {
        for grant in ledger.grants.values() {
            source.work(
                1,
                grant.agent_id.len().saturating_add(decision.agent_id.len()),
            )?;
            if grant.agent_id == decision.agent_id {
                return Err(denied("grant_binding_required").into());
            }
        }
        return Ok(None);
    };
    source.work(
        1 + 11 * (ledger.grants.len().saturating_add(1)).ilog2() as u64,
        grant_id
            .len()
            .saturating_mul(1 + 11 * (ledger.grants.len().saturating_add(1)).ilog2() as usize),
    )?;
    let grant = ledger
        .grants
        .get(grant_id)
        .ok_or_else(|| denied("grant_missing"))?;
    validate_grant_source_v1(source, grant)?;
    if grant.revoked {
        return Err(denied("grant_revoked").into());
    }
    if grant.revision != decision.grant_revision.unwrap_or(0) {
        return Err(denied("grant_revision_stale").into());
    }
    if now < grant.valid_from_tick
        || now > grant.valid_until_tick
        || now > decision.quote_valid_until_tick
    {
        return Err(denied("grant_or_quote_expired").into());
    }
    let Action::ScheduleRecipe {
        factory_id,
        recipe_id,
        plan,
        ..
    } = action
    else {
        return Err(denied("decision_identity_mismatch").into());
    };
    source.work(
        2,
        grant
            .agent_id
            .len()
            .saturating_add(decision.agent_id.len())
            .saturating_add(grant.object_id.len())
            .saturating_add(factory_id.len()),
    )?;
    if grant.agent_id != decision.agent_id || grant.object_id != *factory_id {
        return Err(denied("grant_scope_mismatch").into());
    }
    let mut kind = false;
    for value in &grant.action_kinds {
        source.work(1, value.len() + "ScheduleRecipe".len())?;
        if value == "ScheduleRecipe" {
            kind = true;
            break;
        }
    }
    if !kind {
        return Err(denied("grant_scope_mismatch").into());
    }
    let key = budget_key_v1(grant, source)?;
    let comparisons = 1 + 11 * (ledger.spent.len().saturating_add(1)).ilog2() as u64;
    source.work(comparisons, key.len().saturating_mul(comparisons as usize))?;
    let spent = *ledger.spent.get(&key).unwrap_or(&0);
    let remaining = grant
        .limit_units
        .checked_sub(spent)
        .ok_or_else(|| denied("budget_corrupt"))?;
    let resource = std::cell::RefCell::new(&mut *source);
    let canonical = oasis7_agent_api::world_quote::builtin_recipe_plan_with_work_v1(
        recipe_id,
        plan.accepted_batches,
        &mut |s, b, h| {
            let mut source = resource.borrow_mut();
            source.work(s, b)?;
            if h > 0 {
                source.ownership(h)?
            }
            Ok::<_, S::Error>(())
        },
    )?
    .ok_or_else(|| denied("domain_quote_unsupported"))?;
    drop(resource);
    source.before_value(plan)?;
    if *plan != canonical {
        return Err(denied("domain_quote_plan_mismatch").into());
    }
    let cost =
        u64::try_from(canonical.power_required).map_err(|_| denied("domain_quote_invalid"))?;
    if decision.quoted_units != cost {
        return Err(denied("quote_units_mismatch").into());
    }
    if decision.quoted_units > remaining {
        return Err(denied("budget_exhausted").into());
    }
    source.before_value(grant)?;
    Ok(Some(AgentDelegationAuthorizationV1 {
        grant: grant.clone(),
        cost_units: decision.quoted_units,
        spent_units: spent,
        remaining_units: remaining,
    }))
}

fn copy_text_v1<W: Work>(value: &str, work: &mut W) -> Result<String, W::Error> {
    work.ownership(std::mem::size_of::<String>().saturating_add(value.len()))?;
    let mut out = Vec::with_capacity(value.len());
    for chunk in value.as_bytes().chunks(8192) {
        work.work(1, chunk.len())?;
        out.extend_from_slice(chunk)
    }
    Ok(String::from_utf8(out).expect("original native UTF8"))
}
/// Readonly equivalent of the original default-context native causal bind.
/// Only original ledger records select a grant; the canonical SDK produces
/// the actual action digest. This does not install or debit a decision.
pub(in crate::world) fn build_default_recipe_decision_v1<W, A>(
    ledger: &Ledger,
    request: &str,
    agent: &str,
    action: &Action,
    now: u64,
    work: &mut W,
    canonical: &A,
    stream_error: &mut impl FnMut(oasis7_wasm_abi::CanonicalCborStreamError) -> W::Error,
) -> Result<AgentDelegationDecisionV1, W::Error>
where
    W: GrantSource,
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
{
    if !matches!(action, Action::ScheduleRecipe { .. }) {
        return Err(denied("action_unsupported").into());
    }
    let grant = select_causal_grant_v1(ledger, agent, action, work)?;
    work.before_value(action)?;
    let payload = serde_json::to_value(action).map_err(WorldError::from)?;
    let digest = oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(
        "oasis7.cognition.action.v1",
        &payload,
        canonical,
    )
    .map_err(stream_error)?;
    let decision_request_id = copy_text_v1(request, work)?;
    let agent_id = copy_text_v1(agent, work)?;
    let action_digest = copy_text_v1(digest.as_str(), work)?;
    let grant_id = grant
        .map(|grant| copy_text_v1(&grant.grant_id, work))
        .transpose()?;
    let grant_revision = grant.map(|grant| grant.revision);
    let quote_id = if grant.is_some() {
        let prefix = "native-electricity:";
        work.ownership(std::mem::size_of::<String>() + prefix.len() + digest.as_str().len())?;
        work.work(1, prefix.len() + digest.as_str().len())?;
        Some(format!("{prefix}{}", digest.as_str()))
    } else {
        None
    };
    let quoted_units = if grant.is_some() {
        let Action::ScheduleRecipe {
            recipe_id, plan, ..
        } = action
        else {
            return Err(denied("action_unsupported").into());
        };
        let canonical_plan = oasis7_agent_api::world_quote::builtin_recipe_plan_with_work_v1(
            recipe_id,
            plan.accepted_batches,
            &mut |s, b, h| {
                work.work(s, b)?;
                if h > 0 {
                    work.ownership(h)?
                }
                Ok::<_, W::Error>(())
            },
        )?
        .ok_or_else(|| denied("domain_quote_unsupported"))?;
        work.before_value(plan)?;
        if *plan != canonical_plan {
            return Err(denied("domain_quote_plan_mismatch").into());
        }
        u64::try_from(canonical_plan.power_required).map_err(|_| denied("domain_quote_invalid"))?
    } else {
        0
    };
    Ok(AgentDelegationDecisionV1 {
        decision_request_id,
        agent_id,
        action_digest,
        grant_id,
        grant_revision,
        quote_id,
        quoted_units,
        quote_valid_until_tick: now.saturating_add(1),
        context: AgentDecisionCausalContextV1::default(),
    })
}
pub(in crate::world) fn validate_decision_replay_v1<W: Work>(
    ledger: &Ledger,
    decision: &AgentDelegationDecisionV1,
    work: &mut W,
) -> Result<(), W::Error> {
    let comparisons = 1 + 11 * (ledger.decisions.len().saturating_add(1)).ilog2() as u64;
    work.work(
        comparisons,
        decision
            .decision_request_id
            .len()
            .saturating_mul(comparisons as usize),
    )?;
    if let Some(old) = ledger.decisions.get(&decision.decision_request_id) {
        work.before_value(old)?;
        work.before_value(decision)?;
        if serde_json::to_value(old).map_err(WorldError::from)?
            != serde_json::to_value(decision).map_err(WorldError::from)?
        {
            return Err(denied("decision_idempotency_conflict").into());
        }
    }
    Ok(())
}

pub(in crate::world) fn validate_decision_identity_v1<W, A>(
    decision: &AgentDelegationDecisionV1,
    action: &Action,
    work: &mut W,
    canonical: &A,
    stream_error: &mut impl FnMut(oasis7_wasm_abi::CanonicalCborStreamError) -> W::Error,
) -> Result<(), W::Error>
where
    W: Work,
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
{
    work.work(
        2,
        decision
            .decision_request_id
            .len()
            .saturating_add(decision.agent_id.len()),
    )?;
    if decision.decision_request_id.trim().is_empty()
        || action.actor_id() != Some(decision.agent_id.as_str())
    {
        return Err(denied("decision_identity_mismatch").into());
    }
    work.before_value(action)?;
    let payload = serde_json::to_value(action).map_err(WorldError::from)?;
    let digest = oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(
        "oasis7.cognition.action.v1",
        &payload,
        canonical,
    )
    .map_err(stream_error)?;
    work.work(
        1,
        decision
            .action_digest
            .len()
            .saturating_add(digest.as_str().len()),
    )?;
    if decision.action_digest != digest.as_str() {
        return Err(denied("decision_identity_mismatch").into());
    }
    if decision.grant_id.is_some() {
        if let Some(quote) = &decision.quote_id {
            work.work(1, quote.len())?;
            if quote.trim().is_empty() {
                return Err(denied("trusted_quote_required").into());
            }
        } else {
            return Err(denied("trusted_quote_required").into());
        }
    }
    Ok(())
}

/// A private readonly preview binding borrowing its actual immutable World
/// and action. It cannot be created from a grant DTO or reused on another
/// World. It neither saves a decision nor activates a mutable delegation slot.
pub(in crate::world) struct NativeRecipePreviewBindingV1<'a> {
    world: &'a World,
    action: &'a Action,
    decision: AgentDelegationDecisionV1,
    authorization: AgentDelegationAuthorizationV1,
}
impl NativeRecipePreviewBindingV1<'_> {
    pub(in crate::world) fn authorization(&self) -> &AgentDelegationAuthorizationV1 {
        &self.authorization
    }
    pub(in crate::world) fn validate(
        &self,
        world: &World,
        action: &Action,
    ) -> Result<(), WorldError> {
        if !std::ptr::eq(self.world, world) || action != self.action {
            return Err(denied("decision_action_changed"));
        }
        validate_decision_identity_v1(
            &self.decision,
            action,
            &mut LegacyWork,
            &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
            &mut |error| WorldError::DistributedValidationFailed {
                reason: format!("native action digest: {error:?}"),
            },
        )?;
        world.validate_delegation_decision(&self.decision, action)?;
        Ok(())
    }
}
pub(in crate::world) fn prepare_recipe_preview_binding_v1<'a>(
    world: &'a World,
    agent: &str,
    action: &'a Action,
) -> Result<NativeRecipePreviewBindingV1<'a>, WorldError> {
    let ledger = world.delegation_ledger()?;
    let mut error = |error| WorldError::DistributedValidationFailed {
        reason: format!("native action digest: {error:?}"),
    };
    let decision = build_default_recipe_decision_v1(
        &ledger,
        "builtin-recipe-read-only-preview",
        agent,
        action,
        world.state.time,
        &mut LegacySource(world),
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
        &mut error,
    )?;
    validate_decision_identity_v1(
        &decision,
        action,
        &mut LegacyWork,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
        &mut error,
    )?;
    world.validate_delegation_decision(&decision, action)?;
    validate_decision_replay_v1(&ledger, &decision, &mut LegacyWork)?;
    // The original prepare and bound-authority calls recheck the same immutable
    // records. Preserve those checks without writing the private clone's JSON.
    world.validate_delegation_decision(&decision, action)?;
    let authorization = world
        .validate_delegation_decision(&decision, action)?
        .ok_or_else(|| WorldError::DistributedValidationFailed {
            reason: "native recipe grant required".into(),
        })?;
    Ok(NativeRecipePreviewBindingV1 {
        world,
        action,
        decision,
        authorization,
    })
}
