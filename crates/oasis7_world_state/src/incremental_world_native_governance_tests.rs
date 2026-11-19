use super::*;
use crate::incremental_typed_record_test_support::source_admission;
#[path = "incremental_world_native_governance_test_support.rs"]
mod support;
#[test]
fn native_governance_registry_options_preserve_none_and_some_empty() {
    let _source = source_admission();
    assert_ne!(
        support::field::<GovernanceFinalitySignerRegistryFieldV1>(&(), &None),
        support::field::<GovernanceFinalitySignerRegistryFieldV1>(&(), &Some(Default::default()))
    );
    support::field::<GovernanceFinalitySignerRegistryFieldV1>(&(), &Some(fixture::signer()));
    assert_ne!(
        support::field::<GovernanceMainTokenControllerRegistryFieldV1>(&(), &None),
        support::field::<GovernanceMainTokenControllerRegistryFieldV1>(
            &(),
            &Some(Default::default())
        )
    );
    support::field::<GovernanceMainTokenControllerRegistryFieldV1>(
        &(),
        &Some(fixture::controller()),
    );
}
#[test]
fn native_admission_preserves_all_five_original_statuses_and_options() {
    let _source = source_admission();
    use crate::governance::GovernanceValidatorAdmissionStatus as S;
    for status in [
        S::Applied,
        S::ApprovedCandidate,
        S::ProbationReady,
        S::Active,
        S::Revoked,
    ] {
        let mut record = fixture::admission();
        record.status = status;
        support::field::<GovernanceValidatorAdmissionsFieldV1>(&"outer-candidate".into(), &record);
    }
}
#[test]
fn native_money_records_preserve_original_keys_and_nested_distributions() {
    let _source = source_admission();
    support::field::<MainTokenNodePointsBridgeRecordsFieldV1>(&101, &fixture::bridge());
    support::field::<MainTokenTreasuryDistributionRecordsFieldV1>(
        &"outer-distribution".into(),
        &fixture::treasury(),
    );
    support::field::<RestrictedStarterClaimLiveopsPoolTopUpRecordsFieldV1>(
        &"outer-top-up".into(),
        &fixture::top_up(),
    );
}
#[test]
fn native_finality_rotation_exact_authorized_input_roundtrips() {
    let _source = source_admission();
    support::field::<GovernanceFinalityKeyRotationsFieldV1>(
        &"rotation".into(),
        &fixture::rotation(),
    );
}
#[test]
fn native_finality_rotation_rejects_original_invalid_relations_and_action() {
    let _source = source_admission();
    let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
    let record = fixture::rotation();
    assert!(
        GovernanceFinalityKeyRotationsFieldV1::validate_source_record(
            &"different".into(),
            &record,
            &memory,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    let mut changed = record.clone();
    changed.applied_at_tick += 1;
    assert!(
        GovernanceFinalityKeyRotationsFieldV1::validate_source_record(
            &"rotation".into(),
            &changed,
            &memory,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    let mut changed = record.clone();
    changed.authorized_action.action = Box::new(
        oasis7_agent_api::world_action::Action::ApproveGovernanceValidatorAdmission {
            controller_account_id: "controller".into(),
            candidate_id: "candidate".into(),
        },
    );
    assert!(
        GovernanceFinalityKeyRotationsFieldV1::validate_source_record(
            &"rotation".into(),
            &changed,
            &memory,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    let mut changed = record;
    let oasis7_agent_api::world_action::Action::RotateGovernanceValidatorFinalityKeyV1 {
        old_key_proof,
        ..
    } = changed.authorized_action.action.as_mut()
    else {
        unreachable!()
    };
    old_key_proof.role =
        oasis7_agent_api::world_action::FinalityKeyRotationRoleV1::NewKeyPossession;
    assert!(
        GovernanceFinalityKeyRotationsFieldV1::validate_source_record(
            &"rotation".into(),
            &changed,
            &memory,
            MemoryClass::Ordinary
        )
        .is_err()
    );
}

#[path = "incremental_world_native_governance_binding_tests.rs"]
mod binding_tests;
