use super::*;
use crate::AgentIndustrialPhaseV1;
use crate::{AgentClaimCostQuote, MaterialLedgerId};
use std::collections::BTreeMap;

fn claim(owner: &str) -> StarterOcClaimState {
    StarterOcClaimState {
        agent_id: owner.into(),
        player_id: "player".into(),
        public_key: Some("key".into()),
        amount: 0,
        claimed_at: 1,
        source_treasury_bucket_id: None,
    }
}

fn valid_facts<'a>(
    starter_claim: &'a StarterOcClaimState,
    ledger: &'a MaterialLedgerId,
    owner: &'a str,
    location: &'a str,
) -> ProductionEligibilityFactsV1<'a> {
    ProductionEligibilityFactsV1 {
        primary: PrimaryProductionFactsV1 {
            agent_exists: true,
            starter_claim: Some(starter_claim),
        },
        retired: false,
        blocked: false,
        factory: Some(FactoryProductionFactsV1 {
            builder_agent_id: owner,
            builtin_spec_available: true,
            spec_matches_builtin: true,
            recipe_slots: 2,
            durability_ppm: 1,
            input_ledger: ledger,
            site_location_id: Some(location),
        }),
        site: Some(FactorySiteProductionFactsV1 {
            active: true,
            chunk_ready: true,
            location_id: location,
            owner_agent_id: owner,
            owner_authorized: false,
        }),
        anchor_active: Some(true),
        resolved_owner: Some(ResolvedProductionOwnerFactsV1 {
            player_id: "player",
            source_id: "source",
            public_key: Some("key"),
        }),
    }
}

fn order(input_ledger: MaterialLedgerId) -> AgentIndustrialOrderV1 {
    AgentIndustrialOrderV1 {
        order_id: "order".into(),
        owner_id: "owner".into(),
        factory_id: "factory".into(),
        target_agent_id: "target".into(),
        policy_version: 1,
        inputs: BTreeMap::new(),
        confirmed_at: 1,
        ready_at: 2,
        delivered_at: None,
        phase: AgentIndustrialPhaseV1::Reserved,
        quote: AgentManufacturingQuoteV1 {
            policy_version: 1,
            world_binding: None,
            recipe_id: "recipe".into(),
            owner_id: "owner".into(),
            owner_player_id: "player".into(),
            owner_source_id: "source".into(),
            factory_id: "factory".into(),
            quoted_at: 1,
            expires_at: 2,
            quote_digest: String::new(),
            dependency_digest: String::new(),
            delivery_location_id: "location".into(),
            inputs: BTreeMap::new(),
            input_ledger,
            electricity: 0,
            claim_cost: AgentClaimCostQuote {
                reputation_tier: 0,
                claim_cap: 0,
                slot_index: 0,
                activation_fee_amount: 0,
                activation_fee_burn_amount: 0,
                activation_fee_treasury_amount: 0,
                claim_bond_amount: 0,
                upkeep_per_epoch: 0,
                release_cooldown_epochs: 0,
                grace_epochs: 0,
                idle_warning_epochs: 0,
                forced_idle_reclaim_epochs: 0,
                forced_reclaim_penalty_bps: 0,
            },
            upfront_liquid: 0,
            liquid_after_reservation: 0,
            electricity_after_reservation: 0,
            maintenance_per_tick: 0,
        },
        started_at: None,
        produced_at: None,
        blocked_at: None,
        completed_work_ticks: 0,
        last_progress_tick: None,
        terminal_reason: None,
        refund_pending: false,
        refunded_at: None,
    }
}

fn expected(reason: &str) -> Result<(), WorldError> {
    Err(crate::world_rules::invalid(reason))
}

