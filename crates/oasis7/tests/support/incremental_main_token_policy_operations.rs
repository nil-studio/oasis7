use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
fn open(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    key: MainTokenPolicyKeyV1<'_>,
    family: &ReservedValue<MainTokenPolicyFamilyQualificationV1>,
) -> ReservedValue<MainTokenPolicyRecordQualificationV1> {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = ctx!(
        roots
            .state
            .iter()
            .find(|r| r.namespace == key.kind().namespace())
            .unwrap(),
        &state,
        &values,
        memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    open_qualified_main_token_policy_record_v1(key, roots, family, &mut ctx)
        .unwrap()
        .unwrap()
}
#[test]
fn point_options_closed_status_and_lifecycle_replay_in_one_transaction() {
    use MainTokenConfigColumnV1 as Config;
    use MainTokenPolicyCellV1 as Cell;
    use MainTokenPolicyColumnV1 as C;
    use MainTokenPolicyKeyV1 as K;
    use MainTokenPolicyValueRefV1 as P;
    use MainTokenPolicyValueV1 as V;
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
        let family = qualify_main_token_policy_family_v1(
            &roots,
            &mut c,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
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
        let mut rio = logical();
        let mut rkeys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut rio,
            },
            key_work: &mut rkeys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut writer = store.value_session(physical());
        let mut wio = logical();
        let mut wkeys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut wio,
            key_work: &mut wkeys,
            limits: limits(),
            cancelled: || false,
        };
        let _case_admission = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let cases = [
            (
                K::Config,
                vec![
                    (Cell::Config(Config::MaxSupply), P::U64(0), P::Unit),
                    (Cell::Config(Config::MaxSupply), P::Unit, P::U64(u64::MAX)),
                    (Cell::Config(Config::Decimals), P::U64(255), P::U64(0)),
                    (Cell::Config(Config::Symbol), P::Text(" Ω "), P::Text("")),
                ],
            ),
            (
                K::Scheduled(9),
                vec![
                    (Cell::NextConfig(Config::MaxSupply), P::Unit, P::U64(0)),
                    (Cell::Column(C::ProposalId), P::U64(19), P::U64(0)),
                ],
            ),
            (
                K::Bucket(""),
                vec![(
                    Cell::Column(C::RatioBps),
                    P::U64(u64::from(u32::MAX)),
                    P::U64(0),
                )],
            ),
            (
                K::Issuance(7),
                vec![(Cell::Column(C::EpochIndex), P::U64(17), P::U64(u64::MAX))],
            ),
            (
                K::Grant(" raw grant "),
                vec![
                    (
                        Cell::Column(C::Status),
                        P::Text("revoked"),
                        P::Text("expired"),
                    ),
                    (Cell::Column(C::StatusReason), P::Text(""), P::Unit),
                    (Cell::Column(C::StatusReason), P::Unit, P::Text(" raw Ω ")),
                    (Cell::Column(C::StatusUpdatedAtEpoch), P::U64(0), P::Unit),
                    (
                        Cell::Column(C::StatusUpdatedAtEpoch),
                        P::Unit,
                        P::U64(u64::MAX),
                    ),
                ],
            ),
        ];
        for (key, changes) in cases {
            let mut proof = open(&store, &memory, &roots, key, &family);
            for (cell, expected, after) in changes {
                let guard = overlay.transaction.guard();
                proof = change_qualified_main_token_policy_cell_v1(
                    MainTokenPolicyCellChangeV1 {
                        key,
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
                let mut output = Vec::new();
                let got = read_qualified_main_token_policy_cell_overlay_v1(
                    MainTokenPolicyCellRequestV1 {
                        key,
                        cell,
                        qualification: &proof,
                    },
                    &mut overlay,
                    165,
                    &mut output,
                    &guard,
                    &live,
                )
                .unwrap();
                match after {
                    P::Unit => {
                        assert_eq!(got, V::Unit);
                        assert!(output.is_empty())
                    }
                    P::U64(n) => {
                        assert_eq!(got, V::Unsigned(n));
                        assert!(output.is_empty())
                    }
                    P::Text(text) => {
                        assert_eq!(
                            got,
                            V::Text {
                                utf8_bytes: text.len() as u64
                            }
                        );
                        assert_eq!(output, text.as_bytes());
                    }
                }
                let guard = overlay.transaction.guard();
                assert!(
                    change_qualified_main_token_policy_cell_v1(
                        MainTokenPolicyCellChangeV1 {
                            key,
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
        }
        let src = source();
        for (key, source) in [
            (
                K::Bucket(" new bucket "),
                MainTokenPolicyRecordRefV1::Bucket(&src.main_token_genesis_buckets[""]),
            ),
            (
                K::Issuance(99),
                MainTokenPolicyRecordRefV1::Issuance(&src.main_token_epoch_issuance_records[&7]),
            ),
            (
                K::Grant(" new grant "),
                MainTokenPolicyRecordRefV1::Grant(&src.restricted_starter_claim_grants["none"]),
            ),
            (
                K::Scheduled(99),
                MainTokenPolicyRecordRefV1::Scheduled(&src.main_token_scheduled_policy_updates[&9]),
            ),
        ] {
            let guard = overlay.transaction.guard();
            let proof = create_qualified_main_token_policy_record_v1(
                MainTokenPolicyRecordCreateV1 {
                    key,
                    source,
                    family: &family,
                },
                &mut overlay,
                &mut write,
                &mut MainTokenPolicyWorkV1 {
                    cells_remaining: 64,
                    max_pending_node_bytes: 2 << 20,
                },
                &guard,
                &live,
            )
            .unwrap();
            let guard = overlay.transaction.guard();
            assert_eq!(
                read_qualified_main_token_policy_cell_overlay_v1(
                    MainTokenPolicyCellRequestV1 {
                        key,
                        cell: Cell::Record,
                        qualification: &proof
                    },
                    &mut overlay,
                    0,
                    &mut std::io::sink(),
                    &guard,
                    &live
                )
                .unwrap(),
                V::Unit
            );
        }
        let key = K::Grant("none");
        let proof = open(&store, &memory, &roots, key, &family);
        let guard = overlay.transaction.guard();
        delete_qualified_main_token_policy_record_v1(
            MainTokenPolicyRecordDeleteV1 {
                key,
                qualification: &proof,
            },
            &mut overlay,
            &guard,
            &live,
        )
        .unwrap();
        let guard = overlay.transaction.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 10);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        eprintln!(
            "sender plus explicit generic oracle {compression:?}: state={:?} values={:?}",
            state.remaining_io().unwrap(),
            values.remaining_io().unwrap()
        );
        // Receiving is a separate public operation. Its generic replay and owner
        // pass share these fresh finite sessions continuously, with the same pool.
        let receiver_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let receiver_values = store.value_session(physical());
        let receiver_verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &receiver_state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let mut receiver_io = logical();
        let mut receiver_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = ctx!(
            &sealed.record.after.state[0],
            &receiver_state,
            &receiver_values,
            &memory,
            MemoryClass::Ordinary,
            &mut receiver_io,
            &mut receiver_keys
        );
        let accepted = accept_main_token_policy_transition_v1(
            &family,
            &receiver_verifier,
            &sealed.record,
            &mut ctx,
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap_or_else(|error| {
            panic!(
                "typed receiver {compression:?}: {error:?}; state={:?} values={:?}",
                receiver_state.remaining_io().unwrap(),
                receiver_values.remaining_io().unwrap()
            )
        });
        eprintln!(
            "complete generic plus typed receiver {compression:?}: state={:?} values={:?}",
            receiver_state.remaining_io().unwrap(),
            receiver_values.remaining_io().unwrap()
        );
        assert_eq!(accepted.record_counts(), [1, 2, 2, 2, 2]);
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
        let mut ctx = ctx!(
            &after.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let raw = qualify_main_token_policy_family_v1(
            after,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        assert_eq!(raw.record_counts(), [1, 2, 2, 2, 2]);
        for (key, cell, expected, text) in [
            (
                K::Config,
                Cell::Config(Config::MaxSupply),
                V::Unsigned(u64::MAX),
                "",
            ),
            (
                K::Scheduled(9),
                Cell::NextConfig(Config::MaxSupply),
                V::Unsigned(0),
                "",
            ),
            (
                K::Issuance(7),
                Cell::Column(C::EpochIndex),
                V::Unsigned(u64::MAX),
                "",
            ),
            (
                K::Grant(" raw grant "),
                Cell::Column(C::Status),
                V::Text { utf8_bytes: 7 },
                "expired",
            ),
            (
                K::Grant(" new grant "),
                Cell::Column(C::StatusReason),
                V::Unit,
                "",
            ),
            (
                K::Bucket(" new bucket "),
                Cell::Column(C::BucketId),
                V::Text { utf8_bytes: 14 },
                " other bucket ",
            ),
        ] {
            ctx.root = after
                .state
                .iter()
                .find(|r| r.namespace == key.kind().namespace())
                .unwrap();
            let proof = open_qualified_main_token_policy_record_v1(key, after, &raw, &mut ctx)
                .unwrap()
                .unwrap();
            let mut output = Vec::new();
            assert_eq!(
                read_qualified_main_token_policy_cell_v1(
                    MainTokenPolicyCellRequestV1 {
                        key,
                        cell,
                        qualification: &proof
                    },
                    &mut ctx,
                    165,
                    &mut output
                )
                .unwrap(),
                expected
            );
            assert_eq!(output, text.as_bytes());
        }
    }
}
