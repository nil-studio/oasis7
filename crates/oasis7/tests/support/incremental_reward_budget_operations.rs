use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
fn open(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    key: RewardRecordKeyV1,
    family: &ReservedValue<RewardFamilyQualificationV1>,
) -> ReservedValue<RewardRecordQualificationV1> {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut c = ctx!(
        roots
            .state
            .iter()
            .find(|r| r.namespace == key.kind.namespace())
            .unwrap(),
        &state,
        &values,
        memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    open_qualified_reward_record_v1(key, roots, family, &mut c)
        .unwrap()
        .unwrap()
}
#[test]
fn same_transaction_map_updates_multiple_appends_and_typed_receiver_preserve_order() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("ops"), memory.clone(), compression)
                .unwrap();
        let roots = seed(&store, &memory, &source());
        let parent = ExecutionParent {
            roots: roots.clone(),
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
        let mut c = ctx!(
            &roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family =
            qualify_reward_family_v1(&roots, &mut c, &mut scan(), &mut scan(), &mut 8192, 165)
                .unwrap();
        let pool = RewardRecordKeyV1 {
            kind: RewardRecordKindV1::Pool,
            index: 7,
        };
        let mut proof = open(&store, &memory, &roots, pool, &family);
        let deleted = open(
            &store,
            &memory,
            &roots,
            RewardRecordKeyV1 {
                kind: RewardRecordKindV1::Pool,
                index: 0,
            },
            &family,
        );
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 200,
                max_writes: 32,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut read_io = logical();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
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
            limits: limits(),
            cancelled: || false,
        };
        for (cell, expected, after) in [
            (
                RewardCellV1::Column(RewardColumnV1::RemainingCreditBudget),
                Some(RewardValueRefV1::U64(u64::MAX)),
                Some(RewardValueRefV1::U64(0)),
            ),
            (
                RewardCellV1::Member(RewardCollectionV1::NodeCreditCaps, ""),
                Some(RewardValueRefV1::U64(0)),
                Some(RewardValueRefV1::U64(u64::MAX)),
            ),
            (
                RewardCellV1::Member(RewardCollectionV1::NodeCreditAllocated, " Ω "),
                None,
                Some(RewardValueRefV1::U64(0)),
            ),
            (
                RewardCellV1::Member(RewardCollectionV1::NodeCreditAllocated, " Ω "),
                Some(RewardValueRefV1::U64(0)),
                None,
            ),
        ] {
            let guard = overlay.transaction.guard();
            proof = change_qualified_reward_cell_v1(
                RewardCellChangeV1 {
                    key: pool,
                    cell,
                    qualification: &proof,
                    expected,
                    after,
                    max_current_payload_bytes: 165,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let guard = overlay.transaction.guard();
            let got = read_qualified_reward_cell_overlay_v1(
                RewardCellRequestV1 {
                    key: pool,
                    cell,
                    qualification: &proof,
                },
                &mut overlay,
                165,
                &mut std::io::sink(),
                &guard,
                &live,
            )
            .unwrap();
            assert_eq!(
                got,
                after.map(|v| match v {
                    RewardValueRefV1::U64(v) => RewardValueV1::Unsigned(v),
                    _ => unreachable!(),
                })
            );
            let guard = overlay.transaction.guard();
            assert!(
                change_qualified_reward_cell_v1(
                    RewardCellChangeV1 {
                        key: pool,
                        cell,
                        qualification: &proof,
                        expected: after,
                        after,
                        max_current_payload_bytes: 165,
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
        }
        let mint = NodeRewardMintRecord {
            epoch_index: 9,
            node_id: " next node ".into(),
            source_awarded_points: 0,
            minted_power_credits: 1,
            settlement_hash: "next hash".into(),
            signer_node_id: "other".into(),
            signature: " next signature ".into(),
        };
        for index in [3, 4] {
            let guard = overlay.transaction.guard();
            let appended = append_qualified_reward_mint_v1(
                RewardMintAppendV1 {
                    expected_length: index,
                    source: &mint,
                    family: &family,
                },
                &mut overlay,
                &mut write,
                &mut RewardRecordsWorkV1 {
                    cells_remaining: 100,
                    max_pending_node_bytes: 2 << 20,
                },
                &guard,
                &live,
            )
            .unwrap();
            let key = RewardRecordKeyV1 {
                kind: RewardRecordKindV1::Mint,
                index,
            };
            let guard = overlay.transaction.guard();
            assert_eq!(
                read_qualified_reward_cell_overlay_v1(
                    RewardCellRequestV1 {
                        key,
                        cell: RewardCellV1::Column(RewardColumnV1::EpochIndex),
                        qualification: &appended
                    },
                    &mut overlay,
                    165,
                    &mut std::io::sink(),
                    &guard,
                    &live
                )
                .unwrap(),
                Some(RewardValueV1::Unsigned(9))
            );
            if index == 3 {
                let guard = overlay.transaction.guard();
                change_qualified_reward_cell_v1(
                    RewardCellChangeV1 {
                        key,
                        cell: RewardCellV1::Column(RewardColumnV1::SourceAwardedPoints),
                        qualification: &appended,
                        expected: Some(RewardValueRefV1::U64(0)),
                        after: Some(RewardValueRefV1::U64(u64::MAX)),
                        max_current_payload_bytes: 165,
                        max_pending_node_bytes: 4 << 20,
                    },
                    &mut overlay,
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap()
                .unwrap();
            }
        }
        let created_source = SystemOrderPoolBudget {
            epoch_index: 42,
            total_credit_budget: u64::MAX,
            remaining_credit_budget: 0,
            node_credit_caps: [("copied".into(), 7)].into(),
            node_credit_allocated: Default::default(),
        };
        let guard = overlay.transaction.guard();
        let _created = create_qualified_reward_pool_v1(
            RewardPoolCreateV1 {
                epoch_key: u64::MAX,
                source: &created_source,
                family: &family,
            },
            &mut overlay,
            &mut write,
            &mut RewardRecordsWorkV1 {
                cells_remaining: 100,
                max_pending_node_bytes: 2 << 20,
            },
            &guard,
            &live,
        )
        .unwrap();
        let guard = overlay.transaction.guard();
        delete_qualified_reward_pool_v1(
            RewardPoolDeleteV1 {
                epoch_key: 0,
                qualification: &deleted,
            },
            &mut overlay,
            &guard,
            &live,
        )
        .unwrap();
        let guard = overlay.transaction.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 6);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        let mut receiver_io = logical();
        let mut receiver_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver = ctx!(
            &sealed.record.after.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut receiver_io,
            &mut receiver_keys
        );
        let accepted = accept_reward_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        assert_eq!(accepted.record_counts(), [2, 5]);
        assert_eq!(accepted.mint_length(), 5);
        for node in &generic.new_nodes {
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
        let after = &sealed.record.after;
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver = ctx!(
            &after.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let raw = qualify_reward_family_v1(
            after,
            &mut receiver,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        assert_eq!(raw.record_counts(), [2, 5]);
        assert_eq!(raw.mint_length(), 5);
        for (index, expected) in [(0, 0), (1, 0), (2, 0), (3, u64::MAX), (4, 0)] {
            let key = RewardRecordKeyV1 {
                kind: RewardRecordKindV1::Mint,
                index,
            };
            receiver.root = after
                .state
                .iter()
                .find(|r| r.namespace == key.kind.namespace())
                .unwrap();
            let proof = open_qualified_reward_record_v1(key, after, &raw, &mut receiver)
                .unwrap()
                .unwrap();
            assert_eq!(
                read_qualified_reward_cell_v1(
                    RewardCellRequestV1 {
                        key,
                        cell: RewardCellV1::Column(RewardColumnV1::SourceAwardedPoints),
                        qualification: &proof
                    },
                    &mut receiver,
                    8,
                    &mut std::io::sink()
                )
                .unwrap(),
                Some(RewardValueV1::Unsigned(expected))
            );
        }
    }
}
