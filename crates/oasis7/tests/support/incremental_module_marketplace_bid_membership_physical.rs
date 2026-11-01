use super::*;

fn read_bids_record_ref(
    store: &IncrementalPageStore,
    roots: &[StateRootV1; 6],
    memory: &MemoryBudget,
    outer_key: &str,
) -> RecordRootRefV1 {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let field = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    let outer_semantic = literal_text_semantic(outer_key);
    let outer_field_key = WorldFieldKeyV1::new(
        field,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&outer_semantic).as_bytes()),
    )
    .unwrap();
    let outer_value = roots[0]
        .get(outer_field_key.as_bytes(), &state)
        .unwrap()
        .expect("marketplace outer record root");
    let values = store.value_session(physical());
    let mut value_io = logical();
    let mut value_context = ValueReadContext {
        source: &values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut value_io,
    };
    let mut returned_key = Vec::with_capacity(256);
    let mut root_bytes = [0u8; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &outer_value,
        &outer_field_key,
        &mut value_context,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: outer_semantic.len() as u64,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: 8192,
            },
        },
        &mut returned_key,
        &mut &mut root_bytes[..],
        || false,
    )
    .unwrap();
    assert_eq!(returned_key, outer_semantic);
    let reference = RecordRootRefV1::from_canonical_bytes(&root_bytes).unwrap();
    reference
        .validate_binding(
            blake3::hash(b"oasis7.iws.market.bids-record.v2\0").as_bytes(),
            blake3::hash(&outer_semantic).as_bytes(),
            StateNamespaceV1::Business,
        )
        .unwrap();
    reference
}

fn read_bids_child(
    store: &IncrementalPageStore,
    roots: &[StateRootV1; 6],
    memory: &MemoryBudget,
    outer_key: &str,
) -> StateRootV1 {
    read_bids_record_ref(store, roots, memory, outer_key)
        .validate_binding(
            blake3::hash(b"oasis7.iws.market.bids-record.v2\0").as_bytes(),
            blake3::hash(&literal_text_semantic(outer_key)).as_bytes(),
            StateNamespaceV1::Business,
        )
        .unwrap()
        .clone()
}

fn read_bids_payload(
    store: &IncrementalPageStore,
    roots: &[StateRootV1; 6],
    memory: &MemoryBudget,
    outer_key: &str,
    semantic: &[u8],
    codec: MemberPayloadCodecV1,
) -> Vec<u8> {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let child = read_bids_child(store, roots, memory, outer_key);
    let field = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    let member_key = WorldFieldKeyV1::new(
        field,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(semantic).as_bytes()),
    )
    .unwrap();
    let value = child
        .get(member_key.as_bytes(), &state)
        .unwrap()
        .expect("bid member");
    let values = store.value_session(physical());
    let mut value_io = logical();
    let mut context = ValueReadContext {
        source: &values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut value_io,
    };
    let mut returned_semantic = Vec::with_capacity(256);
    let mut payload = Vec::with_capacity(128);
    let info = read_member_record_profile_v1(
        &value,
        &member_key,
        &mut context,
        MemberReadProfileV1 {
            codec,
            semantic_bytes: semantic.len() as u64,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1024,
                record_bytes: 8192,
            },
        },
        &mut returned_semantic,
        &mut payload,
        || false,
    )
    .unwrap();
    assert_eq!(returned_semantic, semantic);
    assert_eq!(info.payload_bytes, payload.len() as u64);
    payload
}

fn read_bid_member_payload(
    store: &IncrementalPageStore,
    roots: &[StateRootV1; 6],
    memory: &MemoryBudget,
    outer_key: &str,
    ordinal: u64,
    name: &str,
    codec: MemberPayloadCodecV1,
) -> Vec<u8> {
    let semantic = literal_bid_member_semantic(ordinal, name);
    read_bids_payload(store, roots, memory, outer_key, &semantic, codec)
}

fn read_bid_identity(
    store: &IncrementalPageStore,
    roots: &[StateRootV1; 6],
    memory: &MemoryBudget,
    outer_key: &str,
    ordinal: u64,
) -> (u64, String) {
    let order_id = u64::from_be_bytes(
        read_bid_member_payload(
            store,
            roots,
            memory,
            outer_key,
            ordinal,
            "order_id",
            MemberPayloadCodecV1::U64,
        )
        .try_into()
        .unwrap(),
    );
    let bidder_agent_id = String::from_utf8(read_bid_member_payload(
        store,
        roots,
        memory,
        outer_key,
        ordinal,
        "bidder_agent_id",
        MemberPayloadCodecV1::Text,
    ))
    .unwrap();
    (order_id, bidder_agent_id)
}

fn read_bid_length(
    store: &IncrementalPageStore,
    roots: &[StateRootV1; 6],
    memory: &MemoryBudget,
    outer_key: &str,
) -> u64 {
    u64::from_be_bytes(
        read_bids_payload(
            store,
            roots,
            memory,
            outer_key,
            &literal_text_semantic("length"),
            MemberPayloadCodecV1::U64,
        )
        .try_into()
        .unwrap(),
    )
}

fn literal_bid_group_key(order_id: u64, bidder_agent_id: &str) -> String {
    let mut pair = b"oasis7.iws.market.bid-membership.v1\0".to_vec();
    pair.extend_from_slice(&order_id.to_be_bytes());
    pair.extend_from_slice(bidder_agent_id.as_bytes());
    format!("bid_group/{}", blake3::hash(&pair).to_hex())
}

