use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
#[test]
fn family_bound_lifecycle_and_original_columns_share_one_transaction() {
    use FactoryAuthorityCellV1 as C;
    use FactoryAuthorityCellValueRefV1 as V;
    use FactoryAuthorityRecordKindV1 as K;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _caller = memory.reserve(256 << 10, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("operations"),
            memory.clone(),
            compression,
        )
        .unwrap();
        let original = source();
        let top = seed(&original, &store, &memory);
        let mut roots =
            RootSetV1::empty("factory-authority-operations".into(), [0x78; 32]).unwrap();
        *roots
            .state
            .iter_mut()
            .find(|r| r.namespace == StateNamespaceV1::Business)
            .unwrap() = top;
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical_scope());
        let mut read_io = logical();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family = qualify_factory_authority_family_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 4096,
            1024,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [1, 3]);
        let keys = [
            FactoryAuthorityRecordKeyV1 {
                kind: K::Site,
                id: " outer-site\0界 ",
            },
            FactoryAuthorityRecordKeyV1 {
                kind: K::Power,
                id: "none",
            },
            FactoryAuthorityRecordKeyV1 {
                kind: K::Power,
                id: "empty",
            },
            FactoryAuthorityRecordKeyV1 {
                kind: K::Power,
                id: "text",
            },
        ];
        let mut proofs: Vec<_> = keys
            .iter()
            .map(|&key| {
                open_qualified_factory_authority_record_v1(key, &roots, &family, &mut ctx)
                    .unwrap()
                    .unwrap()
            })
            .collect();
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
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 32,
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
        let mut backend = store.value_session(physical_scope());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut backend,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1024,
                record_bytes: 4096,
            },
            cancelled: || false,
        };
        for (i, cell, expected, after) in [
            (0, C::Active, Some(V::Bool(true)), Some(V::Bool(false))),
            (
                0,
                C::AuthorizedAgent(1),
                Some(V::Text("")),
                Some(V::Text(" new-member\0界 ")),
            ),
            (1, C::SourceModuleId, None, Some(V::Text(""))),
            (
                1,
                C::SourceModuleId,
                Some(V::Text("")),
                Some(V::Text(" raw\0界 ")),
            ),
            (1, C::SourceModuleId, Some(V::Text(" raw\0界 ")), None),
            (1, C::SourceModuleId, None, Some(V::Text(" final-module "))),
            (
                3,
                C::ElectricityAmount,
                Some(V::I64(i64::MIN)),
                Some(V::I64(i64::MAX)),
            ),
        ] {
            let guard = overlay.transaction.guard();
            proofs[i] = change_qualified_factory_authority_cell_v1(
                FactoryAuthorityCellChangeV1 {
                    key: keys[i],
                    cell,
                    qualification: &proofs[i],
                    expected,
                    after,
                    max_current_payload_bytes: 1024,
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
        let appended = vec![String::new(), "repeat".into(), String::new()];
        let last = vec!["z".into()];
        let mut list_work = FactoryAuthorityWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 4 << 20,
        };
        for (expected_len, edit) in [
            (3, FactoryAuthorityAllowlistEditV1::Append(&appended)),
            (6, FactoryAuthorityAllowlistEditV1::Truncate(4)),
            (4, FactoryAuthorityAllowlistEditV1::Append(&last)),
        ] {
            let guard = overlay.transaction.guard();
            proofs[0] = change_qualified_factory_allowlist_v1(
                FactoryAuthorityAllowlistChangeV1 {
                    key: keys[0],
                    qualification: &proofs[0],
                    expected_len,
                    edit,
                },
                &mut overlay,
                &mut write,
                &mut list_work,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
        }
        let guard = overlay.transaction.guard();
        assert!(
            change_qualified_factory_allowlist_v1(
                FactoryAuthorityAllowlistChangeV1 {
                    key: keys[0],
                    qualification: &proofs[0],
                    expected_len: 5,
                    edit: FactoryAuthorityAllowlistEditV1::Truncate(5)
                },
                &mut overlay,
                &mut write,
                &mut list_work,
                &guard,
                &live
            )
            .unwrap()
            .is_none()
        );
        let new_key = FactoryAuthorityRecordKeyV1 {
            kind: K::Site,
            id: "created-outer",
        };
        let guard = overlay.transaction.guard();
        let created = create_qualified_factory_authority_record_v1(
            FactoryAuthorityRecordCreateV1 {
                key: new_key,
                source: FactoryAuthorityRecordRefV1::Site(
                    &original.factory_site_authorities[" outer-site\0界 "],
                ),
                family: &family,
            },
            &mut overlay,
            &mut write,
            &mut FactoryAuthorityWorkV1 {
                cells_remaining: 1000,
                max_pending_node_bytes: 4 << 20,
            },
            &guard,
            &live,
        )
        .unwrap();
        assert_ne!(created.root().node_root, proofs[0].root().node_root);
        let guard = overlay.transaction.guard();
        delete_qualified_factory_authority_record_v1(
            FactoryAuthorityRecordDeleteV1 {
                key: keys[2],
                qualification: &proofs[2],
            },
            &mut overlay,
            &guard,
            &live,
        )
        .unwrap();
        let guard = overlay.transaction.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 5);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let generic = verifier.verify(&sealed.record).unwrap();
        let receiver_state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let receiver_values = store.value_session(physical_scope());
        let mut receiver_io = logical();
        let mut receiver_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver_ctx = WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &receiver_state,
            values: ValueReadContext {
                source: &receiver_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut receiver_io,
            },
            key_work: &mut receiver_keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || receiver_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let accepted = accept_factory_authority_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver_ctx,
            &mut scan(),
            &mut 4096,
            1024,
        )
        .unwrap();
        assert_eq!(accepted.record_counts(), [2, 2]);
        accepted
            .validate_binding(&sealed.record.after.digest().unwrap())
            .unwrap();
        for node in &generic.new_nodes {
            write
                .sink
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        let final_state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let final_values = store.value_session(physical_scope());
        let mut final_io = logical();
        let mut final_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut final_ctx = WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &final_state,
            values: ValueReadContext {
                source: &final_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut final_io,
            },
            key_work: &mut final_keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || final_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let after = qualify_factory_authority_family_v1(
            &sealed.record.after,
            &mut final_ctx,
            &mut scan(),
            &mut scan(),
            &mut 4096,
            1024,
        )
        .unwrap();
        assert_eq!(after.record_counts(), [2, 2]);
        assert!(
            open_qualified_factory_authority_record_v1(
                keys[2],
                &sealed.record.after,
                &after,
                &mut final_ctx
            )
            .unwrap()
            .is_none()
        );
        for (key, cell, value, text) in [
            (
                keys[0],
                C::AuthorizedCount,
                Some(FactoryAuthorityValueV1::Unsigned(5)),
                "",
            ),
            (
                keys[0],
                C::AuthorizedAgent(3),
                Some(FactoryAuthorityValueV1::Text { utf8_bytes: 0 }),
                "",
            ),
            (
                keys[0],
                C::AuthorizedAgent(4),
                Some(FactoryAuthorityValueV1::Text { utf8_bytes: 1 }),
                "z",
            ),
            (
                keys[0],
                C::Active,
                Some(FactoryAuthorityValueV1::Bool(false)),
                "",
            ),
            (
                keys[0],
                C::AuthorizedAgent(1),
                Some(FactoryAuthorityValueV1::Text {
                    utf8_bytes: " new-member\0界 ".len() as u64,
                }),
                " new-member\0界 ",
            ),
            (
                keys[1],
                C::SourceModuleId,
                Some(FactoryAuthorityValueV1::Text {
                    utf8_bytes: " final-module ".len() as u64,
                }),
                " final-module ",
            ),
            (
                keys[3],
                C::ElectricityAmount,
                Some(FactoryAuthorityValueV1::Signed(i64::MAX)),
                "",
            ),
            (
                new_key,
                C::SiteId,
                Some(FactoryAuthorityValueV1::Text {
                    utf8_bytes: " inner-site\0界 ".len() as u64,
                }),
                " inner-site\0界 ",
            ),
        ] {
            let token = open_qualified_factory_authority_record_v1(
                key,
                &sealed.record.after,
                &after,
                &mut final_ctx,
            )
            .unwrap()
            .unwrap();
            let mut output = Vec::new();
            assert_eq!(
                read_qualified_factory_authority_cell_v1(
                    FactoryAuthorityCellRequestV1 {
                        key,
                        cell,
                        qualification: &token
                    },
                    &mut final_ctx,
                    1024,
                    &mut output
                )
                .unwrap(),
                value
            );
            assert_eq!(output, text.as_bytes());
        }
    }
}
