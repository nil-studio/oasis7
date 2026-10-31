#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*,
    incremental_world_number_maps::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[path = "support/incremental_number_maps_noop_cancel.rs"]
mod noop_cancel;
#[test]
fn numeric_maps_keep_original_key_value_types_and_absence_after_reopen() {
    use WorldNumberMapFieldV1 as Field;
    use WorldNumberMapKeyRefV1 as Key;
    use WorldNumberV1 as Number;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let source = WorldState {
            industry_settlement_orders: [(0, 0), (u64::MAX, u64::MAX)].into(),
            agent_claim_acquisition_sequences: [("".into(), 17)].into(),
            contract_pair_last_success_settled_at: [("pair/一".into(), u64::MAX)].into(),
            reputation_reward_window_started_at: Default::default(),
            reputation_reward_window_accumulated: [("acct".into(), i64::MIN), ("".into(), 0)]
                .into(),
            reputation_scores: [("acct".into(), i64::MAX), ("zero".into(), 0)].into(),
            main_token_treasury_balances: [("treasury".into(), 0)].into(),
            main_token_claim_nonces: [("claim".into(), 17)].into(),
            main_token_transfer_nonces: [("transfer".into(), u64::MAX)].into(),
            node_redeem_nonces: [("node".into(), 7)].into(),
            governance_authorization_nonces: [("governance".into(), 9)].into(),
            ..WorldState::default()
        };
        let roots = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let mut values = store.value_session(physical());
            let _batch = producer.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(24);
            assert_eq!(
                prepare_world_number_maps_genesis_v1(
                    &source,
                    &mut MemberRecordWriteContext {
                        sink: &mut values,
                        memory: &producer,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 4096
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 8,
                            record_bytes: 4096
                        },
                        cancelled: || false
                    },
                    &mut 24,
                    &mut |row| {
                        assert!(rows.len() < 24);
                        rows.push(row);
                        Ok(())
                    }
                )
                .unwrap(),
                13
            );
            // Literal source-field oracle, independent of the adapter enum.
            for (name, source_key, namespace, codec, expected) in [
                (
                    "reputation_reward_window_accumulated",
                    "acct",
                    StateNamespaceV1::Business,
                    MemberPayloadCodecV1::I64,
                    i64::MIN.to_be_bytes(),
                ),
                (
                    "reputation_scores",
                    "acct",
                    StateNamespaceV1::Business,
                    MemberPayloadCodecV1::I64,
                    i64::MAX.to_be_bytes(),
                ),
                (
                    "main_token_treasury_balances",
                    "treasury",
                    StateNamespaceV1::Business,
                    MemberPayloadCodecV1::U64,
                    0u64.to_be_bytes(),
                ),
                (
                    "main_token_claim_nonces",
                    "claim",
                    StateNamespaceV1::Authorization,
                    MemberPayloadCodecV1::U64,
                    17u64.to_be_bytes(),
                ),
                (
                    "main_token_transfer_nonces",
                    "transfer",
                    StateNamespaceV1::Authorization,
                    MemberPayloadCodecV1::U64,
                    u64::MAX.to_be_bytes(),
                ),
                (
                    "node_redeem_nonces",
                    "node",
                    StateNamespaceV1::Authorization,
                    MemberPayloadCodecV1::U64,
                    7u64.to_be_bytes(),
                ),
                (
                    "governance_authorization_nonces",
                    "governance",
                    StateNamespaceV1::Authorization,
                    MemberPayloadCodecV1::U64,
                    9u64.to_be_bytes(),
                ),
            ] {
                let mut key = Vec::from(&b"oasis7.iws.semantic-key.v1\0"[..]);
                key.push(1);
                key.extend_from_slice(&(source_key.len() as u64).to_be_bytes());
                key.extend_from_slice(source_key.as_bytes());
                let field = FieldDescriptor {
                    owner: FieldOwner::WorldState,
                    name,
                    destination: FieldDestination::State(namespace),
                    shape: FieldShape::Map,
                };
                let expected_key = WorldFieldKeyV1::new(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    Some(*blake3::hash(&key).as_bytes()),
                )
                .unwrap();
                let row = rows
                    .iter()
                    .find(|row| row.mutation.key == expected_key.as_bytes())
                    .unwrap();
                let mut payload = [0; 8];
                read_member_record_profile_v1(
                    &row.mutation.after.unwrap(),
                    &expected_key,
                    &mut ValueReadContext {
                        source: &values,
                        memory: &producer,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    MemberReadProfileV1 {
                        codec,
                        semantic_bytes: key.len() as u64,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 8,
                            record_bytes: 4096,
                        },
                    },
                    &mut std::io::sink(),
                    &mut &mut payload[..],
                    || false,
                )
                .unwrap();
                assert_eq!(payload, expected);
            }
            let bad = write_member_record_v1(
                FieldDescriptor {
                    owner: FieldOwner::WorldState,
                    name: "reputation_scores",
                    destination: FieldDestination::State(StateNamespaceV1::Business),
                    shape: FieldShape::Map,
                },
                WorldFieldRecordKindV1::Entry,
                "acct",
                MemberPayloadRefV1::U64(i64::MAX as u64),
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &producer,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            [
                (StateNamespaceV1::Allocator, false),
                (StateNamespaceV1::Business, false),
                (StateNamespaceV1::Business, true),
                (StateNamespaceV1::Authorization, false),
            ]
            .map(|(namespace, wrong_codec)| {
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let workspace = producer.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
                let mut changes: Vec<_> = rows
                    .iter()
                    .filter(|row| row.namespace == namespace)
                    .map(|row| StateWriteV1 {
                        key: row.mutation.key.clone(),
                        before: None,
                        after: if wrong_codec && row.mutation.key == bad.field_key.as_bytes() {
                            Some(bad.value_root)
                        } else {
                            row.mutation.after
                        },
                    })
                    .collect();
                changes.sort_by(|a, b| a.key.cmp(&b.key));
                let delta = workspace.retain(
                    StateDeltaV1::build(
                        StateRootV1::empty(namespace).unwrap(),
                        changes,
                        &state,
                        2 << 20,
                    )
                    .unwrap(),
                );
                assert_eq!(
                    delta.after.leaf_count,
                    if namespace == StateNamespaceV1::Allocator {
                        5
                    } else if namespace == StateNamespaceV1::Business {
                        11
                    } else {
                        8
                    }
                );
                store
                    .value_session(physical())
                    .put_state_pages(&delta, &producer)
                    .unwrap();
                delta.after.canonical_bytes().unwrap()
            })
        };
        assert_eq!(producer.used(), 0);
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _filler = if class == MemoryClass::Recovery {
                Some(
                    memory
                        .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                        .unwrap(),
                )
            } else {
                None
            };
            for (field, key, expected) in [
                (
                    Field::MainTokenTreasuryBalances,
                    Key::Text("treasury"),
                    Some(Number::Unsigned(0)),
                ),
                (Field::MainTokenTreasuryBalances, Key::Text("missing"), None),
                (
                    Field::MainTokenClaimNonces,
                    Key::Text("claim"),
                    Some(Number::Unsigned(17)),
                ),
                (Field::MainTokenClaimNonces, Key::Text("missing"), None),
                (
                    Field::MainTokenTransferNonces,
                    Key::Text("transfer"),
                    Some(Number::Unsigned(u64::MAX)),
                ),
                (Field::MainTokenTransferNonces, Key::Text("missing"), None),
                (
                    Field::NodeRedeemNonces,
                    Key::Text("node"),
                    Some(Number::Unsigned(7)),
                ),
                (Field::NodeRedeemNonces, Key::Text("missing"), None),
                (
                    Field::GovernanceAuthorizationNonces,
                    Key::Text("governance"),
                    Some(Number::Unsigned(9)),
                ),
                (
                    Field::GovernanceAuthorizationNonces,
                    Key::Text("missing"),
                    None,
                ),
                (
                    Field::IndustrySettlementOrders,
                    Key::Action(0),
                    Some(Number::Unsigned(0)),
                ),
                (
                    Field::IndustrySettlementOrders,
                    Key::Action(u64::MAX),
                    Some(Number::Unsigned(u64::MAX)),
                ),
                (Field::IndustrySettlementOrders, Key::Action(1), None),
                (
                    Field::AgentClaimAcquisitionSequences,
                    Key::Text(""),
                    Some(Number::Unsigned(17)),
                ),
                (
                    Field::ContractPairLastSuccess,
                    Key::Text("pair/一"),
                    Some(Number::Unsigned(u64::MAX)),
                ),
                (Field::ReputationWindowStarted, Key::Text("empty"), None),
                (
                    Field::ReputationWindowAccumulated,
                    Key::Text("acct"),
                    Some(Number::Signed(i64::MIN)),
                ),
                (
                    Field::ReputationWindowAccumulated,
                    Key::Text(""),
                    Some(Number::Signed(0)),
                ),
                (
                    Field::ReputationScores,
                    Key::Text("acct"),
                    Some(Number::Signed(i64::MAX)),
                ),
                (
                    Field::ReputationScores,
                    Key::Text("zero"),
                    Some(Number::Signed(0)),
                ),
                (Field::ReputationScores, Key::Text("absent"), None),
            ] {
                let index = match field.namespace() {
                    StateNamespaceV1::Allocator => 0,
                    StateNamespaceV1::Business => 1,
                    StateNamespaceV1::Authorization => 3,
                    _ => unreachable!(),
                };
                let root = StateRootV1::from_canonical_bytes(&roots[index]).unwrap();
                let wrong =
                    StateRootV1::from_canonical_bytes(&roots[if index == 0 { 1 } else { 0 }])
                        .unwrap();
                let empty = StateRootV1::empty(field.namespace()).unwrap();
                let bad = StateRootV1::from_canonical_bytes(&roots[2]).unwrap();
                let state = store.state_session(physical(), class).unwrap();
                let values = store.value_session(physical());
                let mut read = WorldFieldReadContextV1 {
                    root: &root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || {
                        state.take_read_error().map(|error| {
                            eprintln!(
                                "number map state source error: {error:?}, remaining {:?}",
                                state.remaining_io()
                            );
                            ValueTreeError::Invalid("state page failure")
                        })
                    },
                    cancelled: || false,
                };
                assert_eq!(
                    read_world_number_map_v1(field, key, &mut read).unwrap(),
                    expected
                );
                let wrong_key = match key {
                    Key::Text(_) => Key::Action(0),
                    Key::Action(_) => Key::Text("0"),
                };
                assert!(matches!(
                    read_world_number_map_v1(field, wrong_key, &mut read),
                    Err(ValueTreeError::Invalid("world number map key type"))
                ));
                read.root = &wrong;
                assert!(matches!(
                    read_world_number_map_v1(field, key, &mut read),
                    Err(ValueTreeError::Invalid("field namespace"))
                ));
                read.root = &empty;
                assert!(matches!(
                    read_world_number_map_v1(field, key, &mut read),
                    Err(ValueTreeError::Invalid("world number map missing metadata"))
                ));
                if field == Field::ReputationScores && matches!(key, Key::Text("acct")) {
                    read.root = &bad;
                    assert!(matches!(
                        read_world_number_map_v1(field, key, &mut read),
                        Err(ValueTreeError::Invalid("member expected profile"))
                    ));
                }
            }
        }
        let next = {
            use oasis7_formal_protocol::incremental_execution::{
                AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1,
                TransactionLimits, TransitionVerifier,
            };
            use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
            use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let mut root_set = RootSetV1::empty("number-maps".into(), [3; 32]).unwrap();
            for bytes in [&roots[0], &roots[1], &roots[3]] {
                let root = StateRootV1::from_canonical_bytes(bytes).unwrap();
                let namespace = root.namespace;
                *root_set
                    .state
                    .iter_mut()
                    .find(|root| root.namespace == namespace)
                    .unwrap() = root;
            }
            let parent = ExecutionParent {
                commit: [4; 32],
                roots: root_set,
                authority: AuthorityBinding {
                    token: [5; 32],
                    epoch: 7,
                },
            };
            let live = parent.fence().unwrap();
            let limits = TransactionLimits {
                max_reads: 32,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            };
            // This fixture now exercises 22 dependencies and eight writes across
            // three roots. Keep one finite allowance for the whole contribution.
            let mut transaction_io = physical();
            transaction_io.reads_remaining = 500_000;
            let state = store
                .state_session(transaction_io.clone(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
                    .unwrap();
            let values = store.value_session(physical());
            let mut writer = store.value_session(physical());
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || {
                        state.take_read_error().map(|error| {
                            eprintln!(
                                "number map state source error: {error:?}, remaining {:?}",
                                state.remaining_io()
                            );
                            ValueTreeError::Invalid("state page failure")
                        })
                    },
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 8192,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                };
                for (field, key, expected, after, changed) in [
                    (
                        Field::MainTokenTreasuryBalances,
                        Key::Text("treasury"),
                        Some(Number::Unsigned(0)),
                        Some(Number::Unsigned(42)),
                        true,
                    ),
                    (
                        Field::MainTokenClaimNonces,
                        Key::Text("claim"),
                        Some(Number::Unsigned(17)),
                        Some(Number::Unsigned(u64::MAX)),
                        true,
                    ),
                    (
                        Field::MainTokenTransferNonces,
                        Key::Text("transfer"),
                        Some(Number::Unsigned(u64::MAX)),
                        None,
                        true,
                    ),
                    (
                        Field::NodeRedeemNonces,
                        Key::Text("node"),
                        Some(Number::Unsigned(7)),
                        Some(Number::Unsigned(0)),
                        true,
                    ),
                    (
                        Field::GovernanceAuthorizationNonces,
                        Key::Text("governance"),
                        Some(Number::Unsigned(9)),
                        Some(Number::Unsigned(10)),
                        true,
                    ),
                    (
                        Field::IndustrySettlementOrders,
                        Key::Action(0),
                        Some(Number::Unsigned(0)),
                        Some(Number::Unsigned(3)),
                        true,
                    ),
                    (
                        Field::IndustrySettlementOrders,
                        Key::Action(0),
                        Some(Number::Unsigned(3)),
                        Some(Number::Unsigned(5)),
                        true,
                    ),
                    (
                        Field::IndustrySettlementOrders,
                        Key::Action(0),
                        Some(Number::Unsigned(5)),
                        None,
                        true,
                    ),
                    (
                        Field::IndustrySettlementOrders,
                        Key::Action(0),
                        None,
                        Some(Number::Unsigned(0)),
                        true,
                    ),
                    (
                        Field::AgentClaimAcquisitionSequences,
                        Key::Text(""),
                        Some(Number::Unsigned(17)),
                        Some(Number::Unsigned(u64::MAX)),
                        true,
                    ),
                    (
                        Field::ReputationScores,
                        Key::Text("zero"),
                        Some(Number::Signed(0)),
                        None,
                        true,
                    ),
                    (
                        Field::ReputationScores,
                        Key::Text("zero"),
                        None,
                        Some(Number::Signed(0)),
                        true,
                    ),
                    (
                        Field::ReputationWindowStarted,
                        Key::Text("epoch"),
                        None,
                        Some(Number::Unsigned(0)),
                        true,
                    ),
                    (
                        Field::ReputationWindowAccumulated,
                        Key::Text("acct"),
                        Some(Number::Signed(i64::MIN)),
                        Some(Number::Signed(i64::MAX)),
                        true,
                    ),
                    (
                        Field::ContractPairLastSuccess,
                        Key::Text("pair/一"),
                        Some(Number::Unsigned(u64::MAX)),
                        Some(Number::Unsigned(u64::MAX)),
                        false,
                    ),
                ] {
                    let guard = read.transaction.guard();
                    assert_eq!(
                        change_world_number_map_overlay_v1(
                            WorldNumberMapChangeV1 {
                                field,
                                key,
                                expected,
                                after
                            },
                            &mut read,
                            &mut write,
                            &guard,
                            &live
                        )
                        .unwrap_or_else(|error| panic!(
                            "number map update {field:?}/{key:?}: {error:?}"
                        )),
                        changed
                    );
                    assert_eq!(
                        read_world_number_map_overlay_v1(field, key, &mut read).unwrap(),
                        after
                    );
                }
            }
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.reads.len(), 22);
            assert_eq!(sealed.record.writes.len(), 8);
            assert_eq!(
                sealed
                    .record
                    .writes
                    .iter()
                    .filter(|write| write.namespace == StateNamespaceV1::Authorization)
                    .count(),
                4
            );
            let verifier = store
                .state_session(transaction_io, MemoryClass::Ordinary)
                .unwrap();
            let verified = TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &verifier,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&sealed.record)
            .unwrap();
            assert_eq!(verified.record.after, sealed.record.after);
            eprintln!(
                "number maps transaction remaining {:?}; verifier remaining {:?}",
                state.remaining_io(),
                verifier.remaining_io()
            );
            let mut pages = store.value_session(physical());
            for node in &verified.new_nodes {
                let charge = memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                pages
                    .put_node(
                        *blake3::hash(&node.bytes).as_bytes(),
                        charge.retain(node.bytes.clone()),
                    )
                    .unwrap();
            }
            for case in 0..4 {
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let mut transaction =
                    ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
                        .unwrap();
                {
                    let mut read = WorldFieldOverlayContextV1 {
                        transaction: &mut transaction,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut logical(),
                        },
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 8192,
                        },
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || {
                            state.take_read_error().map(|error| {
                                eprintln!(
                                    "number map state source error: {error:?}, remaining {:?}",
                                    state.remaining_io()
                                );
                                ValueTreeError::Invalid("state page failure")
                            })
                        },
                        cancelled: || false,
                    };
                    let mut write = MemberRecordWriteContext {
                        sink: &mut writer,
                        memory: &memory,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 8192,
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 8,
                            record_bytes: 4096,
                        },
                        cancelled: || false,
                    };
                    let guard = read.transaction.guard();
                    change_world_number_map_overlay_v1(
                        WorldNumberMapChangeV1 {
                            field: Field::AgentClaimAcquisitionSequences,
                            key: Key::Text(""),
                            expected: Some(Number::Unsigned(17)),
                            after: Some(Number::Unsigned(18)),
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live,
                    )
                    .unwrap();
                    let (field, key, expected, after, reason) = match case {
                        0 => (
                            Field::ReputationWindowAccumulated,
                            Key::Text("acct"),
                            Some(Number::Signed(0)),
                            None,
                            "world number map expected value",
                        ),
                        1 => (
                            Field::IndustrySettlementOrders,
                            Key::Text("0"),
                            Some(Number::Unsigned(0)),
                            None,
                            "world number map key type",
                        ),
                        2 => (
                            Field::ReputationScores,
                            Key::Text("zero"),
                            Some(Number::Unsigned(0)),
                            Some(Number::Signed(1)),
                            "world number map value type",
                        ),
                        _ => (
                            Field::ReputationScores,
                            Key::Text("zero"),
                            Some(Number::Signed(0)),
                            Some(Number::Unsigned(1)),
                            "world number map value type",
                        ),
                    };
                    let guard = read.transaction.guard();
                    let result = change_world_number_map_overlay_v1(
                        WorldNumberMapChangeV1 {
                            field,
                            key,
                            expected,
                            after,
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live,
                    );
                    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1 as Error;
                    assert!(
                        matches!(result,Err(Error::Value(ValueTreeError::Invalid(actual)))if actual==reason)
                    );
                }
                let guard = transaction.guard();
                assert!(transaction.seal(&guard, &live, 1).is_err());
            }
            [
                StateNamespaceV1::Allocator,
                StateNamespaceV1::Business,
                StateNamespaceV1::Authorization,
            ]
            .map(|namespace| {
                verified
                    .record
                    .after
                    .state
                    .iter()
                    .find(|root| root.namespace == namespace)
                    .unwrap()
                    .canonical_bytes()
                    .unwrap()
            })
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            for newer in [false, true] {
                for (field, key, expected) in [
                    (
                        Field::MainTokenTreasuryBalances,
                        Key::Text("treasury"),
                        if newer {
                            Some(Number::Unsigned(42))
                        } else {
                            Some(Number::Unsigned(0))
                        },
                    ),
                    (
                        Field::MainTokenClaimNonces,
                        Key::Text("claim"),
                        if newer {
                            Some(Number::Unsigned(u64::MAX))
                        } else {
                            Some(Number::Unsigned(17))
                        },
                    ),
                    (
                        Field::MainTokenTransferNonces,
                        Key::Text("transfer"),
                        if newer {
                            None
                        } else {
                            Some(Number::Unsigned(u64::MAX))
                        },
                    ),
                    (
                        Field::NodeRedeemNonces,
                        Key::Text("node"),
                        if newer {
                            Some(Number::Unsigned(0))
                        } else {
                            Some(Number::Unsigned(7))
                        },
                    ),
                    (
                        Field::GovernanceAuthorizationNonces,
                        Key::Text("governance"),
                        if newer {
                            Some(Number::Unsigned(10))
                        } else {
                            Some(Number::Unsigned(9))
                        },
                    ),
                    (
                        Field::IndustrySettlementOrders,
                        Key::Action(0),
                        Some(Number::Unsigned(0)),
                    ),
                    (
                        Field::AgentClaimAcquisitionSequences,
                        Key::Text(""),
                        Some(Number::Unsigned(if newer { u64::MAX } else { 17 })),
                    ),
                    (
                        Field::ReputationWindowStarted,
                        Key::Text("epoch"),
                        if newer {
                            Some(Number::Unsigned(0))
                        } else {
                            None
                        },
                    ),
                    (
                        Field::ReputationWindowAccumulated,
                        Key::Text("acct"),
                        Some(Number::Signed(if newer { i64::MAX } else { i64::MIN })),
                    ),
                    (
                        Field::ReputationScores,
                        Key::Text("zero"),
                        Some(Number::Signed(0)),
                    ),
                ] {
                    let index = match field.namespace() {
                        StateNamespaceV1::Allocator => 0,
                        StateNamespaceV1::Business => 1,
                        StateNamespaceV1::Authorization => 2,
                        _ => unreachable!(),
                    };
                    let root = StateRootV1::from_canonical_bytes(if newer {
                        &next[index]
                    } else {
                        &roots[if index == 2 { 3 } else { index }]
                    })
                    .unwrap();
                    let state = store.state_session(physical(), class).unwrap();
                    let values = store.value_session(physical());
                    let mut read = WorldFieldReadContextV1 {
                        root: &root,
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut logical(),
                        },
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 4096,
                        },
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || {
                            state.take_read_error().map(|error| {
                                eprintln!(
                                    "number map state source error: {error:?}, remaining {:?}",
                                    state.remaining_io()
                                );
                                ValueTreeError::Invalid("state page failure")
                            })
                        },
                        cancelled: || false,
                    };
                    assert_eq!(
                        read_world_number_map_v1(field, key, &mut read).unwrap(),
                        expected
                    );
                }
            }
        }
    }
}
