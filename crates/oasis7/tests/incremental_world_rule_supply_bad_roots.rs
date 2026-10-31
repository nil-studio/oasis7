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
#[path = "support/incremental_world_rule_supply_fixture.rs"]
mod supply_fixture;
use supply_fixture::{authorization, batch};
#[path = "support/incremental_world_rule_supply_semantic_oracle.rs"]
mod semantic_oracle;
use oasis7_formal_protocol::{
    incremental_execution::TransitionVerifier, incremental_value::ValueNodeWriter,
};
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
use semantic_oracle::{Path, semantic};
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 100000,
        read_bytes_remaining: 256 << 20,
    }
}
fn roots(root: &StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("supply-bad-roots".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    roots
}
fn key(path: Path<'_>) -> WorldFieldKeyV1 {
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic(path)).as_bytes()),
    )
    .unwrap()
}
macro_rules! replacement {
    ($key:expr,$payload:expr,$write:expr $(,)?) => {
        write_member_record_v1(
            descriptor(),
            WorldFieldRecordKindV1::Entry,
            $key,
            $payload,
            $write,
        )
        .unwrap()
    };
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
fn raw_and_incremental_supply_closure_reject_missing_columns_unknowns_and_partial_frozen_deletion()
{
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("bad-roots"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let source = WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            supply_authorizations: [(KEY.into(), authorization())].into(),
            supply_batches: [(KEY.into(), batch())].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    };
    let mut rows = produce(&store, &memory, &source);
    let mut writer = store.value_session(physical());
    {
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                payload_bytes: 165,
                ..limits()
            },
            cancelled: || false,
        };
        prepare_supply_root_genesis_v1(&source, &mut write, &mut 128, 4 << 20, &mut |row| {
            rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
            Ok(())
        })
        .unwrap();
    }
    let before = build(&store, &memory, &rows);
    let parent = ExecutionParent {
        roots: roots(&before),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let initial = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut fixed_io = logical();
    let mut fixed_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = fixed!(
        &before,
        &initial,
        &values,
        &memory,
        &mut fixed_io,
        &mut fixed_keys
    );
    let family = qualify_supply_family_v1(
        &parent.roots,
        &mut read,
        &mut scan(),
        &mut scan(),
        &mut 4096,
        165,
    )
    .unwrap();
    let auth = open_qualified_supply_record_v1(
        SupplyRootKeyV1::Authorization(KEY),
        &parent.roots,
        &family,
        &mut read,
    )
    .unwrap()
    .unwrap();
    let batch = open_qualified_supply_record_v1(
        SupplyRootKeyV1::Batch(KEY),
        &parent.roots,
        &family,
        &mut read,
    )
    .unwrap()
    .unwrap();
    for case in 0..16 {
        let owner = if case < 9 || case == 14 {
            SupplyRootKeyV1::Authorization(KEY)
        } else {
            SupplyRootKeyV1::Batch(KEY)
        };
        let child = if case < 9 || case == 14 {
            auth.root()
        } else {
            batch.root()
        };
        let prepare = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let _admission = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                payload_bytes: 165,
                ..limits()
            },
            cancelled: || false,
        };
        let paths = match case {
            0 => vec![Path::Record],
            1 => vec![Path::Field("id")],
            2 => vec![Path::Meta("requests")],
            3 => vec![Path::Flag("success")],
            4 => vec![Path::Value("strategy")],
            5 => vec![Path::Field("active")],
            6 => vec![Path::Field("policy_version")],
            7 => vec![Path::Crisis("status")],
            8 => vec![Path::Field("unknown")],
            9 => vec![Path::Meta("demands")],
            10 => vec![Path::Member("frozen_lineages", OLD)],
            11 => vec![Path::Frozen(OLD, "fulfilled")],
            12 => vec![Path::Frozen(OLD, "target")],
            13 => vec![
                Path::Member("frozen_lineages", OLD),
                Path::Frozen(OLD, "target"),
                Path::Frozen(OLD, "fulfilled"),
                Path::Frozen(OLD, "reserved"),
            ],
            14 => vec![],
            15 => vec![Path::Field("unknown")],
            _ => unreachable!(),
        };
        let row = match case {
            4 => Some(replacement!(
                &((((), "crisis_evidence"), "strategy"), "value"),
                MemberPayloadRefV1::Text(""),
                &mut write,
            )),
            5 => Some(replacement!(
                &((), "active"),
                MemberPayloadRefV1::I64(2),
                &mut write,
            )),
            6 => Some(replacement!(
                &((), "policy_version"),
                MemberPayloadRefV1::U64(u64::from(u32::MAX) + 1),
                &mut write,
            )),
            7 => Some(replacement!(
                &(((), "crisis_evidence"), "status"),
                MemberPayloadRefV1::Text("unknown"),
                &mut write,
            )),
            8 | 15 => Some(replacement!(
                &((), "unknown"),
                MemberPayloadRefV1::Text("extra"),
                &mut write,
            )),
            12 => Some(replacement!(
                &((((), "frozen_lineages"), OLD), "target"),
                MemberPayloadRefV1::Text("wrong-codec"),
                &mut write,
            )),
            _ => None,
        };
        let mut changes = Vec::new();
        for path in paths {
            let key = key(path);
            let before = child.get(key.as_bytes(), &prepare).unwrap();
            if let Some(row) = &row {
                assert_eq!(row.field_key, key);
            }
            changes.push(StateWriteV1 {
                key: key.as_bytes().to_vec(),
                before,
                after: row.as_ref().map(|r| r.value_root),
            });
        }
        changes.sort_by(|a, b| a.key.cmp(&b.key));
        let edge = if case == 14 {
            None
        } else {
            let delta = _admission
                .retain(StateDeltaV1::build(child.clone(), changes, &prepare, 4 << 20).unwrap());
            write.sink.put_state_pages(&delta, &memory).unwrap();
            Some(write_supply_root_reference_v1(owner, &delta.after, &mut write).unwrap())
        };
        let outer = if let Some(row) = &edge {
            row.field_key.clone()
        } else {
            WorldFieldKeyV1::new(
                descriptor(),
                WorldFieldRecordKindV1::Entry,
                Some(
                    *blake3::hash(&{
                        let mut b = b"oasis7.iws.semantic-key.v1\0".to_vec();
                        b.push(5);
                        for s in ["supply_authorizations", KEY] {
                            b.push(1);
                            b.extend_from_slice(&(s.len() as u64).to_be_bytes());
                            b.extend(s.as_bytes());
                        }
                        b
                    })
                    .as_bytes(),
                ),
            )
            .unwrap()
        };
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &prepare,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 96,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        assert!(
            tx.read(Ns::Authorization, outer.as_bytes())
                .unwrap()
                .is_some()
        );
        tx.write(
            Ns::Authorization,
            outer.as_bytes(),
            edge.as_ref().map(|r| r.value_root),
        )
        .unwrap();
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
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
        let root = prepared
            .record
            .after
            .state
            .iter()
            .find(|r| r.namespace == Ns::Authorization)
            .unwrap();
        // Independent complete raw validation and normal transition acceptance each
        // retain one continuous finite scope, without resetting within an API call.
        let raw_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut raw_io = logical();
        let mut raw_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut raw = fixed!(
            root,
            &raw_state,
            &values,
            &memory,
            &mut raw_io,
            &mut raw_keys
        );
        let result = qualify_supply_family_v1(
            &prepared.record.after,
            &mut raw,
            &mut scan(),
            &mut scan(),
            &mut 4096,
            165,
        );
        if case == 13 || case == 14 {
            assert_eq!(
                result.unwrap().record_counts(),
                if case == 13 { [1, 1] } else { [0, 1] },
                "raw case {case}"
            );
        } else {
            assert!(
                matches!(result, Err(ValueTreeError::Invalid(_))),
                "raw case {case}: {result:?}"
            );
        }
        let verify_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &verify_state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        verifier.verify(&prepared.record).unwrap();
        let mut accepted_io = logical();
        let mut accepted_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut accepted = fixed!(
            root,
            &verify_state,
            &values,
            &memory,
            &mut accepted_io,
            &mut accepted_keys
        );
        let result = accept_supply_transition_v1(
            &family,
            &verifier,
            &prepared.record,
            &mut accepted,
            &mut scan(),
            &mut 4096,
            165,
        );
        if case == 13 {
            assert!(result.is_ok(), "receiver case {case}: {result:?}");
        } else {
            assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
                ),
                "receiver case {case}: {result:?}"
            );
        }
    }
}