fn bid_pair_group_exists(
    store: &IncrementalPageStore,
    roots: &[StateRootV1; 6],
    memory: &MemoryBudget,
    outer_key: &str,
    order_id: u64,
    bidder_agent_id: &str,
) -> bool {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let child = read_bids_child(store, roots, memory, outer_key);
    let field = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    let group_key = literal_bid_group_key(order_id, bidder_agent_id);
    let semantic = literal_text_semantic(&group_key);
    let key = WorldFieldKeyV1::new(
        field,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic).as_bytes()),
    )
    .unwrap();
    child.get(key.as_bytes(), &state).unwrap().is_some()
}

fn persist_bid_change(
    path: &std::path::Path,
    compression: PageCompression,
    roots: &RootSetV1,
    operation_id: u8,
    change: ModuleArtifactBidChangeV1<'_>,
) -> Vec<u8> {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store =
        IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Ordinary)
            .unwrap();
    let parent = ExecutionParent {
        commit: [50 + operation_id; 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [43; 32],
            epoch: 2,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut transaction = ExecutionTransaction::begin(
        parent.clone(),
        [60 + operation_id; 32],
        &state,
        &memory,
        limits(),
    )
    .unwrap();
    let guard = transaction.guard();
    let expected = read_bids_record_ref(&store, &roots.state, &memory, "bids/组件");
    let mut remaining_records = 128;
    let mut value_writer = store.value_session(physical());
    let mut read = WorldFieldOverlayContextV1 {
        transaction: &mut transaction,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut logical(),
        },
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 64 << 10,
        },
        semantic_bytes: 4096,
        record_bytes: MAX_TEST_RECORD_BYTES,
        take_state_error: || {
            state.take_read_error().map(|error| match error {
                PageStoreError::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
                PageStoreError::Cancelled => ValueTreeError::Cancelled,
                _ => ValueTreeError::Invalid("marketplace physical state read"),
            })
        },
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut value_writer,
        memory: &memory,
        io: &mut logical(),
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 64 << 10,
        },
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: MAX_TEST_RECORD_BYTES,
        },
        cancelled: || false,
    };
    let next = change_module_bid_overlay_v1(
        "bids/组件",
        &expected,
        change,
        &mut remaining_records,
        &mut read,
        &mut write,
        &guard,
        &live,
    )
    .unwrap();
    assert_ne!(next, expected);
    let guard = transaction.guard();
    let prepared = transaction.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.writes.len(), 1);
    let verifier_state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let verified = TransitionVerifier {
        parent: &parent,
        operation: [60 + operation_id; 32],
        source: &verifier_state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 16 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
    let mut destination = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    destination.persist_transition_pages(&verified).unwrap();
    verified.record.after.canonical_bytes().unwrap()
}

#[test]
fn marketplace_bid_pair_membership_survives_identity_migration_and_delete_cold_reopen() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let genesis_bytes = prepare_state_roots(&path, compression);
        let genesis = RootSetV1::from_canonical_bytes(&genesis_bytes).unwrap();

        // The initial pair memberships and original bid identities are read
        // from reopened pages using independent literal field/key encodings.
        {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            assert_eq!(
                read_bid_identity(&store, &genesis.state, &memory, "bids/组件", 0),
                (7, "buyer/一".to_owned())
            );
            assert_eq!(
                read_bid_identity(&store, &genesis.state, &memory, "bids/组件", 1),
                (8, "buyer/二".to_owned())
            );
            assert!(bid_pair_group_exists(
                &store,
                &genesis.state,
                &memory,
                "bids/组件",
                7,
                "buyer/一"
            ));
            assert!(bid_pair_group_exists(
                &store,
                &genesis.state,
                &memory,
                "bids/组件",
                8,
                "buyer/二"
            ));
        }

        // Move the first stable bid into the second bid's pair. Its original
        // price and Vec position remain intact, while the old pair group is
        // removed and the destination pair contains both stable IDs.
        let moved_bytes = persist_bid_change(
            &path,
            compression,
            &genesis,
            1,
            ModuleArtifactBidChangeV1::UpdateIdentity {
                ordinal: 0,
                expected_order_id: 7,
                expected_bidder_agent_id: "buyer/一",
                after_order_id: 8,
                after_bidder_agent_id: "buyer/二",
            },
        );
        let moved = RootSetV1::from_canonical_bytes(&moved_bytes).unwrap();
        {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            verify_world_state_closure(&store, &moved, &memory);
            assert_eq!(
                read_bid_identity(&store, &moved.state, &memory, "bids/组件", 0),
                (8, "buyer/二".to_owned())
            );
            assert_eq!(
                read_bid_identity(&store, &moved.state, &memory, "bids/组件", 1),
                (8, "buyer/二".to_owned())
            );
            assert_eq!(
                read_bid_price(&store, &moved.state, &memory, "bids/组件", 0),
                -5
            );
            assert!(!bid_pair_group_exists(
                &store,
                &moved.state,
                &memory,
                "bids/组件",
                7,
                "buyer/一"
            ));
            assert!(bid_pair_group_exists(
                &store,
                &moved.state,
                &memory,
                "bids/组件",
                8,
                "buyer/二"
            ));
        }

        // Delete every member of the now-duplicate pair in one typed change.
        // The final RootSet is reopened and the full owner closure is checked
        // before direct reads confirm the empty sequence and removed group.
        let deleted_bytes = persist_bid_change(
            &path,
            compression,
            &moved,
            2,
            ModuleArtifactBidChangeV1::DeleteMatching {
                order_id: 8,
                bidder_agent_id: "buyer/二",
                expected_matches: 2,
            },
        );
        let deleted = RootSetV1::from_canonical_bytes(&deleted_bytes).unwrap();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        verify_world_state_closure(&store, &deleted, &memory);
        assert_eq!(
            read_bid_length(&store, &deleted.state, &memory, "bids/组件"),
            0
        );
        assert!(!bid_pair_group_exists(
            &store,
            &deleted.state,
            &memory,
            "bids/组件",
            8,
            "buyer/二"
        ));
    }
}
