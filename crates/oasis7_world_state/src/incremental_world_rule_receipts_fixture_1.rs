use super::*;
pub(crate) fn fixture_OrganizationContinuityCommandV1() -> OrganizationContinuityCommandV1 {
    OrganizationContinuityCommandV1::Enable {
        organization_id: "original:organization_id".into(),
        expected_charter_hash: "original:expected_charter_hash".into(),
        governance_proposal_key: "original:governance_proposal_key".into(),
        designated_successors: vec!["original:designated_successors".into()],
        neutral_maintainers: vec!["original:neutral_maintainers".into()],
        appointment_authorities: BTreeSet::from(["original:appointment_authorities".into()]),
        remedy_adjudicators: BTreeSet::from(["original:remedy_adjudicators".into()]),
        residual_share_bps: BTreeMap::from([("_length".into(), u16::MAX)]),
    }
}
pub(crate) fn fixture_EmergencySupplyCommandV1() -> EmergencySupplyCommandV1 {
    EmergencySupplyCommandV1::Observe {
        region_id: "original:region_id".into(),
    }
}
pub(crate) fn fixture_OrganizationEstateRightV1() -> OrganizationEstateRightV1 {
    OrganizationEstateRightV1::Preserve
}
pub(crate) fn fixture_OrganizationEstateScopeV1() -> OrganizationEstateScopeV1 {
    OrganizationEstateScopeV1 {
        object_id: "original:object_id".into(),
        right: fixture_OrganizationEstateRightV1(),
    }
}
pub(crate) fn variants_WorldRuleCommandV1() -> Vec<WorldRuleCommandV1> {
    vec![
        WorldRuleCommandV1::IndustrialWork {
            command: Box::new(fixture_IndustrialWorkCommandV1()),
        },
        WorldRuleCommandV1::RecipeSubstitution {
            command: Box::new(fixture_RecipeSubstitutionCommandV1()),
        },
        WorldRuleCommandV1::MinimumService {
            command: fixture_MinimumServiceCommandV1(),
        },
        WorldRuleCommandV1::OrganizationContinuity {
            command: fixture_OrganizationContinuityCommandV1(),
        },
        WorldRuleCommandV1::EmergencySupply {
            command: fixture_EmergencySupplyCommandV1(),
        },
        WorldRuleCommandV1::DepositOrganizationEstate {
            organization_id: "original:organization_id".into(),
            material: "original:material".into(),
            amount: i64::MIN,
            held_in_trust: true,
        },
        WorldRuleCommandV1::OpenOrganizationProtection {
            organization_id: "original:organization_id".into(),
        },
        WorldRuleCommandV1::BeginOrganizationDissolution {
            organization_id: "original:organization_id".into(),
        },
        WorldRuleCommandV1::AdvanceOrganization {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
        },
        WorldRuleCommandV1::GrantOrganizationScope {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            proposal_key: "original:proposal_key".into(),
            delegate_id: "original:delegate_id".into(),
            scopes: BTreeSet::from([fixture_OrganizationEstateScopeV1()]),
            expires_at: u64::MAX,
            replace_grant_id: Some("original:replace_grant_id".into()),
        },
        WorldRuleCommandV1::RevokeOrganizationScope {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            grant_id: "original:grant_id".into(),
            proposal_key: "original:proposal_key".into(),
        },
        WorldRuleCommandV1::RequestOrganizationRecovery {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            appeal: true,
        },
        WorldRuleCommandV1::ResolveOrganizationRecovery {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            recovery_request_id: "original:recovery_request_id".into(),
            approved: true,
            proposal_key: "original:proposal_key".into(),
        },
        WorldRuleCommandV1::DisposeOrganizationEstate {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            grant_id: "original:grant_id".into(),
            grant_revision: u64::MAX,
            deposit_id: "original:deposit_id".into(),
        },
        WorldRuleCommandV1::AuthorizeSupply {
            proposal_key: "original:proposal_key".into(),
            crisis_id: "original:crisis_id".into(),
            material: "original:material".into(),
            beneficiaries: BTreeSet::from(["original:beneficiaries".into()]),
            budget: i64::MIN,
            expires_at: u64::MAX,
        },
        WorldRuleCommandV1::RequestSupply {
            authorization_id: "original:authorization_id".into(),
            cumulative_demand: i64::MIN,
        },
        WorldRuleCommandV1::FreezeSupply {
            authorization_id: "original:authorization_id".into(),
        },
        WorldRuleCommandV1::AllocateSupply {
            batch_id: "original:batch_id".into(),
        },
        WorldRuleCommandV1::DispatchSupply {
            batch_id: "original:batch_id".into(),
            beneficiary_id: "original:beneficiary_id".into(),
            amount: i64::MIN,
            route_id: "original:route_id".into(),
        },
        WorldRuleCommandV1::SettleSupply { job_id: u64::MAX },
        WorldRuleCommandV1::CloseSupply {
            authorization_id: "original:authorization_id".into(),
        },
        WorldRuleCommandV1::ConfirmAgentOrder {
            factory_id: "original:factory_id".into(),
            quoted_at: u64::MAX,
            quote_digest: "original:quote_digest".into(),
        },
        WorldRuleCommandV1::AdvanceAgentOrder {
            order_id: "original:order_id".into(),
        },
        WorldRuleCommandV1::CancelAgentOrder {
            order_id: "original:order_id".into(),
        },
        WorldRuleCommandV1::DisposeAgentOrder {
            order_id: "original:order_id".into(),
        },
        WorldRuleCommandV1::DeliverAgentOrder {
            order_id: "original:order_id".into(),
        },
    ]
}
pub(crate) fn fixture_IndustrialInputBaselineV1() -> IndustrialInputBaselineV1 {
    IndustrialInputBaselineV1 {
        material_profile: fixture_SubstitutionProfileRefV1(),
        unit: "original:unit".into(),
        spec: "original:spec".into(),
        quality: "original:quality".into(),
        custody: "original:custody".into(),
    }
}
pub(crate) fn fixture_IndustrialDispositionV1() -> IndustrialDispositionV1 {
    IndustrialDispositionV1::Hold
}
pub(crate) fn fixture_IndustrialBaselineAuthorityV1() -> IndustrialBaselineAuthorityV1 {
    IndustrialBaselineAuthorityV1 {
        world_binding: "original:world_binding".into(),
        owner_binding_digest: "original:owner_binding_digest".into(),
        state_digest: "original:state_digest".into(),
        profile_digest: "original:profile_digest".into(),
        plan_digest: "original:plan_digest".into(),
        baseline_impacts: fixture_SubstitutionImpactV1(),
        allowed_dispositions: vec![fixture_IndustrialDispositionV1()],
        terminal_capacities: BTreeMap::from([("_length".into(), u64::MAX)]),
        quoted_at: u64::MAX,
        expires_at: u64::MAX,
        consensus_height: u64::MAX,
    }
}
pub(crate) fn variants_IndustrialWorkCommandV1() -> Vec<IndustrialWorkCommandV1> {
    vec![
        IndustrialWorkCommandV1::ImportQualifiedLot {
            facts: Box::new(fixture_QualifiedIndustrialLotFactsV1()),
            certificate: fixture_GovernanceFinalityCertificate(),
        },
        IndustrialWorkCommandV1::AcceptRoot {
            factory_id: "original:factory_id".into(),
            recipe_id: "original:recipe_id".into(),
            batches: u32::MAX,
            input_baselines: BTreeMap::from([(
                "_length".into(),
                fixture_IndustrialInputBaselineV1(),
            )]),
            authority: Box::new(fixture_IndustrialBaselineAuthorityV1()),
            certificate: fixture_GovernanceFinalityCertificate(),
        },
        IndustrialWorkCommandV1::StartRoot {
            root_ref: fixture_IndustrialRefV1(),
            expected_profile_digest: "original:expected_profile_digest".into(),
            logistics_route_ids: vec!["original:logistics_route_ids".into()],
            logistics_path_ids: vec!["original:logistics_path_ids".into()],
        },
        IndustrialWorkCommandV1::TerminateRoot {
            root_ref: fixture_IndustrialRefV1(),
        },
        IndustrialWorkCommandV1::ReleaseLot {
            lot_ref: fixture_IndustrialRefV1(),
        },
    ]
}
pub(crate) fn fixture_SubstitutionPreviewStatusV1() -> SubstitutionPreviewStatusV1 {
    SubstitutionPreviewStatusV1::Available
}
pub(crate) fn fixture_SubstitutionPreviewRequestV1() -> SubstitutionPreviewRequestV1 {
    SubstitutionPreviewRequestV1 {
        root_ref: fixture_IndustrialRefV1(),
        slot_id: "original:slot_id".into(),
        authority_refs: vec![fixture_IndustrialRefV1()],
    }
}
pub(crate) fn fixture_SubstitutionRoundingV1() -> SubstitutionRoundingV1 {
    SubstitutionRoundingV1::Exact
}
pub(crate) fn fixture_SubstitutionResidualV1() -> SubstitutionResidualV1 {
    SubstitutionResidualV1::Reject
}
pub(crate) fn fixture_SubstitutionConversionV1() -> SubstitutionConversionV1 {
    SubstitutionConversionV1::None
}
pub(crate) fn fixture_SubstitutionRuleV1() -> SubstitutionRuleV1 {
    SubstitutionRuleV1 {
        rule: fixture_SubstitutionProfileRefV1(),
        recipe: fixture_SubstitutionProfileRefV1(),
        input_edge: "original:input_edge".into(),
        required_material: fixture_SubstitutionProfileRefV1(),
        allowed_materials: vec![fixture_SubstitutionProfileRefV1()],
        accepted_specs: vec!["original:accepted_specs".into()],
        accepted_quality: vec!["original:accepted_quality".into()],
        accepted_custody: vec!["original:accepted_custody".into()],
        owner_ledger_path_policy: fixture_SubstitutionProfileRefV1(),
        source_unit: "original:source_unit".into(),
        target_unit: "original:target_unit".into(),
        ratio_numerator: u64::MAX,
        ratio_denominator: u64::MAX,
        executable_target_quantum: u64::MAX,
        rounding: fixture_SubstitutionRoundingV1(),
        residual: fixture_SubstitutionResidualV1(),
        aggregation_domain: fixture_SubstitutionProfileRefV1(),
        source_loss_numerator: u64::MAX,
        source_loss_denominator: u64::MAX,
        impacts: fixture_SubstitutionImpactV1(),
        conversion: fixture_SubstitutionConversionV1(),
        applicable_input_states: vec!["original:applicable_input_states".into()],
        disposition_policy: fixture_SubstitutionProfileRefV1(),
        recheck_policy: fixture_SubstitutionProfileRefV1(),
    }
}
pub(crate) fn fixture_SubstitutionCandidateKindV1() -> SubstitutionCandidateKindV1 {
    SubstitutionCandidateKindV1::SameMaterial
}
pub(crate) fn fixture_SubstitutionQuantityPlanV1() -> SubstitutionQuantityPlanV1 {
    SubstitutionQuantityPlanV1 {
        rule: fixture_SubstitutionProfileRefV1(),
        domain: fixture_SubstitutionProfileRefV1(),
        domain_key: "original:domain_key".into(),
        source_unit: "original:source_unit".into(),
        target_unit: "original:target_unit".into(),
        source_required: u64::MAX,
        source_loss: u64::MAX,
        target_executable: u64::MAX,
        residual_numerator: u64::MAX,
        residual_denominator: u64::MAX,
        unmet_target: u64::MAX,
    }
}
pub(crate) fn fixture_IndustrialParentV1() -> IndustrialParentV1 {
    IndustrialParentV1 {
        lot_ref: fixture_IndustrialRefV1(),
        settlement_ref: fixture_IndustrialRefV1(),
        quantity: u64::MAX,
    }
}
pub(crate) fn fixture_IndustrialInputSlotV1() -> IndustrialInputSlotV1 {
    IndustrialInputSlotV1 {
        material_profile: fixture_SubstitutionProfileRefV1(),
        material: "original:material".into(),
        unit: "original:unit".into(),
        spec: "original:spec".into(),
        quality: "original:quality".into(),
        custody: "original:custody".into(),
        quantity: u64::MAX,
        binding_revision: u64::MAX,
        parents: vec![fixture_IndustrialParentV1()],
        consumed: true,
    }
}
