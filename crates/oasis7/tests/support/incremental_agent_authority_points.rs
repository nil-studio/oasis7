use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
fn state_error(e: oasis7_distfs::incremental_page_store::PageStoreError) -> ValueTreeError {
    use oasis7_distfs::incremental_page_store::PageStoreError as E;
    match e {
        E::Memory(error) => ValueTreeError::Memory(error),
        E::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        E::Cancelled => ValueTreeError::Cancelled,
        E::Io(error) => ValueTreeError::Io(error),
        E::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "agent authority physical page missing",
        )),
        E::Corrupt(reason) => ValueTreeError::Invalid(reason),
        E::UnsupportedPlatform => ValueTreeError::Invalid("agent authority physical platform"),
    }
}
#[test]
fn point_option_creation_deletion_and_receiver_preserve_original_agent_records() {
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
        let family = qualify_agent_authority_family_v1(
            &before,
            &mut ctx,
            &mut work().scan,
            &mut work().scan,
            &mut 8192,
            165,
        )
        .unwrap();
        let mut proofs = Vec::new();
        for key in [
            AgentAuthorityRecordKeyV1 {
                kind: AgentAuthorityRecordKindV1::IntentLedger,
                id: " outer intent ",
            },
            AgentAuthorityRecordKeyV1 {
                kind: AgentAuthorityRecordKindV1::AgentLocation,
                id: " outer agent ",
            },
            AgentAuthorityRecordKeyV1 {
                kind: AgentAuthorityRecordKindV1::LocationAnchor,
                id: "",
            },
        ] {
            ctx.root = before
                .state
                .iter()
                .find(|r| r.namespace == key.kind.namespace())
                .unwrap();
            proofs.push(
                open_qualified_agent_authority_record_v1(key, &before, &family, &mut ctx)
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
        use AgentAuthorityColumnV1 as C;
        use AgentAuthorityValueRefV1 as V;
        use IntentFieldV1 as I;
        let ik = AgentAuthorityRecordKeyV1 {
            kind: AgentAuthorityRecordKindV1::IntentLedger,
            id: " outer intent ",
        };
        let ak = AgentAuthorityRecordKeyV1 {
            kind: AgentAuthorityRecordKindV1::AgentLocation,
            id: " outer agent ",
        };
        for (index, key, column, expected, after) in [
            (0, ik, C::Intent(I::EventSeq), V::U64(17), V::U64(18)),
            (0, ik, C::Intent(I::TargetId), V::Unit, V::Text("")),
            (
                0,
                ik,
                C::Intent(I::TargetId),
                V::Text(""),
                V::Text(" raw target "),
            ),
            (
                0,
                ik,
                C::Intent(I::TargetId),
                V::Text(" raw target "),
                V::Unit,
            ),
            (0, ik, C::Intent(I::IntentTick), V::U64(0), V::Unit),
            (0, ik, C::Intent(I::IntentTick), V::Unit, V::U64(u64::MAX)),
            (1, ak, C::Active, V::Bool(false), V::Bool(true)),
        ] {
            let guard = overlay.transaction.guard();
            proofs[index] = change_qualified_agent_authority_cell_v1(
                AgentAuthorityCellChangeV1 {
                    key,
                    cell: AgentAuthorityCellV1::Column(column),
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
        }
        let original = source();
        let guard = overlay.transaction.guard();
        let created = create_qualified_agent_authority_record_v1(
            AgentAuthorityRecordCreateV1 {
                key: AgentAuthorityRecordKeyV1 {
                    kind: AgentAuthorityRecordKindV1::LocationAnchor,
                    id: " created outer ",
                },
                source: AgentAuthorityRecordRefV1::LocationAnchor(&original.location_anchors[""]),
                family: &family,
            },
            &mut overlay,
            &mut write,
            &mut work(),
            &guard,
            &live,
        )
        .unwrap();
        let _created = created;
        let guard = overlay.transaction.guard();
        delete_qualified_agent_authority_record_v1(
            AgentAuthorityRecordDeleteV1 {
                key: AgentAuthorityRecordKeyV1 {
                    kind: AgentAuthorityRecordKindV1::LocationAnchor,
                    id: "",
                },
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
        let after = accept_agent_authority_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut receiver,
            &mut work().scan,
            &mut 8192,
            165,
        )
        .unwrap();
        assert_eq!(after.record_counts(), [1, 1, 1]);
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
        let raw = qualify_agent_authority_family_v1(
            &sealed.record.after,
            &mut final_ctx,
            &mut work().scan,
            &mut work().scan,
            &mut 8192,
            165,
        )
        .unwrap();
        assert_eq!(raw.record_counts(), [1, 1, 1]);
        for (key, column, expected) in [
            (
                ik,
                C::Intent(I::EventSeq),
                AgentAuthorityValueV1::Unsigned(18),
            ),
            (ik, C::Intent(I::TargetId), AgentAuthorityValueV1::Unit),
            (
                ik,
                C::Intent(I::IntentTick),
                AgentAuthorityValueV1::Unsigned(u64::MAX),
            ),
            (ak, C::Active, AgentAuthorityValueV1::Bool(true)),
            (
                AgentAuthorityRecordKeyV1 {
                    kind: AgentAuthorityRecordKindV1::LocationAnchor,
                    id: " created outer ",
                },
                C::Active,
                AgentAuthorityValueV1::Bool(false),
            ),
        ] {
            final_ctx.root = sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == key.namespace())
                .unwrap();
            let proof = open_qualified_agent_authority_record_v1(
                key,
                &sealed.record.after,
                &raw,
                &mut final_ctx,
            )
            .unwrap()
            .unwrap();
            let value = read_qualified_agent_authority_cell_v1(
                AgentAuthorityCellRequestV1 {
                    key,
                    cell: AgentAuthorityCellV1::Column(column),
                    qualification: &proof,
                },
                &mut final_ctx,
                165,
                &mut std::io::sink(),
            )
            .unwrap();
            assert_eq!(value, expected);
        }
        assert!(
            open_qualified_agent_authority_record_v1(
                AgentAuthorityRecordKeyV1 {
                    kind: AgentAuthorityRecordKindV1::LocationAnchor,
                    id: ""
                },
                &sealed.record.after,
                &raw,
                &mut final_ctx
            )
            .unwrap()
            .is_none()
        );
    }
}
