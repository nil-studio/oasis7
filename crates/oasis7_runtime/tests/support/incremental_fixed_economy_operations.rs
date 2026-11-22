use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
#[test]
fn all_fixed_economy_columns_update_and_query_the_same_transaction() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("ops"), memory.clone(), compression)
                .unwrap();
        let roots = seed(&store, &memory);
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
        let mut ctx = read_ctx!(
            &roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family =
            qualify_fixed_economy_family_v1(&roots, &mut ctx, &mut scan(), &mut scan(), &mut 1000)
                .unwrap();
        let mut proofs: Vec<ReservedValue<FixedEconomyRecordQualificationV1>> =
            Vec::with_capacity(4);
        let _fixture = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        for field in FixedEconomyFieldV1::ALL {
            ctx.root = roots
                .state
                .iter()
                .find(|root| root.namespace == field.namespace())
                .unwrap();
            proofs.push(
                open_qualified_fixed_economy_record_v1(field, &roots, &family, &mut ctx).unwrap(),
            );
        }
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 16,
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
            record_bytes: 4096,
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
        let mut wanted = Vec::with_capacity(15);
        for (field, column, _, before) in expected() {
            let index = FixedEconomyFieldV1::ALL
                .iter()
                .position(|v| *v == field)
                .unwrap();
            let after = match before {
                FixedEconomyValueV1::U64(v) => FixedEconomyValueV1::U64(v.wrapping_add(1)),
                FixedEconomyValueV1::I64(v) => {
                    FixedEconomyValueV1::I64(if v == i64::MIN { i64::MAX } else { i64::MIN })
                }
                FixedEconomyValueV1::Bool(v) => FixedEconomyValueV1::Bool(!v),
            };
            let guard = overlay.transaction.guard();
            proofs[index] = change_qualified_fixed_economy_cell_v1(
                FixedEconomyCellChangeV1 {
                    column,
                    qualification: &proofs[index],
                    expected: before,
                    after,
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
            assert_eq!(
                read_qualified_fixed_economy_overlay_v1(
                    FixedEconomyCellRequestV1 {
                        column,
                        qualification: &proofs[index]
                    },
                    &mut overlay,
                    &guard,
                    &live
                )
                .unwrap(),
                after
            );
            let guard = overlay.transaction.guard();
            assert!(
                change_qualified_fixed_economy_cell_v1(
                    FixedEconomyCellChangeV1 {
                        column,
                        qualification: &proofs[index],
                        expected: after,
                        after,
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
            wanted.push((field, column, index, after));
        }
        let guard = overlay.transaction.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 4);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        let state2 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values2 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver = read_ctx!(
            &sealed.record.after.state[0],
            &state2,
            &values2,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let accepted = accept_fixed_economy_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut 8192,
        )
        .unwrap();
        accepted
            .validate_binding(&sealed.record.after.digest().unwrap())
            .unwrap();
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
        let state3 = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values3 = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut final_ctx = read_ctx!(
            &sealed.record.after.state[0],
            &state3,
            &values3,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        qualify_fixed_economy_family_v1(
            &sealed.record.after,
            &mut final_ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
        )
        .unwrap();
        for (field, column, index, wanted) in wanted {
            final_ctx.root = sealed
                .record
                .after
                .state
                .iter()
                .find(|root| root.namespace == field.namespace())
                .unwrap();
            assert_eq!(
                read_qualified_fixed_economy_cell_v1(
                    FixedEconomyCellRequestV1 {
                        column,
                        qualification: &proofs[index]
                    },
                    &mut final_ctx
                )
                .unwrap(),
                wanted
            );
        }
    }
}
