use super::*;
pub(crate) fn fixture_RootOperationV1() -> RootOperationV1 {
    RootOperationV1 {
        owner_binding_digest: "original:owner_binding_digest".into(),
        allowed_dispositions: vec![fixture_IndustrialDispositionV1()],
        output_ledger: fixture_MaterialLedgerId(),
        terminal_capacities: BTreeMap::from([("_length".into(), u64::MAX)]),
        started_job_id: Some(u64::MAX),
        root_ref: fixture_IndustrialRefV1(),
        owner_id: "original:owner_id".into(),
        factory_id: "original:factory_id".into(),
        pinned_recipe_id: "original:pinned_recipe_id".into(),
        accepted_batches: u32::MAX,
        pinned_profile_digest: "original:pinned_profile_digest".into(),
        causal_digest: "original:causal_digest".into(),
        work: u64::MAX,
        first_sink: true,
        slots: BTreeMap::from([("_length".into(), fixture_IndustrialInputSlotV1())]),
    }
}
pub(crate) fn fixture_DispositionPlanV1() -> DispositionPlanV1 {
    DispositionPlanV1 {
        root_ref: fixture_IndustrialRefV1(),
        plan_ref: fixture_IndustrialRefV1(),
        expected_causal_digest: "original:expected_causal_digest".into(),
        disposition: fixture_IndustrialDispositionV1(),
    }
}
pub(crate) fn fixture_MaterialStack() -> MaterialStack {
    MaterialStack {
        kind: "original:kind".into(),
        amount: i64::MIN,
    }
}
pub(crate) fn fixture_RecipeExecutionPlan() -> RecipeExecutionPlan {
    RecipeExecutionPlan {
        accepted_batches: u32::MAX,
        consume: vec![fixture_MaterialStack()],
        produce: vec![fixture_MaterialStack()],
        byproducts: vec![fixture_MaterialStack()],
        power_required: i64::MIN,
        duration_ticks: u32::MAX,
        reject_reason: Some("original:reject_reason".into()),
    }
}
pub(crate) fn fixture_SubstitutionCandidateV1() -> SubstitutionCandidateV1 {
    SubstitutionCandidateV1 {
        candidate_digest: "original:candidate_digest".into(),
        authority: fixture_SubstitutionExecutionAuthorityV1(),
        rule: Some(fixture_SubstitutionRuleV1()),
        kind: fixture_SubstitutionCandidateKindV1(),
        plan: Some(fixture_SubstitutionQuantityPlanV1()),
        native_source_quantity: Some(u64::MAX),
        source_material_kind: Some("original:source_material_kind".into()),
        candidate_root: Some(fixture_RootOperationV1()),
        cutover_plan: Some(fixture_DispositionPlanV1()),
        cutover_plan_digest: Some("original:cutover_plan_digest".into()),
        recipe_plan: Some(fixture_RecipeExecutionPlan()),
        causal_equivalent: true,
        conversion_required: true,
        source_remaining: u64::MAX,
        binding_revision: u64::MAX,
        resource_balances: BTreeMap::from([("_length".into(), i64::MIN)]),
    }
}
pub(crate) fn fixture_SubstitutionPreviewV1() -> SubstitutionPreviewV1 {
    SubstitutionPreviewV1 {
        preview_digest: "original:preview_digest".into(),
        status: fixture_SubstitutionPreviewStatusV1(),
        blocker: Some("original:blocker".into()),
        next_action: "original:next_action".into(),
        next_recheck: u64::MAX,
        request: fixture_SubstitutionPreviewRequestV1(),
        candidates: vec![fixture_SubstitutionCandidateV1()],
        evaluated_at: u64::MAX,
    }
}
pub(crate) fn fixture_SubstitutionScopeV1() -> SubstitutionScopeV1 {
    SubstitutionScopeV1::DecisionOnly
}
pub(crate) fn fixture_ConfirmSubstitutionV1() -> ConfirmSubstitutionV1 {
    ConfirmSubstitutionV1 {
        preview: fixture_SubstitutionPreviewV1(),
        candidate_digest: "original:candidate_digest".into(),
        scope: fixture_SubstitutionScopeV1(),
        maximum_source: u64::MAX,
        maximum_costs: BTreeMap::from([("_length".into(), u64::MAX)]),
        maximum_power: u64::MAX,
        cutover_plan_digest: Some("original:cutover_plan_digest".into()),
    }
}
pub(crate) fn variants_RecipeSubstitutionCommandV1() -> Vec<RecipeSubstitutionCommandV1> {
    vec![
        RecipeSubstitutionCommandV1::PublishAuthority {
            authority: Box::new(fixture_SubstitutionExecutionAuthorityV1()),
            consensus_height: u64::MAX,
            finality_certificate: fixture_GovernanceFinalityCertificate(),
        },
        RecipeSubstitutionCommandV1::Confirm {
            confirmation: fixture_ConfirmSubstitutionV1(),
        },
        RecipeSubstitutionCommandV1::Convert {
            decision_ref: fixture_IndustrialRefV1(),
        },
        RecipeSubstitutionCommandV1::ApplyBinding {
            decision_ref: fixture_IndustrialRefV1(),
        },
        RecipeSubstitutionCommandV1::Cutover {
            decision_ref: fixture_IndustrialRefV1(),
            disposition: fixture_DispositionPlanV1(),
            candidate_root: Box::new(fixture_RootOperationV1()),
        },
    ]
}
pub(crate) fn variants_MinimumServiceCommandV1() -> Vec<MinimumServiceCommandV1> {
    vec![MinimumServiceCommandV1::Register {
        registration: fixture_EmergencyServiceRegistrationV1(),
        region_budget_lineage: "original:region_budget_lineage".into(),
        certificate: fixture_GovernanceFinalityCertificate(),
    }]
}
pub(crate) fn fixture_OrganizationVoluntaryCaseKindV1() -> OrganizationVoluntaryCaseKindV1 {
    OrganizationVoluntaryCaseKindV1::Dissolution
}
pub(crate) fn fixture_OrganizationRemedyKindV1() -> OrganizationRemedyKindV1 {
    OrganizationRemedyKindV1::Reclaim
}
pub(crate) fn fixture_OrganizationRemedyOutcomeV1() -> OrganizationRemedyOutcomeV1 {
    OrganizationRemedyOutcomeV1::Rejected
}
pub(crate) fn fixture_OrganizationEstateOperationV1() -> OrganizationEstateOperationV1 {
    OrganizationEstateOperationV1::ReturnCustody
}
pub(crate) fn variants_OrganizationContinuityCommandV1() -> Vec<OrganizationContinuityCommandV1> {
    vec![
        OrganizationContinuityCommandV1::Enable {
            organization_id: "original:organization_id".into(),
            expected_charter_hash: "original:expected_charter_hash".into(),
            governance_proposal_key: "original:governance_proposal_key".into(),
            designated_successors: vec!["original:designated_successors".into()],
            neutral_maintainers: vec!["original:neutral_maintainers".into()],
            appointment_authorities: BTreeSet::from(["original:appointment_authorities".into()]),
            remedy_adjudicators: BTreeSet::from(["original:remedy_adjudicators".into()]),
            residual_share_bps: BTreeMap::from([("_length".into(), u16::MAX)]),
        },
        OrganizationContinuityCommandV1::RegisterActivityContract {
            organization_id: "original:organization_id".into(),
            contract_id: "original:contract_id".into(),
            expected_terms_hash: "original:expected_terms_hash".into(),
            governance_proposal_key: "original:governance_proposal_key".into(),
        },
        OrganizationContinuityCommandV1::RecordAvailability {
            organization_id: "original:organization_id".into(),
            from_world_tick: u64::MAX,
            through_world_tick: u64::MAX,
            world_writes_available: true,
            notice_available: true,
            remedy_available: true,
            certificate: fixture_GovernanceFinalityCertificate(),
        },
        OrganizationContinuityCommandV1::FinalizeAvailabilityUnknown {
            organization_id: "original:organization_id".into(),
            through_world_tick: u64::MAX,
        },
        OrganizationContinuityCommandV1::CloseActivityTick {
            organization_id: "original:organization_id".into(),
            through_world_tick: u64::MAX,
            previous_close_hash: Some("original:previous_close_hash".into()),
            source_catalog_root: "original:source_catalog_root".into(),
            through_event_id: u64::MAX,
            through_event_hash: "original:through_event_hash".into(),
            close_hash: "original:close_hash".into(),
        },
        OrganizationContinuityCommandV1::OpenDormancyCase {
            organization_id: "original:organization_id".into(),
            expected_manifest_root: "original:expected_manifest_root".into(),
            trigger_activity_receipt: Some("original:trigger_activity_receipt".into()),
        },
        OrganizationContinuityCommandV1::OpenVoluntaryCase {
            organization_id: "original:organization_id".into(),
            expected_charter_hash: "original:expected_charter_hash".into(),
            governance_proposal_key: "original:governance_proposal_key".into(),
            kind: fixture_OrganizationVoluntaryCaseKindV1(),
        },
        OrganizationContinuityCommandV1::AdvancePhase {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
        },
        OrganizationContinuityCommandV1::PublishNotice {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            recipient_ids: BTreeSet::from(["original:recipient_ids".into()]),
            scope_refs: BTreeSet::from(["original:scope_refs".into()]),
            reason_code: "original:reason_code".into(),
            trigger_fact_ref: "original:trigger_fact_ref".into(),
            evidence_refs: vec!["original:evidence_refs".into()],
            allowed_actions: BTreeSet::from(["original:allowed_actions".into()]),
            restricted_actions: BTreeSet::from(["original:restricted_actions".into()]),
        },
        OrganizationContinuityCommandV1::SubmitSuccessor {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            candidate_id: "original:candidate_id".into(),
            scope_refs: BTreeSet::from(["original:scope_refs".into()]),
            budget_ref: "original:budget_ref".into(),
            qualification_receipts: vec!["original:qualification_receipts".into()],
            conflict_disclosures: BTreeSet::from(["original:conflict_disclosures".into()]),
            acceptance_receipt: "original:acceptance_receipt".into(),
        },
        OrganizationContinuityCommandV1::CommitHandoff {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            candidate_id: "original:candidate_id".into(),
            scope_refs: BTreeSet::from(["original:scope_refs".into()]),
            governance_proposal_key: "original:governance_proposal_key".into(),
        },
        OrganizationContinuityCommandV1::SubmitRemedy {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            request_id: "original:request_id".into(),
            kind: fixture_OrganizationRemedyKindV1(),
            scope_refs: BTreeSet::from(["original:scope_refs".into()]),
            fact_ref: "original:fact_ref".into(),
            evidence_refs: vec!["original:evidence_refs".into()],
        },
        OrganizationContinuityCommandV1::ResolveRemedy {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            request_id: "original:request_id".into(),
            outcome: fixture_OrganizationRemedyOutcomeV1(),
            supported_scope_refs: BTreeSet::from(["original:supported_scope_refs".into()]),
            adjudication_ref: "original:adjudication_ref".into(),
            governance_proposal_key: "original:governance_proposal_key".into(),
        },
        OrganizationContinuityCommandV1::AdvanceDisposition {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            governance_proposal_key: "original:governance_proposal_key".into(),
        },
        OrganizationContinuityCommandV1::SettleEstateAsset {
            case_id: "original:case_id".into(),
            expected_revision: u64::MAX,
            asset_ref: "original:asset_ref".into(),
            expected_asset_version: u64::MAX,
            operation: fixture_OrganizationEstateOperationV1(),
            amount: i64::MIN,
            allocations: BTreeMap::from([("_length".into(), i64::MIN)]),
            effect_ref: "original:effect_ref".into(),
            governance_proposal_key: "original:governance_proposal_key".into(),
        },
    ]
}
pub(crate) fn fixture_EmergencyObservationV1() -> EmergencyObservationV1 {
    EmergencyObservationV1 {
        window_start: u64::MAX,
        world_root: "original:world_root".into(),
        obligation_ids: BTreeSet::from(["original:obligation_ids".into()]),
        normal_coverage: BTreeMap::from([("_length".into(), i64::MIN)]),
    }
}
pub(crate) fn fixture_EmergencyEvidenceV1() -> EmergencyEvidenceV1 {
    EmergencyEvidenceV1 {
        region_id: "original:region_id".into(),
        region_budget_lineage: "original:region_budget_lineage".into(),
        observations: vec![fixture_EmergencyObservationV1()],
        source_ref: "original:source_ref".into(),
    }
}
pub(crate) fn fixture_EmergencyNeedV1() -> EmergencyNeedV1 {
    EmergencyNeedV1 {
        obligation_id: "original:obligation_id".into(),
        interval_start: u64::MAX,
    }
}
pub(crate) fn variants_EmergencySupplyCommandV1() -> Vec<EmergencySupplyCommandV1> {
    vec![
        EmergencySupplyCommandV1::Observe {
            region_id: "original:region_id".into(),
        },
        EmergencySupplyCommandV1::RegisterService {
            proposal_key: "original:proposal_key".into(),
            registration: fixture_EmergencyServiceRegistrationV1(),
        },
        EmergencySupplyCommandV1::PublishEvidence {
            proposal_key: "original:proposal_key".into(),
            evidence: fixture_EmergencyEvidenceV1(),
        },
        EmergencySupplyCommandV1::Authorize {
            proposal_key: "original:proposal_key".into(),
            evidence_id: "original:evidence_id".into(),
            reviewer_id: "original:reviewer_id".into(),
            starts_at: u64::MAX,
            expires_at: u64::MAX,
            quantity: i64::MIN,
        },
        EmergencySupplyCommandV1::RefreshEvidence {
            authorization_id: "original:authorization_id".into(),
            evidence_id: "original:evidence_id".into(),
        },
        EmergencySupplyCommandV1::Request {
            authorization_id: "original:authorization_id".into(),
            needs: vec![fixture_EmergencyNeedV1()],
        },
        EmergencySupplyCommandV1::Freeze {
            authorization_id: "original:authorization_id".into(),
        },
        EmergencySupplyCommandV1::Allocate {
            batch_id: "original:batch_id".into(),
        },
        EmergencySupplyCommandV1::Deliver {
            batch_id: "original:batch_id".into(),
            need_id: "original:need_id".into(),
            quantity: i64::MIN,
        },
        EmergencySupplyCommandV1::OpenReview {
            batch_id: "original:batch_id".into(),
            need_id: "original:need_id".into(),
            reason_ref: "original:reason_ref".into(),
        },
        EmergencySupplyCommandV1::DecideReview {
            proposal_key: "original:proposal_key".into(),
            review_id: "original:review_id".into(),
            cancel_unsettled: true,
        },
        EmergencySupplyCommandV1::Close {
            authorization_id: "original:authorization_id".into(),
        },
    ]
}
