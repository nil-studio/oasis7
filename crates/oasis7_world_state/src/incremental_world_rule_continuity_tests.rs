use super::*;
use crate::incremental_typed_record_test_support::continuity_data as data;
use crate::incremental_typed_record_test_support::{roundtrip, source_admission};
#[test]
fn continuity_full_original_nested_graph_cold_roundtrip() {
    let _source_admission = source_admission();
    let mut source = data::fixture_OrganizationContinuityStateV1();
    *source.consumed_effects.values_mut().next().unwrap() = "x".repeat(256 << 10);
    roundtrip(
        &source,
        WorldRuleCompositeBranchV1::OrganizationContinuity.profile(),
    )
}
#[test]
fn emergency_full_original_nested_graph_cold_roundtrip() {
    let _source_admission = source_admission();
    let mut source = data::fixture_EmergencySupplyStateV1();
    source.reviews.values_mut().next().unwrap().reason_ref = "r".repeat(256 << 10);
    roundtrip(
        &source,
        WorldRuleCompositeBranchV1::EmergencySupply.profile(),
    )
}

#[test]
fn continuity_every_explicit_unit_variant_cold_roundtrip() {
    let _source_admission = source_admission();
    macro_rules! check {($ty:path; $($variant:ident),+)=>{{use $ty as T;for value in [$(T::$variant),+]{roundtrip(&value,WorldRuleCompositeBranchV1::OrganizationContinuity.profile());}}};}
    check!(crate::world_rules::OrganizationAvailabilityEvidenceStatusV1;Unknown,Certified);
    check!(crate::world_rules::OrganizationActivityKindV1;FulfilledDelivery,ExternalServiceSettlement,ExternalAtomicExchangeSettlement,NonCircularProductionDelivery,AuthorizedBusinessDecision);
    check!(crate::world_rules::OrganizationContinuityPhaseV1;AwaitingNotice,Notice,Protection,CandidateCollection,Protected,Disposition,Resolved,Cancelled);
    check!(crate::world_rules::OrganizationCaseOriginV1;Dormancy,VoluntaryDissolution,VoluntaryReorganization);
    check!(crate::world_rules::OrganizationDispositionStageV1;None,Frozen,ExistingContracts,CustodyReturn,ClaimsAndCosts,BusinessTransfer,Residual,Complete);
    check!(oasis7_agent_api::world_action::OrganizationRemedyKindV1;Reclaim,Appeal);
    check!(oasis7_agent_api::world_action::OrganizationRemedyOutcomeV1;Rejected,Withdrawn,Supported,PartiallySupported);
}
