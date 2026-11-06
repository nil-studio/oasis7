use super::*;
pub(crate) fn fixture_MaterialLedgerId() -> MaterialLedgerId {
    MaterialLedgerId::World
}
pub(crate) fn fixture_SubstitutionProfileRefV1() -> SubstitutionProfileRefV1 {
    SubstitutionProfileRefV1 {
        id: "original:id".into(),
        version: "original:version".into(),
        digest: "original:digest".into(),
    }
}
pub(crate) fn fixture_IndustrialRefV1() -> IndustrialRefV1 {
    IndustrialRefV1 {
        world_binding: "original:world_binding".into(),
        domain: "original:domain".into(),
        schema: "original:schema".into(),
        id: "original:id".into(),
        revision: u64::MAX,
        digest: "original:digest".into(),
    }
}
pub(crate) fn fixture_QualifiedIndustrialLotFactsV1() -> QualifiedIndustrialLotFactsV1 {
    QualifiedIndustrialLotFactsV1 {
        world_binding: "original:world_binding".into(),
        owner_id: "original:owner_id".into(),
        owner_binding_digest: "original:owner_binding_digest".into(),
        source_ledger: fixture_MaterialLedgerId(),
        material: "original:material".into(),
        material_profile: fixture_SubstitutionProfileRefV1(),
        unit: "original:unit".into(),
        spec: "original:spec".into(),
        quality: "original:quality".into(),
        custody: "original:custody".into(),
        provenance_ref: fixture_IndustrialRefV1(),
        quantity: u64::MAX,
        escrow_capacity: u64::MAX,
        expires_at: u64::MAX,
        consensus_height: u64::MAX,
    }
}
pub(crate) fn fixture_GovernanceFinalityCertificate() -> GovernanceFinalityCertificate {
    GovernanceFinalityCertificate {
        proposal_id: u64::MAX,
        manifest_hash: "original:manifest_hash".into(),
        consensus_height: u64::MAX,
        epoch_id: u64::MAX,
        validator_set_hash: "original:validator_set_hash".into(),
        stake_root: "original:stake_root".into(),
        threshold_bps: u16::MAX,
        min_unique_signers: u16::MAX,
        threshold: u16::MAX,
        signatures: BTreeMap::from([("_length".into(), "original:signatures".into())]),
    }
}
pub(crate) fn fixture_IndustrialWorkCommandV1() -> IndustrialWorkCommandV1 {
    IndustrialWorkCommandV1::ImportQualifiedLot {
        facts: Box::new(fixture_QualifiedIndustrialLotFactsV1()),
        certificate: fixture_GovernanceFinalityCertificate(),
    }
}
pub(crate) fn fixture_WorldRuleCommandV1() -> WorldRuleCommandV1 {
    WorldRuleCommandV1::IndustrialWork {
        command: Box::new(fixture_IndustrialWorkCommandV1()),
    }
}
pub(crate) fn fixture_WorldRuleReceiptV1() -> WorldRuleReceiptV1 {
    WorldRuleReceiptV1 {
        request_id: "original:request_id".into(),
        actor_id: "original:actor_id".into(),
        command: fixture_WorldRuleCommandV1(),
        committed_at: u64::MAX,
        result_id: "original:result_id".into(),
    }
}
pub(crate) fn fixture_AgentIndustrialPhaseV1() -> AgentIndustrialPhaseV1 {
    AgentIndustrialPhaseV1::Reserved
}
pub(crate) fn fixture_AgentClaimCostQuote() -> AgentClaimCostQuote {
    AgentClaimCostQuote {
        reputation_tier: u8::MAX,
        claim_cap: u8::MAX,
        slot_index: u8::MAX,
        activation_fee_amount: u64::MAX,
        activation_fee_burn_amount: u64::MAX,
        activation_fee_treasury_amount: u64::MAX,
        claim_bond_amount: u64::MAX,
        upkeep_per_epoch: u64::MAX,
        release_cooldown_epochs: u64::MAX,
        grace_epochs: u64::MAX,
        idle_warning_epochs: u64::MAX,
        forced_idle_reclaim_epochs: u64::MAX,
        forced_reclaim_penalty_bps: u16::MAX,
    }
}
pub(crate) fn fixture_AgentManufacturingQuoteV1() -> AgentManufacturingQuoteV1 {
    AgentManufacturingQuoteV1 {
        policy_version: u32::MAX,
        world_binding: Some("original:world_binding".into()),
        recipe_id: "original:recipe_id".into(),
        owner_id: "original:owner_id".into(),
        owner_player_id: "original:owner_player_id".into(),
        owner_source_id: "original:owner_source_id".into(),
        factory_id: "original:factory_id".into(),
        quoted_at: u64::MAX,
        expires_at: u64::MAX,
        quote_digest: "original:quote_digest".into(),
        dependency_digest: "original:dependency_digest".into(),
        delivery_location_id: "original:delivery_location_id".into(),
        inputs: BTreeMap::from([("_length".into(), i64::MIN)]),
        input_ledger: fixture_MaterialLedgerId(),
        electricity: i64::MIN,
        claim_cost: fixture_AgentClaimCostQuote(),
        upfront_liquid: u64::MAX,
        liquid_after_reservation: u64::MAX,
        electricity_after_reservation: i64::MIN,
        maintenance_per_tick: i64::MIN,
    }
}
pub(crate) fn fixture_AgentIndustrialOrderV1() -> AgentIndustrialOrderV1 {
    AgentIndustrialOrderV1 {
        order_id: "original:order_id".into(),
        owner_id: "original:owner_id".into(),
        factory_id: "original:factory_id".into(),
        target_agent_id: "original:target_agent_id".into(),
        policy_version: u32::MAX,
        inputs: BTreeMap::from([("_length".into(), i64::MIN)]),
        confirmed_at: u64::MAX,
        ready_at: u64::MAX,
        delivered_at: Some(u64::MAX),
        phase: fixture_AgentIndustrialPhaseV1(),
        quote: fixture_AgentManufacturingQuoteV1(),
        started_at: Some(u64::MAX),
        produced_at: Some(u64::MAX),
        blocked_at: Some(u64::MAX),
        completed_work_ticks: u32::MAX,
        last_progress_tick: Some(u64::MAX),
        terminal_reason: Some("original:terminal_reason".into()),
        refund_pending: true,
        refunded_at: Some(u64::MAX),
    }
}
pub(crate) fn fixture_SubstitutionSelectionV1() -> SubstitutionSelectionV1 {
    SubstitutionSelectionV1::SameMaterialSource {
        source_lot: fixture_IndustrialRefV1(),
        source_material: fixture_SubstitutionProfileRefV1(),
    }
}
pub(crate) fn fixture_SubstitutionImpactV1() -> SubstitutionImpactV1 {
    SubstitutionImpactV1 {
        outputs: fixture_SubstitutionProfileRefV1(),
        quality: fixture_SubstitutionProfileRefV1(),
        power: fixture_SubstitutionProfileRefV1(),
        time: fixture_SubstitutionProfileRefV1(),
        byproducts: fixture_SubstitutionProfileRefV1(),
        terminal: fixture_SubstitutionProfileRefV1(),
        value_class: fixture_SubstitutionProfileRefV1(),
        logistics: fixture_SubstitutionProfileRefV1(),
        buffer: fixture_SubstitutionProfileRefV1(),
    }
}
pub(crate) fn fixture_SubstitutionValuationV1() -> SubstitutionValuationV1 {
    SubstitutionValuationV1 {
        unit: "original:unit".into(),
        held: u64::MAX,
        consumed: u64::MAX,
        expected_output: u64::MAX,
    }
}
pub(crate) fn fixture_SubstitutionExecutionAuthorityV1() -> SubstitutionExecutionAuthorityV1 {
    SubstitutionExecutionAuthorityV1 {
        authority_ref: fixture_IndustrialRefV1(),
        owner_id: "original:owner_id".into(),
        owner_binding_digest: "original:owner_binding_digest".into(),
        factory_digest: "original:factory_digest".into(),
        root_ref: fixture_IndustrialRefV1(),
        slot_id: "original:slot_id".into(),
        expected_binding_revision: u64::MAX,
        profile_digest: "original:profile_digest".into(),
        selection: fixture_SubstitutionSelectionV1(),
        target_recipe: "original:target_recipe".into(),
        baseline_impacts: fixture_SubstitutionImpactV1(),
        candidate_impacts: fixture_SubstitutionImpactV1(),
        output_material: "original:output_material".into(),
        output_unit: "original:output_unit".into(),
        output_spec: "original:output_spec".into(),
        output_quality: "original:output_quality".into(),
        output_custody: "original:output_custody".into(),
        owner_ledger_path_policy: fixture_SubstitutionProfileRefV1(),
        input_state: "original:input_state".into(),
        costs: BTreeMap::from([("_length".into(), u64::MAX)]),
        power_cost: u64::MAX,
        duration_ticks: u64::MAX,
        terminal_capacity: u64::MAX,
        valid_until: u64::MAX,
        valuation: fixture_SubstitutionValuationV1(),
        risks: vec!["original:risks".into()],
        next_recheck: u64::MAX,
    }
}
pub(crate) fn fixture_RecipeSubstitutionCommandV1() -> RecipeSubstitutionCommandV1 {
    RecipeSubstitutionCommandV1::PublishAuthority {
        authority: Box::new(fixture_SubstitutionExecutionAuthorityV1()),
        consensus_height: u64::MAX,
        finality_certificate: fixture_GovernanceFinalityCertificate(),
    }
}
pub(crate) fn fixture_EmergencyServiceRegistrationV1() -> EmergencyServiceRegistrationV1 {
    EmergencyServiceRegistrationV1 {
        obligation_id: "original:obligation_id".into(),
        beneficiary_id: "original:beneficiary_id".into(),
        controller_id: "original:controller_id".into(),
        region_id: "original:region_id".into(),
        location_id: "original:location_id".into(),
        registered_at: u64::MAX,
        electricity_per_interval: i64::MIN,
        public_critical: true,
        recipe_ids: BTreeSet::from(["original:recipe_ids".into()]),
        source_ref: "original:source_ref".into(),
    }
}
pub(crate) fn fixture_MinimumServiceCommandV1() -> MinimumServiceCommandV1 {
    MinimumServiceCommandV1::Register {
        registration: fixture_EmergencyServiceRegistrationV1(),
        region_budget_lineage: "original:region_budget_lineage".into(),
        certificate: fixture_GovernanceFinalityCertificate(),
    }
}
