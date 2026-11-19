//! Metadata codecs preserve original structural checks; no authority is issued.
use super::*;
use crate::governance::BootstrapFinalityKeyRotationRecordV1;
use crate::{
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, io, source_admission},
    incremental_world_native::recover_world_native_record_v1,
    incremental_world_rule_composite_family::test_support::{persist, read_work, scan, write_work},
};
use oasis7_agent_api::world_action::{
    Action, BootstrapFinalityKeyRotationBodyV1, BootstrapFinalityKeyRotationProofV1,
    BootstrapFinalityKeyRotationRoleV1, GovernanceAuthorizationV1, GovernanceAuthorizedActionV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::ValueReadContext,
};
use std::collections::BTreeMap;
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn record(id: &str) -> BootstrapFinalityKeyRotationRecordV1 {
    let context = GovernanceAuthorizationV1 {
        schema_version: 1,
        world_id: "rotation-vector".into(),
        chain_id: "chain".into(),
        genesis_digest: h(1),
        parent_position: 8,
        parent_record_id: h(2),
        parent_state_root: h(3),
        policy_digest: h(4),
        controller_account_id: "controller".into(),
        nonce: 1,
        valid_from_position: 9,
        valid_through_position: 9,
        signatures: BTreeMap::new(),
    };
    let body = BootstrapFinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: context.clone(),
        rotation_id: id.into(),
        bootstrap_registry_digest_hex: h(8),
        bootstrap_public_key_hex: h(9),
        node_id: "node".into(),
        expected_generation: 1,
        old_public_key_hex: h(5),
        new_public_key_hex: h(6),
        decision_height: 9,
        activation_height: 11,
        target_epoch: 11,
    };
    let old_key_proof = BootstrapFinalityKeyRotationProofV1 {
        schema_version: 1,
        role: BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
        signature_hex: "00".repeat(64),
    };
    let new_key_proof = BootstrapFinalityKeyRotationProofV1 {
        schema_version: 1,
        role: BootstrapFinalityKeyRotationRoleV1::NewKeyPossession,
        signature_hex: "00".repeat(64),
    };
    let mut authorization = context;
    authorization.signatures.insert(h(7), "00".repeat(64));
    BootstrapFinalityKeyRotationRecordV1 {
        authorized_action: GovernanceAuthorizedActionV1 {
            action: Box::new(Action::RotateGovernanceBootstrapFinalityKeyV1 {
                controller_account_id: "controller".into(),
                body,
                old_key_proof,
                new_key_proof,
            }),
            authorization,
        },
        applied_at_tick: 9,
    }
}
#[test]
fn nonempty_bootstrap_genesis_pages_qualify_and_recover_original_record() {
    let _source = source_admission();
    let mut world = WorldState::default();
    let original = record("bootstrap-one");
    original.validate().unwrap();
    world
        .governance_bootstrap_finality_key_rotations
        .insert("bootstrap-one".into(), original.clone());
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 131072);
    let _rows = memory.reserve(32768, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::with_capacity(32);
    let mut records = 1000;
    prepare_world_native_governance_genesis_v1(
        &world,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
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
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| {
                let key = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
                key.kind == WorldFieldRecordKindV1::Metadata
            })
            .count(),
        8
    );
    let mut writes: Vec<StateWriteV1> = rows.iter().map(|row| row.mutation.clone()).collect();
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
    let mut roots = RootSetV1::empty("bootstrap-metadata".into(), [77; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap() = delta.after.clone();
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: &delta.after,
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut work = crate::incremental_world_native::family::WorldNativeFieldReadWorkV1 {
        records_remaining: 10000,
        trees_remaining: 100000,
        max_key_heap: 1 << 20,
        max_pending_records: 256,
        typed: read_work(),
    };
    let q = crate::incremental_world_native::family::qualify_world_native_field_v1::<
        GovernanceBootstrapFinalityKeyRotationsFieldV1,
    >(&roots, &mut ctx, &mut scan(), &mut work)
    .unwrap();
    assert_eq!(q.records(), 1);
    q.validate_binding(&roots).unwrap();
    let recovered =
        recover_world_native_record_v1::<GovernanceBootstrapFinalityKeyRotationsFieldV1>(
            &"bootstrap-one".into(),
            &mut ctx,
            &mut scan(),
            &mut read_work(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(recovered.value().value(), &original);
}
#[test]
fn bootstrap_source_rejects_wrong_outer_id_and_invalid_original_record() {
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let valid = record("bootstrap-one");
    assert!(
        GovernanceBootstrapFinalityKeyRotationsFieldV1::validate_source_record(
            &"other".into(),
            &valid,
            &memory,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    let mut invalid_record = valid;
    invalid_record.applied_at_tick += 1;
    assert!(
        GovernanceBootstrapFinalityKeyRotationsFieldV1::validate_source_record(
            &"bootstrap-one".into(),
            &invalid_record,
            &memory,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    assert_eq!(memory.used(), 0);
}
#[test]
fn admitted_plus_bootstrap_capacity_rejects_before_any_genesis_pages() {
    let _source = source_admission();
    let mut world = WorldState::default();
    for i in 0..129 {
        world
            .governance_finality_key_rotations
            .insert(format!("validator-{i}"), fixture::rotation());
    }
    for i in 0..128 {
        let id = format!("bootstrap-{i}");
        world
            .governance_bootstrap_finality_key_rotations
            .insert(id.clone(), record(&id));
    }
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let mut pages = Pages::new(&memory);
    let mut outputs = 0;
    let mut records = 1000;
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
#[test]
fn bootstrap_budget_and_cancellation_fail_without_emitting_a_record() {
    for cancel in [false, true] {
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        let mut pages = Pages::new(&memory);
        let mut records = 0;
        let mut outputs = 0;
        let value = record("bootstrap-one");
        assert!(
            prepare_world_native_record_v1::<GovernanceBootstrapFinalityKeyRotationsFieldV1>(
                &"bootstrap-one".into(),
                &value,
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
                    cancelled: || cancel
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
    }
}
