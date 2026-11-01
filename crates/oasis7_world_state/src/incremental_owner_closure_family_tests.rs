use super::*;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StateScanBudget,
    incremental_value::{ValueIoBudget, ValueReadContext},
};

pub(super) fn append_records(
    issue: FixtureIssue,
    values: &mut ValuePages,
    memory: &MemoryBudget,
    state: &StatePages,
    value_io: &mut ValueIoBudget,
    key_work: &mut SemanticKeyBudget,
    writes: &mut BTreeMap<StateNamespaceV1, Vec<StateWriteV1>>,
) {
    let mut source = crate::WorldState::default();
    source.governance_identity_profiles.insert(
        "governance-outer-id".into(),
        crate::GovernanceIdentityProfileState {
            agent_id: "embedded-governance-id".into(),
            ..Default::default()
        },
    );
    source.factory_site_authorities.insert(
        "factory-outer-id".into(),
        crate::FactorySiteAuthorityV1 {
            site_id: "embedded-factory-id".into(),
            ..Default::default()
        },
    );
    source.main_token_epoch_issuance_records.insert(
        7,
        crate::MainTokenEpochIssuanceRecord {
            epoch_index: 11,
            issued_amount: 13,
            ..Default::default()
        },
    );

    let mut record_store = FixtureRecordStore { values, state };
    let mut context = MemberRecordWriteContext {
        sink: &mut record_store,
        memory,
        io: value_io,
        key_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let mut sink = |write: ReservedValue<ExecutionWriteV1>| -> Result<(), ValueTreeError> {
        let write = &*write;
        if issue == FixtureIssue::MissingGovernanceFamilyEntries {
            let key = crate::incremental_world_field_key::WorldFieldKeyV1::from_bytes(
                &write.mutation.key,
            )
            .unwrap();
            if key.name == "governance_identity_profiles"
                && key.kind == WorldFieldRecordKindV1::Entry
            {
                return Ok(());
            }
        }
        writes
            .entry(write.namespace)
            .or_default()
            .push(write.mutation.clone());
        Ok(())
    };

    let mut governance_records = 100_000;
    let mut governance_work = crate::incremental_governance_records::GovernanceWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
        scan: StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 10_000,
        },
    };
    crate::incremental_governance_records::prepare_governance_records_genesis_v1(
        &source,
        &mut context,
        &mut governance_records,
        &mut governance_work,
        &mut sink,
    )
    .unwrap();

    let mut factory_work = crate::incremental_factory_authority_records::FactoryAuthorityWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
    };
    crate::incremental_factory_authority_records::prepare_factory_authority_genesis_v1(
        &source,
        &mut context,
        &mut factory_work,
        &mut sink,
    )
    .unwrap();

    let mut token_work = crate::incremental_main_token_policy_records::MainTokenPolicyWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
    };
    crate::incremental_main_token_policy_records::prepare_main_token_policy_genesis_v1(
        &source,
        &mut context,
        &mut token_work,
        &mut sink,
    )
    .unwrap();
}

fn verify_family_fixture(
    fixture: &Fixture,
) -> Result<ReservedValue<UnsupportedWorldStateOwnersV1>, WorldStateOwnerError> {
    let mut value_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    };
    let mut values = ValueReadContext {
        source: &fixture.values,
        memory: &fixture.memory,
        class: MemoryClass::Ordinary,
        io: &mut value_io,
    };
    // This fixture runs the generic pass and one full root pass per present
    // family. One explicit finite allowance covers those shared traversals.
    let mut state_io = StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100_000,
    };
    verify_world_state_owner_coverage(
        &root_set(fixture),
        &fixture.state,
        &mut values,
        &mut state_io,
        1 << 20,
        || false,
    )
}

#[test]
fn absent_closed_record_families_remain_explicitly_unsupported() {
    let fixture = super::fixture(true);
    let report = super::verify(&fixture, || false).unwrap();
    assert_eq!(report.checked_governance_fields(), 0);
    assert_eq!(report.checked_factory_authority_fields(), 0);
    assert_eq!(report.checked_main_token_policy_fields(), 0);
    for name in [
        "governance_identity_profiles",
        "governance_proposals",
        "governance_votes",
        "factory_site_authorities",
        "factory_construction_power_profiles",
        "main_token_config",
        "main_token_genesis_buckets",
        "main_token_epoch_issuance_records",
        "restricted_starter_claim_grants",
        "main_token_scheduled_policy_updates",
    ] {
        assert!(report.unsupported_fields().contains(&name), "{name}");
    }
}

#[test]
fn present_record_families_are_qualified_as_root_bound_closed_units() {
    let fixture = super::fixture_with_issue(true, Some(FixtureIssue::RecordFamilies));
    let report = verify_family_fixture(&fixture).unwrap();
    assert_eq!(report.checked_governance_fields(), 3);
    assert_eq!(report.checked_factory_authority_fields(), 2);
    assert_eq!(report.checked_main_token_policy_fields(), 5);
    for name in [
        "governance_identity_profiles",
        "governance_proposals",
        "governance_votes",
        "factory_site_authorities",
        "factory_construction_power_profiles",
        "main_token_config",
        "main_token_genesis_buckets",
        "main_token_epoch_issuance_records",
        "restricted_starter_claim_grants",
        "main_token_scheduled_policy_updates",
    ] {
        assert!(!report.unsupported_fields().contains(&name), "{name}");
    }
    // Other typed owners and the still-unimplemented Runtime/Cognition
    // registries remain explicit gaps; these family receipts do not activate a
    // complete WorldState.
    assert!(report.runtime_registry_missing());
    assert!(report.cognition_registry_missing());
    assert!(report.unsupported_fields().contains(&"resources"));
}

#[test]
fn a_partial_marked_record_family_fails_closed() {
    let fixture =
        super::fixture_with_issue(true, Some(FixtureIssue::MissingGovernanceFamilyEntries));
    assert!(matches!(
        verify_family_fixture(&fixture),
        Err(WorldStateOwnerError::Value(ValueTreeError::Invalid(_)))
    ));
}
