use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
fn state_error(e: impl std::fmt::Display) -> ValueTreeError {
    ValueTreeError::Io(std::io::Error::other(e.to_string()))
}
#[test]
fn original_point_columns_and_route_options_share_one_transaction_and_receiver() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("point-pages"),
            memory.clone(),
            compression,
        )
        .unwrap();
        let before = seed(&store, &memory, &source());
        let parent = ExecutionParent {
            roots: before.clone(),
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
        let values = store.value_session(physical_scope());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let top = before
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Business)
            .unwrap();
        let mut ctx = WorldFieldReadContextV1 {
            root: top,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family =
            qualify_material_family_v1(&before, &mut ctx, &mut scan(), &mut scan(), &mut 8192, 165)
                .unwrap();
        let mut proofs = Vec::new();
        for key in [
            MaterialRecordKeyV1::Profile(PROFILE),
            MaterialRecordKeyV1::Transfer(0),
            MaterialRecordKeyV1::Transfer(u64::MAX),
        ] {
            ctx.root = before
                .state
                .iter()
                .find(|r| r.namespace == key.kind().namespace())
                .unwrap();
            proofs.push(
                open_qualified_material_record_v1(key, &before, &family, &mut ctx)
                    .unwrap()
                    .unwrap(),
            );
        }
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
        let mut writer = store.value_session(physical_scope());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 4096,
                record_bytes: 4096,
            },
            cancelled: || false,
        };
        use MaterialCellV1 as C;
        use MaterialCellValueRefV1 as V;
        let site = oasis7_runtime::MaterialLedgerId::Site(" raw\0界 ".into());
        let agent = oasis7_runtime::MaterialLedgerId::Agent(String::new());
        for (index, key, cell, expected, after) in [
            (
                0,
                MaterialRecordKeyV1::Profile(PROFILE),
                C::Tier,
                Some(V::U64(255)),
                Some(V::U64(0)),
            ),
            (
                0,
                MaterialRecordKeyV1::Profile(PROFILE),
                C::Category,
                Some(V::Text("")),
                Some(V::Text(" new\0界 ")),
            ),
            (
                1,
                MaterialRecordKeyV1::Transfer(0),
                C::RouteId,
                None,
                Some(V::Text("")),
            ),
            (
                1,
                MaterialRecordKeyV1::Transfer(0),
                C::RouteId,
                Some(V::Text("")),
                Some(V::Text(" route ")),
            ),
            (
                1,
                MaterialRecordKeyV1::Transfer(0),
                C::RouteId,
                Some(V::Text(" route ")),
                None,
            ),
            (
                1,
                MaterialRecordKeyV1::Transfer(0),
                C::FromLedger,
                Some(V::Ledger(&agent)),
                Some(V::Ledger(&site)),
            ),
            (
                2,
                MaterialRecordKeyV1::Transfer(u64::MAX),
                C::Amount,
                Some(V::I64(i64::MIN)),
                Some(V::I64(i64::MAX)),
            ),
        ] {
            let guard = overlay.transaction.guard();
            let proof = change_qualified_material_cell_v1(
                MaterialCellChangeV1 {
                    key,
                    cell,
                    qualification: &proofs[index],
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
            proofs[index] = proof;
        }
        let original = source();
        let guard = overlay.transaction.guard();
        proofs.push(
            create_qualified_material_record_v1(
                MaterialRecordCreateV1 {
                    key: MaterialRecordKeyV1::Profile("created"),
                    source: MaterialRecordRefV1::Profile(&original.material_profiles[PROFILE]),
                    family: &family,
                },
                &mut overlay,
                &mut write,
                &mut MaterialWorkBudgetV1 {
                    cells_remaining: 128,
                    max_pending_node_bytes: 4 << 20,
                    scan: scan(),
                },
                &guard,
                &live,
            )
            .unwrap(),
        );
        let guard = overlay.transaction.guard();
        delete_qualified_material_record_v1(
            MaterialRecordDeleteV1 {
                key: MaterialRecordKeyV1::Transfer(u64::MAX),
                qualification: &proofs[2],
            },
            &mut overlay,
            &guard,
            &live,
        )
        .unwrap();
        let guard = tx.guard();
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
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let values2 = store.value_session(physical_scope());
        let mut receiver_io = logical();
        let mut receiver_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver = WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &state2,
            values: ValueReadContext {
                source: &values2,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut receiver_io,
            },
            key_work: &mut receiver_keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state2.take_read_error().map(state_error),
            cancelled: || false,
        };
        let after = accept_material_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        assert_eq!(after.record_counts(), [2, 1]);
        after
            .validate_binding(&sealed.record.after.digest().unwrap())
            .unwrap();
        for node in &generic.new_nodes {
            let digest = *blake3::hash(&node.bytes).as_bytes();
            assert_eq!(node.digest, blake3::hash(&node.bytes).to_hex().to_string());
            write
                .sink
                .put_node(
                    digest,
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
        let raw = qualify_material_family_v1(
            &sealed.record.after,
            &mut final_ctx,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        assert_eq!(raw.record_counts(), [2, 1]);
        for (index, key, cell, expected, bytes) in [
            (
                0,
                MaterialRecordKeyV1::Profile(PROFILE),
                C::Tier,
                Some(MaterialValueV1::Unsigned(0)),
                "",
            ),
            (
                0,
                MaterialRecordKeyV1::Profile(PROFILE),
                C::Category,
                Some(MaterialValueV1::Text {
                    utf8_bytes: " new\0界 ".len() as u64,
                }),
                " new\0界 ",
            ),
            (1, MaterialRecordKeyV1::Transfer(0), C::RouteId, None, ""),
            (
                1,
                MaterialRecordKeyV1::Transfer(0),
                C::FromLedger,
                Some(MaterialValueV1::Ledger(
                    oasis7_runtime::incremental_ledger_value::LedgerValueInfoV1 {
                        kind: oasis7_runtime::incremental_ledger_value::LedgerValueKindV1::Site,
                        id_bytes: " raw\0界 ".len() as u64,
                    },
                )),
                " raw\0界 ",
            ),
            (
                3,
                MaterialRecordKeyV1::Profile("created"),
                C::Tier,
                Some(MaterialValueV1::Unsigned(255)),
                "",
            ),
        ] {
            final_ctx.root = sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == key.kind().namespace())
                .unwrap();
            let mut output = Vec::new();
            let got = read_qualified_material_cell_v1(
                MaterialCellRequestV1 {
                    key,
                    cell,
                    qualification: &proofs[index],
                },
                &mut final_ctx,
                165,
                &mut output,
            )
            .unwrap();
            assert_eq!(got, expected);
            assert_eq!(output, bytes.as_bytes());
        }
        final_ctx.root = sealed
            .record
            .after
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Authorization)
            .unwrap();
        assert!(
            inspect_material_record_root_v1(
                MaterialRecordKeyV1::Transfer(u64::MAX),
                &mut final_ctx
            )
            .unwrap()
            .is_none()
        );
    }
}
