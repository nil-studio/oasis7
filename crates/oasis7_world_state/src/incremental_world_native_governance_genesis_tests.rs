use super::*;
use crate::{
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, io, source_admission},
    incremental_world_native::recover_world_native_record_v1,
    incremental_world_rule_composite_family::test_support::{persist, read_work, scan, write_work},
};
use oasis7_formal_protocol::{
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::ValueReadContext,
};
#[test]
fn native_governance_group_genesis_emits_all_seven_original_families() {
    let _source = source_admission();
    let mut world = WorldState::default();
    fixture::populate_world(&mut world);
    world
        .governance_validator_admissions
        .insert("second-outer".into(), fixture::admission());
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 131_072);
    let _rows = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::with_capacity(16);
    let mut io_write = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut records = 1000;
    prepare_world_native_governance_genesis_v1(
        &world,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io_write,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut records,
        &mut write_work(),
        &mut |row| {
            assert!(rows.len() < 16);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(rows.len(), 15);
    assert_eq!(records, 985);
    let names: std::collections::BTreeSet<_> = rows
        .iter()
        .filter_map(|row| {
            let key = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
            (key.kind == WorldFieldRecordKindV1::Metadata).then(|| key.name.to_owned())
        })
        .collect();
    assert_eq!(
        names,
        std::collections::BTreeSet::from(
            [
                GovernanceFinalitySignerRegistryFieldV1::NAME,
                GovernanceValidatorAdmissionsFieldV1::NAME,
                GovernanceFinalityKeyRotationsFieldV1::NAME,
                GovernanceMainTokenControllerRegistryFieldV1::NAME,
                MainTokenNodePointsBridgeRecordsFieldV1::NAME,
                MainTokenTreasuryDistributionRecordsFieldV1::NAME,
                RestrictedStarterClaimLiveopsPoolTopUpRecordsFieldV1::NAME
            ]
            .map(str::to_owned)
        )
    );
    let mut writes: Vec<StateWriteV1> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let _delta = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
        writes,
        &pages,
        8 << 20,
    )
    .unwrap();
    persist(&pages, &memory, &delta.new_nodes);
    let mut io_read = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: &delta.after,
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io_read,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    macro_rules! check {
        ($field:ty,$key:expr,$value:expr) => {{
            let record = recover_world_native_record_v1::<$field>(
                $key,
                &mut read,
                &mut scan(),
                &mut read_work(),
            )
            .unwrap()
            .unwrap();
            assert_eq!(record.value().value(), $value);
        }};
    }
    check!(
        GovernanceFinalitySignerRegistryFieldV1,
        &(),
        &world.governance_finality_signer_registry
    );
    check!(
        GovernanceMainTokenControllerRegistryFieldV1,
        &(),
        &world.governance_main_token_controller_registry
    );
    check!(
        GovernanceValidatorAdmissionsFieldV1,
        &"outer-candidate".into(),
        &world.governance_validator_admissions["outer-candidate"]
    );
    check!(
        GovernanceValidatorAdmissionsFieldV1,
        &"second-outer".into(),
        &world.governance_validator_admissions["second-outer"]
    );
    check!(
        GovernanceFinalityKeyRotationsFieldV1,
        &"rotation".into(),
        &world.governance_finality_key_rotations["rotation"]
    );
    check!(
        MainTokenNodePointsBridgeRecordsFieldV1,
        &101,
        &world.main_token_node_points_bridge_records[&101]
    );
    check!(
        MainTokenTreasuryDistributionRecordsFieldV1,
        &"outer-distribution".into(),
        &world.main_token_treasury_distribution_records["outer-distribution"]
    );
    check!(
        RestrictedStarterClaimLiveopsPoolTopUpRecordsFieldV1,
        &"outer-top-up".into(),
        &world.restricted_starter_claim_liveops_pool_top_up_records["outer-top-up"]
    );
}
#[test]
fn native_rotation_original_field_count_rejected_before_any_output() {
    let _source = source_admission();
    let mut world = WorldState::default();
    for index in 0..257 {
        let mut record = fixture::rotation();
        let key = format!("rotation-{index}");
        let oasis7_agent_api::world_action::Action::RotateGovernanceValidatorFinalityKeyV1 {
            body,
            ..
        } = record.authorized_action.action.as_mut()
        else {
            unreachable!()
        };
        body.rotation_id = key.clone();
        world.governance_finality_key_rotations.insert(key, record);
    }
    let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let mut records = 1000;
    let mut outputs = 0;
    assert!(
        prepare_world_native_governance_genesis_v1(
            &world,
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 32 << 20
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 2 << 20,
                    record_bytes: 3 << 20
                },
                cancelled: || false
            },
            &mut records,
            &mut write_work(),
            &mut |_| {
                outputs += 1;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(outputs, 0);
    assert_eq!(records, 1000);
    assert_eq!(pages.len(), 0);
}