#[test]
fn primary_production_checks_keep_existing_reason_order() {
    let valid_claim = claim("owner");
    assert_eq!(
        validate_primary_production_facts_v1(
            PrimaryProductionFactsV1 {
                agent_exists: false,
                starter_claim: None,
            },
            "owner",
        ),
        expected("primary_bound_agent_missing")
    );
    assert_eq!(
        validate_primary_production_facts_v1(
            PrimaryProductionFactsV1 {
                agent_exists: true,
                starter_claim: None,
            },
            "owner",
        ),
        expected("primary_bound_agent_required")
    );
    let mut invalid_claim = valid_claim.clone();
    invalid_claim.public_key = Some(String::new());
    assert_eq!(
        validate_primary_production_facts_v1(
            PrimaryProductionFactsV1 {
                agent_exists: true,
                starter_claim: Some(&invalid_claim),
            },
            "owner",
        ),
        expected("primary_owner_source_unavailable")
    );
    assert_eq!(
        validate_primary_production_facts_v1(
            PrimaryProductionFactsV1 {
                agent_exists: true,
                starter_claim: Some(&valid_claim),
            },
            "owner",
        ),
        Ok(())
    );
}

#[test]
fn production_facts_keep_existing_factory_site_and_anchor_order() {
    let claim = claim("owner");
    let ledger = MaterialLedgerId::agent("owner");
    let mut facts = valid_facts(&claim, &ledger, "owner", "location");

    facts.primary.agent_exists = false;
    facts.retired = true;
    facts.blocked = true;
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("primary_bound_agent_missing")
    );
    facts.primary.agent_exists = true;
    facts.primary.starter_claim = None;
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("primary_bound_agent_required")
    );
    facts.primary.starter_claim = Some(&claim);
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("factory_retired")
    );
    facts.retired = false;
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("production_authority_blocked")
    );
    facts.blocked = false;
    facts.factory = None;
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("factory_missing")
    );

    let mut factory = valid_facts(&claim, &ledger, "owner", "location")
        .factory
        .unwrap();
    factory.builtin_spec_available = false;
    facts.factory = Some(factory);
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("assembler_policy_unavailable")
    );
    factory.builtin_spec_available = true;
    factory.spec_matches_builtin = false;
    facts.factory = Some(factory);
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("owned_working_assembler_required")
    );
    factory.spec_matches_builtin = true;
    factory.recipe_slots = 1;
    facts.factory = Some(factory);
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("owned_working_assembler_required")
    );
    factory.recipe_slots = 2;
    facts.factory = Some(factory);
    facts.site = None;
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("factory_site_authority_unavailable")
    );
    facts.site = Some(FactorySiteProductionFactsV1 {
        active: true,
        chunk_ready: true,
        location_id: "location",
        owner_agent_id: "owner",
        owner_authorized: false,
    });
    facts.anchor_active = None;
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        expected("factory_location_unavailable")
    );
    facts.anchor_active = Some(true);
    assert_eq!(
        production_eligibility_from_facts_v1(facts, "owner", "factory", "location"),
        Ok(())
    );
}

#[test]
fn order_checks_production_then_input_source_then_resolved_owner() {
    let claim = claim("owner");
    let ledger = MaterialLedgerId::agent("owner");
    let mut facts = valid_facts(&claim, &ledger, "owner", "location");
    let mut order = order(MaterialLedgerId::World);
    facts.primary.agent_exists = false;
    assert_eq!(
        order_production_eligibility_from_facts_v1(facts, &order),
        expected("primary_bound_agent_missing")
    );
    facts.primary.agent_exists = true;
    facts.resolved_owner = None;
    assert_eq!(
        order_production_eligibility_from_facts_v1(facts, &order),
        expected("manufacturing_input_source_changed")
    );
    order.quote.input_ledger = ledger.clone();
    assert_eq!(
        order_production_eligibility_from_facts_v1(facts, &order),
        expected("manufacturing_root_owner_changed")
    );
    facts.resolved_owner = Some(ResolvedProductionOwnerFactsV1 {
        player_id: "player",
        source_id: "source",
        public_key: Some("key"),
    });
    assert_eq!(
        order_production_eligibility_from_facts_v1(facts, &order),
        Ok(())
    );
}
