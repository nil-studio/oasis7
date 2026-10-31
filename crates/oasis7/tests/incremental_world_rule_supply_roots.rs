#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1, RootSetV1,
        TransactionLimits,
    },
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateScanBudget,
        StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError, write_value},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    CrisisState, CrisisStatus, WorldState,
    incremental_field_member::*,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_effects::*,
    incremental_world_rule_supply_root::*,
    world_rules::{SupplyAuthorizationV1, SupplyBatchV1, SupplyLineageV1, WorldRulesStateV1},
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        ..fixture::physical()
    }
}
#[allow(dead_code)]
#[path = "support/incremental_world_rules_fixture.rs"]
mod rules_fixture;
use rules_fixture::*;
const KEY: &str = "outer-record\0é";
const OLD: &str = "old-member\0é";
use oasis7_formal_protocol::{
    incremental_execution::TransitionVerifier, incremental_value::ValueNodeWriter,
};
#[path = "support/incremental_world_rule_supply_fixture.rs"]
mod supply_fixture;
use supply_fixture::{authorization, batch};
// The new fixed RootRef payload is 165 bytes; the old leaf fixture admitted only 128.
fn root_limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        payload_bytes: 165,
        ..limits()
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 100000,
        read_bytes_remaining: 256 << 20,
    }
}
fn roots(root: &StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("supply-qualified".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    roots
}
macro_rules! fixed {
    ($root:expr,$state:expr,$values:expr,$memory:expr,$io:expr,$keys:expr) => {
        WorldFieldReadContextV1 {
            root: $root,
            state: $state,
            values: ValueReadContext {
                source: $values,
                memory: $memory,
                class: MemoryClass::Ordinary,
                io: $io,
            },
            key_work: $keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || $state.take_read_error().map(state_error),
            cancelled: || false,
        }
    };
}
#[test]
fn fresh_supply_records_same_transaction_and_root_only_cold_successors() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("qualified");
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        let source = WorldState {
            world_rules: Some(WorldRulesStateV1 {
                governance_effects: [("p".into(), "original".into())].into(),
                ..WorldRulesStateV1::default()
            }),
            ..WorldState::default()
        };
        let before = build(&store, &memory, &produce(&store, &memory, &source));
        let parent = ExecutionParent {
            roots: roots(&before),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = fixed!(&before, &state, &values, &memory, &mut io, &mut keys);
        let family = qualify_supply_family_v1(
            &parent.roots,
            &mut read,
            &mut scan(),
            &mut scan(),
            &mut 4096,
            165,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [0, 0]);
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 96,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut overlay_io = logical();
        let mut overlay_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut overlay_io,
            },
            key_work: &mut overlay_keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: root_limits(),
            cancelled: || false,
        };
        let mut auth = authorization();
        auth.beneficiaries.clear();
        auth.requests.clear();
        let mut batch = batch();
        batch.frozen_lineages.clear();
        batch.demands.clear();
        batch.allocation.clear();
        let guard = overlay.transaction.guard();
        let mut auth_proof = create_qualified_supply_empty_v1(
            SupplyEmptyRootCreateV1 {
                owner: SupplyRootKeyV1::Authorization(KEY),
                source: SupplyRootSourceV1::Authorization(&auth),
                parent: &family,
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
            &mut 128,
        )
        .unwrap();
        let guard = overlay.transaction.guard();
        let mut batch_proof = create_qualified_supply_empty_v1(
            SupplyEmptyRootCreateV1 {
                owner: SupplyRootKeyV1::Batch(KEY),
                source: SupplyRootSourceV1::Batch(&batch),
                parent: &family,
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
            &mut 128,
        )
        .unwrap();
        for (owner, member, value, is_auth) in [
            (
                SupplyRootKeyV1::Authorization(KEY),
                SupplyMemberKeyV1::Beneficiary(OLD),
                SupplyMemberValueV1::Unit,
                true,
            ),
            (
                SupplyRootKeyV1::Authorization(KEY),
                SupplyMemberKeyV1::Request(OLD),
                SupplyMemberValueV1::I64(i64::MIN),
                true,
            ),
            (
                SupplyRootKeyV1::Batch(KEY),
                SupplyMemberKeyV1::Demand(OLD),
                SupplyMemberValueV1::I64(-9),
                false,
            ),
            (
                SupplyRootKeyV1::Batch(KEY),
                SupplyMemberKeyV1::Allocation(OLD),
                SupplyMemberValueV1::I64(0),
                false,
            ),
            (
                SupplyRootKeyV1::Batch(KEY),
                SupplyMemberKeyV1::FrozenLineage(OLD),
                SupplyMemberValueV1::Lineage {
                    target: -7,
                    fulfilled: 5,
                    reserved: 3,
                },
                false,
            ),
        ] {
            let proof = if is_auth {
                &mut auth_proof
            } else {
                &mut batch_proof
            };
            let guard = overlay.transaction.guard();
            *proof = change_qualified_supply_member_v1(
                SupplyMemberChangeV1 {
                    request: SupplyMemberRequestV1 {
                        owner,
                        member,
                        qualification: proof,
                    },
                    expected: None,
                    after: Some(value),
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let mut local_io = logical();
            let mut local_keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut local = fixed!(
                proof.root(),
                &state,
                &values,
                &memory,
                &mut local_io,
                &mut local_keys
            );
            assert_eq!(
                read_qualified_supply_member_v1(
                    SupplyMemberRequestV1 {
                        owner,
                        member,
                        qualification: proof
                    },
                    &mut local
                )
                .unwrap(),
                Some(value)
            );
        }
        // Scalar projections deliberately carry different nonempty member sources;
        // neither source may replace the persisted independent collections.
        let mut after = authorization();
        after.id = "new-auth\0é".into();
        after.remaining_escrow = i64::MAX;
        after.crisis_evidence.status = CrisisStatus::Active;
        after.crisis_evidence.resolver_agent_id = Some(String::new());
        after.crisis_evidence.success = None;
        let guard = overlay.transaction.guard();
        auth_proof = change_qualified_supply_scalars_v1(
            SupplyScalarChangeV1 {
                owner: SupplyRootKeyV1::Authorization(KEY),
                qualification: &auth_proof,
                expected: SupplyRootSourceV1::Authorization(&auth),
                after: SupplyRootSourceV1::Authorization(&after),
                max_current_payload_bytes: 128,
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        let mut local_io = logical();
        let mut local_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut local = fixed!(
            auth_proof.root(),
            &state,
            &values,
            &memory,
            &mut local_io,
            &mut local_keys
        );
        assert_eq!(
            read_qualified_supply_member_v1(
                SupplyMemberRequestV1 {
                    owner: SupplyRootKeyV1::Authorization(KEY),
                    member: SupplyMemberKeyV1::Request(OLD),
                    qualification: &auth_proof
                },
                &mut local
            )
            .unwrap(),
            Some(SupplyMemberValueV1::I64(i64::MIN))
        );
        let guard = overlay.transaction.guard();
        assert!(
            change_qualified_supply_scalars_v1(
                SupplyScalarChangeV1 {
                    owner: SupplyRootKeyV1::Authorization(KEY),
                    qualification: &auth_proof,
                    expected: SupplyRootSourceV1::Authorization(&after),
                    after: SupplyRootSourceV1::Authorization(&after),
                    max_current_payload_bytes: 128,
                    max_pending_node_bytes: 4 << 20
                },
                &mut overlay,
                &mut write,
                &guard,
                &live
            )
            .unwrap()
            .is_none()
        );
        let guard = overlay.transaction.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(prepared.record.writes.len(), 2);
        for node in &prepared.new_nodes {
            writer
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let after_root = prepared
            .record
            .after
            .state
            .iter()
            .find(|r| r.namespace == Ns::Authorization)
            .unwrap();
        let mut after_io = logical();
        let mut after_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = fixed!(
            after_root,
            &state,
            &values,
            &memory,
            &mut after_io,
            &mut after_keys
        );
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        assert_eq!(
            accept_supply_transition_v1(
                &family,
                &verifier,
                &prepared.record,
                &mut read,
                &mut scan(),
                &mut 4096,
                165
            )
            .unwrap()
            .record_counts(),
            [1, 1]
        );
        let canonical = prepared.record.after.canonical_bytes().unwrap();
        drop(prepared);
        drop(writer);
        drop(values);
        drop(state);
        drop(store);
        // No World or complete record snapshot is passed into cold qualification.
        let cold =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let state = cold
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = cold.value_session(physical());
        let cold_roots = RootSetV1::from_canonical_bytes(&canonical).unwrap();
        let root = cold_roots
            .state
            .iter()
            .find(|r| r.namespace == Ns::Authorization)
            .unwrap();
        let mut cold_io = logical();
        let mut cold_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = fixed!(root, &state, &values, &memory, &mut cold_io, &mut cold_keys);
        let family = qualify_supply_family_v1(
            &cold_roots,
            &mut read,
            &mut scan(),
            &mut scan(),
            &mut 4096,
            165,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [1, 1]);
        let proof = open_qualified_supply_record_v1(
            SupplyRootKeyV1::Batch(KEY),
            &cold_roots,
            &family,
            &mut read,
        )
        .unwrap()
        .unwrap();
        let mut point_io = logical();
        let mut point_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut point = fixed!(
            proof.root(),
            &state,
            &values,
            &memory,
            &mut point_io,
            &mut point_keys
        );
        assert_eq!(
            read_qualified_supply_member_v1(
                SupplyMemberRequestV1 {
                    owner: SupplyRootKeyV1::Batch(KEY),
                    member: SupplyMemberKeyV1::FrozenLineage(OLD),
                    qualification: &proof
                },
                &mut point
            )
            .unwrap(),
            Some(SupplyMemberValueV1::Lineage {
                target: -7,
                fulfilled: 5,
                reserved: 3
            })
        );
    }
}
