//! Pure predicates shared by legacy World and Current-backed eligibility readers.
//!
//! These small borrowed facts carry observations only; this module does not
//! mint authority or prove that a caller read a complete/current World. A
//! Current-backed caller must obtain each fact from the exact transaction and
//! perform primary checks before reading the remaining eligibility inputs.
use super::industrial::AgentIndustrialOrderV1;
#[cfg(test)]
use super::industrial::AgentManufacturingQuoteV1;
use crate::{MaterialLedgerId, StarterOcClaimState, WorldError};

/// The minimum inputs needed to preserve `manufacturing_existing_targets`'s
/// initial primary-agent and starter-claim checks.
#[derive(Debug, Clone, Copy)]
pub struct PrimaryProductionFactsV1<'a> {
    pub agent_exists: bool,
    pub starter_claim: Option<&'a StarterOcClaimState>,
}

/// Borrowed factory facts used by the shared production predicate.
#[derive(Debug, Clone, Copy)]
pub struct FactoryProductionFactsV1<'a> {
    pub builder_agent_id: &'a str,
    pub builtin_spec_available: bool,
    pub spec_matches_builtin: bool,
    /// Losslessly widened from `FactoryModuleSpec::recipe_slots` (`u16`).
    pub recipe_slots: u32,
    pub durability_ppm: i64,
    pub input_ledger: &'a MaterialLedgerId,
    pub site_location_id: Option<&'a str>,
}

/// The location-specific site facts read for one factory eligibility check.
#[derive(Debug, Clone, Copy)]
pub struct FactorySiteProductionFactsV1<'a> {
    pub active: bool,
    pub chunk_ready: bool,
    pub location_id: &'a str,
    pub owner_agent_id: &'a str,
    /// True when the owner’s authorized-agent set contains the requested owner.
    pub owner_authorized: bool,
}

/// The already-resolved owner tuple needed for the quote binding comparison.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedProductionOwnerFactsV1<'a> {
    pub player_id: &'a str,
    pub source_id: &'a str,
    pub public_key: Option<&'a str>,
}

/// Observations required by production and order-production eligibility.
#[derive(Debug, Clone, Copy)]
pub struct ProductionEligibilityFactsV1<'a> {
    pub primary: PrimaryProductionFactsV1<'a>,
    pub retired: bool,
    pub blocked: bool,
    pub factory: Option<FactoryProductionFactsV1<'a>>,
    pub site: Option<FactorySiteProductionFactsV1<'a>>,
    pub anchor_active: Option<bool>,
    pub resolved_owner: Option<ResolvedProductionOwnerFactsV1<'a>>,
}

/// Preserve the first checks and exact error reasons of
/// `manufacturing_existing_targets` without materializing its target set.
/// Call this before fetching factory, site, anchor, or owner facts.
pub fn validate_primary_production_facts_v1(
    primary: PrimaryProductionFactsV1<'_>,
    owner: &str,
) -> Result<(), WorldError> {
    if !primary.agent_exists {
        return Err(super::invalid("primary_bound_agent_missing"));
    }
    let claim = primary
        .starter_claim
        .ok_or_else(|| super::invalid("primary_bound_agent_required"))?;
    if claim.agent_id != owner
        || claim.player_id.is_empty()
        || claim.public_key.as_ref().is_none_or(|key| key.is_empty())
    {
        return Err(super::invalid("primary_owner_source_unavailable"));
    }
    Ok(())
}

/// Apply the legacy production eligibility checks in their original order.
/// Success is only a predicate result over caller-supplied facts, not a
/// capability token or independent business qualification.
///
/// The caller must source `factory` from the exact lookup for `factory_id`.
/// This narrow fact type intentionally carries no factory identity, so this
/// predicate cannot verify that storage binding itself.
pub fn production_eligibility_from_facts_v1(
    facts: ProductionEligibilityFactsV1<'_>,
    owner: &str,
    _factory_id: &str,
    location: &str,
) -> Result<(), WorldError> {
    validate_primary_production_facts_v1(facts.primary, owner)?;
    if facts.retired {
        return Err(super::invalid("factory_retired"));
    }
    if facts.blocked {
        return Err(super::invalid("production_authority_blocked"));
    }
    let factory = facts
        .factory
        .ok_or_else(|| super::invalid("factory_missing"))?;
    if !factory.builtin_spec_available {
        return Err(super::invalid("assembler_policy_unavailable"));
    }
    if factory.builder_agent_id != owner
        || !factory.spec_matches_builtin
        || factory.recipe_slots < 2
        || factory.durability_ppm <= 0
    {
        return Err(super::invalid("owned_working_assembler_required"));
    }
    let site = facts
        .site
        .ok_or_else(|| super::invalid("factory_site_authority_unavailable"))?;
    let anchor_active = facts
        .anchor_active
        .ok_or_else(|| super::invalid("factory_location_unavailable"))?;
    if !site.active
        || !site.chunk_ready
        || site.location_id != location
        || !anchor_active
        || factory.site_location_id != Some(location)
        || !(site.owner_agent_id == owner || site.owner_authorized)
    {
        return Err(super::invalid("factory_authority_changed"));
    }
    Ok(())
}

/// Apply order-specific checks after the shared production checks, preserving
/// the input-source error before the resolved-owner error.
pub fn order_production_eligibility_from_facts_v1(
    facts: ProductionEligibilityFactsV1<'_>,
    order: &AgentIndustrialOrderV1,
) -> Result<(), WorldError> {
    production_eligibility_from_facts_v1(
        facts,
        &order.owner_id,
        &order.factory_id,
        &order.quote.delivery_location_id,
    )?;
    let factory = facts
        .factory
        .ok_or_else(|| super::invalid("factory_missing"))?;
    if factory.input_ledger != &order.quote.input_ledger {
        return Err(super::invalid("manufacturing_input_source_changed"));
    }
    if !facts.resolved_owner.is_some_and(|owner| {
        owner.player_id == order.quote.owner_player_id
            && owner.source_id == order.quote.owner_source_id
            && owner.public_key.is_some_and(|key| !key.is_empty())
    }) {
        return Err(super::invalid("manufacturing_root_owner_changed"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "industrial_eligibility_kernel_tests.rs"]
mod tests;
