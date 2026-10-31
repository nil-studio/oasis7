use super::*;
use std::collections::BTreeSet;

fn bid_overlay_parent(
    memory: &MemoryBudget,
    bid_count: u64,
) -> (SharedValues, ExecutionParent, StatePages, RecordRootRefV1) {
    let world = bid_overlay_world_with_count(bid_count);
    let mut values = SharedValues::new(memory.clone());
    let mut io = ValueIoBudget {
        nodes_remaining: 100_000u64.max(bid_count.saturating_mul(2_000)),
        read_bytes_remaining: 1 << 30,
        // Fixture construction appends a large tree incrementally. This
        // setup allowance is separate from and never included in mutation
        // cost measurements below.
        write_bytes_remaining: (64 << 20).max(bid_count.saturating_mul(2 << 20)),
    };
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut remaining = 20_000;
    let mut genesis = Vec::<ReservedValue<ExecutionWriteV1>>::new();
    let result = prepare_module_marketplace_genesis_v1(
        &world,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 4 << 20,
                record_bytes: 8 << 20,
            },
            cancelled: || false,
        },
        &mut remaining,
        &mut |write| {
            genesis.push(write);
            Ok(())
        },
    );
    if let Err(error) = result {
        panic!(
            "bid genesis failed for {bid_count} bids: {error:?}; writes_remaining={} nodes_remaining={} key_work_remaining={} records_remaining={remaining}",
            io.write_bytes_remaining, io.nodes_remaining, key_work.bytes_remaining
        );
    }

    let mut parent = ExecutionParent {
        commit: [31; 32],
        roots: RootSetV1::empty("marketplace-bid-overlay".into(), [32; 32]).unwrap(),
        authority: AuthorityBinding {
            token: [33; 32],
            epoch: 0,
        },
    };
    let mut state_pages = StatePages::default();
    let business = parent
        .roots
        .state
        .iter()
        .position(|root| root.namespace == StateNamespaceV1::Business)
        .unwrap();
    let mut state_writes = genesis
        .iter()
        .filter(|write| write.namespace == StateNamespaceV1::Business)
        .map(|write| write.mutation.clone())
        .collect::<Vec<_>>();
    state_writes.sort_unstable_by(|left, right| left.key.cmp(&right.key));
    let delta = StateDeltaV1::build(
        parent.roots.state[business].clone(),
        state_writes,
        &state_pages,
        16 << 20,
    )
    .unwrap();
    for page in delta.new_nodes {
        state_pages.0.insert(page.digest, page.bytes);
    }
    parent.roots.state[business] = delta.after;

    let bids = ModuleMarketplaceFieldV1::ArtifactBids;
    let child = outer_child(bids, "bid-map-key", &genesis, &values, memory);
    let mut outer_semantic = Vec::new();
    write_semantic_key_v1(&"bid-map-key", &mut outer_semantic).unwrap();
    let expected = RecordRootRefV1::new(
        bids.schema(),
        *blake3::hash(&outer_semantic).as_bytes(),
        child,
    )
    .unwrap();
    (values, parent, state_pages, expected)
}

#[expect(
    clippy::too_many_arguments,
    reason = "The test helper measures a mutation with each transaction resource supplied independently."
)]
fn apply_bid_change(
    transaction: &mut ExecutionTransaction<'_, StatePages>,
    values: &mut SharedValues,
    memory: &MemoryBudget,
    expected: &RecordRootRefV1,
    change: ModuleArtifactBidChangeV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    read_io: &mut ValueIoBudget,
    write_io: &mut ValueIoBudget,
    read_key_work: &mut SemanticKeyBudget,
    write_key_work: &mut SemanticKeyBudget,
    remaining: &mut u64,
) -> Result<RecordRootRefV1, crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
    let reader = values.clone();
    let mut overlay = crate::incremental_field_overlay::WorldFieldOverlayContextV1 {
        transaction,
        values: ValueReadContext {
            source: &reader,
            memory,
            class: MemoryClass::Ordinary,
            io: read_io,
        },
        key_work: read_key_work,
        semantic_bytes: 1 << 20,
        record_bytes: 8 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: values,
        memory,
        io: write_io,
        key_work: write_key_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 4 << 20,
            record_bytes: 8 << 20,
        },
        cancelled: || false,
    };
    change_module_bid_overlay_v1(
        "bid-map-key",
        expected,
        change,
        remaining,
        &mut overlay,
        &mut write,
        guard,
        live,
    )
}

#[derive(Debug)]
struct BidMutatorMetrics {
    nodes: u64,
    bytes: u64,
    value_reads: usize,
    state_reads: usize,
}

fn measure_middle_bid_delete(bid_count: u64) -> BidMutatorMetrics {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let (mut values, parent, state_pages, expected) = bid_overlay_parent(&memory, bid_count);
    let live = parent.fence().unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent.clone(),
        [35; 32],
        &state_pages,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let mut read_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 64 << 20,
    };
    let mut write_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 64 << 20,
    };
    let mut read_key_work = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_key_work = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut remaining = 20_000;
    let ordinal = bid_count / 2;
    let (expected_order_id, expected_bidder_agent_id) = if matches!(ordinal, 0 | 4 | 8) {
        (77, "duplicate/β".to_owned())
    } else {
        (100 + ordinal, format!("bidder/{ordinal}"))
    };
    let guard = transaction.guard();
    let nodes_before = read_io.nodes_remaining;
    let bytes_before = read_io.read_bytes_remaining;
    let reads_before = values.read_counts();
    let next = apply_bid_change(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::Delete {
            ordinal,
            expected_order_id,
            expected_bidder_agent_id: &expected_bidder_agent_id,
        },
        &guard,
        &live,
        &mut read_io,
        &mut write_io,
        &mut read_key_work,
        &mut write_key_work,
        &mut remaining,
    )
    .unwrap();
    let child = next
        .validate_binding(
            &ModuleMarketplaceFieldV1::ArtifactBids.schema(),
            &expected.owner_key,
            StateNamespaceV1::Business,
        )
        .unwrap();
    assert_eq!(child.leaf_count, 3 + (bid_count - 1) * 7);
    let operation_reads = values.read_counts();
    let metrics = BidMutatorMetrics {
        nodes: nodes_before - read_io.nodes_remaining,
        bytes: bytes_before - read_io.read_bytes_remaining,
        value_reads: operation_reads.0 - reads_before.0,
        state_reads: operation_reads.1 - reads_before.1,
    };
    let mut expected_ids = (0..bid_count).collect::<Vec<_>>();
    expected_ids.remove(ordinal as usize);
    audit_bid_index_tree(child, &values, &memory, &expected_ids);
    let guard = transaction.guard();
    let prepared = transaction.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.reads.len(), 2);
    assert_eq!(prepared.record.writes.len(), 1);
    metrics
}

struct BidIdsOracleRead {
    ids: Vec<u64>,
    nodes: u64,
    bytes: u64,
    value_reads: usize,
    state_reads: usize,
}

fn read_bid_ids(
    child: &StateRootV1,
    values: &SharedValues,
    memory: &MemoryBudget,
    length: u64,
) -> BidIdsOracleRead {
    let mut io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 16 << 20,
    };
    let mut source = ValueReadContext {
        source: values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut io,
    };
    let reads_before = values.read_counts();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let ids = (0..length)
        .map(|ordinal| {
            bid_index::select_ordinal(
                child,
                ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap(),
                ordinal,
                &mut source,
                &mut key_work,
                1 << 20,
                8 << 20,
                &mut || false,
            )
            .unwrap()
        })
        .collect();
    let reads_after = values.read_counts();
    BidIdsOracleRead {
        ids,
        nodes: 100_000 - io.nodes_remaining,
        bytes: (64 << 20) - io.read_bytes_remaining,
        value_reads: reads_after.0 - reads_before.0,
        state_reads: reads_after.1 - reads_before.1,
    }
}

fn audit_bid_index_tree(
    child: &StateRootV1,
    values: &SharedValues,
    memory: &MemoryBudget,
    expected_ids: &[u64],
) {
    let descriptor = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    let (length_codec, length_bytes) = child_payload(child, descriptor, &"length", values, memory);
    assert_eq!(length_codec, MemberPayloadCodecV1::U64);
    assert_eq!(
        u64::from_be_bytes(length_bytes.try_into().unwrap()),
        expected_ids.len() as u64
    );
    let (root_codec, root_bytes) =
        child_payload(child, descriptor, &"sequence_root_id", values, memory);
    assert_eq!(root_codec, MemberPayloadCodecV1::U64);
    let root_id = u64::from_be_bytes(root_bytes.try_into().unwrap());

    #[expect(
        clippy::too_many_arguments,
        reason = "The independent recursive oracle threads its bounded reader and structural checks explicitly."
    )]
    fn visit(
        child: &StateRootV1,
        descriptor: super::FieldDescriptor,
        values: &SharedValues,
        memory: &MemoryBudget,
        id: u64,
        parent_id: u64,
        depth: usize,
        seen: &mut BTreeSet<u64>,
        in_order: &mut Vec<u64>,
    ) -> (u64, u64) {
        if id == bid_index::NO_BID_ID {
            return (0, 0);
        }
        assert!(depth < bid_index::MAX_BID_INDEX_DEPTH);
        assert!(seen.insert(id), "duplicate/cyclic index ID {id}");
        let key = (("bid", id), "index_node");
        let (codec, bytes) = child_payload(child, descriptor, &key, values, memory);
        assert_eq!(codec, MemberPayloadCodecV1::FixedU64x7);
        assert_eq!(bytes.len(), bid_index::BID_INDEX_NODE_BYTES);
        let field = |index: usize| {
            let start = index * 8;
            u64::from_be_bytes(bytes[start..start + 8].try_into().unwrap())
        };
        let left_id = field(0);
        let right_id = field(1);
        let left_size = field(2);
        let left_height = field(3);
        let right_size = field(4);
        let right_height = field(5);
        assert_eq!(field(6), parent_id, "parent link for index ID {id}");

        let left = visit(
            child,
            descriptor,
            values,
            memory,
            left_id,
            id,
            depth + 1,
            seen,
            in_order,
        );
        assert_eq!(
            (left_size, left_height),
            left,
            "left summary for index ID {id}"
        );
        in_order.push(id);
        let right = visit(
            child,
            descriptor,
            values,
            memory,
            right_id,
            id,
            depth + 1,
            seen,
            in_order,
        );
        assert_eq!(
            (right_size, right_height),
            right,
            "right summary for index ID {id}"
        );
        assert!(left.1.abs_diff(right.1) <= 1, "unbalanced index ID {id}");
        (
            1u64.checked_add(left.0)
                .and_then(|size| size.checked_add(right.0))
                .unwrap(),
            1 + left.1.max(right.1),
        )
    }

    let mut seen = BTreeSet::new();
    let mut in_order = Vec::new();
    let summary = visit(
        child,
        descriptor,
        values,
        memory,
        root_id,
        bid_index::NO_BID_ID,
        0,
        &mut seen,
        &mut in_order,
    );
    assert_eq!(summary.0, expected_ids.len() as u64);
    assert_eq!(seen.len(), expected_ids.len());
    assert_eq!(in_order, expected_ids);
}

#[test]
fn marketplace_bid_index_middle_delete_has_bounded_growth_with_vec_length() {
    let small = measure_middle_bid_delete(9);
    let large = measure_middle_bid_delete(129);
    eprintln!(
        "middle-delete mutator costs: 9 bids nodes={} bytes={} value_reads={}; 129 bids nodes={} bytes={} value_reads={}",
        small.nodes, small.bytes, small.value_reads, large.nodes, large.bytes, large.value_reads
    );
    assert_eq!(small.state_reads, 0);
    assert_eq!(large.state_reads, 0);
    assert!(small.nodes > 0 && small.bytes > 0 && small.value_reads > 0);
    assert!(large.nodes > 0 && large.bytes > 0 && large.value_reads > 0);
    assert!(
        large.nodes <= small.nodes * 3,
        "node reads grew with unrelated bids: small={small:?} large={large:?}"
    );
    assert!(
        large.bytes <= small.bytes * 3,
        "read bytes grew with unrelated bids: small={small:?} large={large:?}"
    );
}

#[test]
fn marketplace_bid_index_rotations_match_independent_vec_and_node_oracles() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let (mut values, parent, state_pages, mut expected) = bid_overlay_parent(&memory, 0);
    let live = parent.fence().unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent.clone(),
        [36; 32],
        &state_pages,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 32 << 20,
        },
    )
    .unwrap();
    let mut read_io = ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 1 << 35,
        write_bytes_remaining: 32 << 20,
    };
    let mut write_io = ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 1 << 35,
        write_bytes_remaining: 256 << 20,
    };
    let mut read_key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut write_key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut remaining = 100_000;
    let mut expected_ids = Vec::new();

    // Monotone tail insertion and repeated head insertion force the AVL
    // single rotations; alternating ranks then exercise both double rotations.
    let insert_ranks = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0, 0, 0, 0, 8, 4, 12, 3];
    for (stable_id, ordinal) in insert_ranks.into_iter().enumerate() {
        let bid = ModuleArtifactBidState {
            order_id: 10_000 + stable_id as u64,
            bidder_agent_id: format!("rotation/{stable_id}"),
            price_kind: if stable_id % 2 == 0 {
                ResourceKind::Electricity
            } else {
                ResourceKind::Data
            },
            price_amount: stable_id as i64 - 9,
            bid_at: u64::MAX - stable_id as u64,
        };
        let guard = transaction.guard();
        expected = apply_bid_change(
            &mut transaction,
            &mut values,
            &memory,
            &expected,
            ModuleArtifactBidChangeV1::Insert { ordinal, bid: &bid },
            &guard,
            &live,
            &mut read_io,
            &mut write_io,
            &mut read_key_work,
            &mut write_key_work,
            &mut remaining,
        )
        .unwrap();
        expected_ids.insert(ordinal as usize, stable_id as u64);
        let child = expected
            .validate_binding(
                &ModuleMarketplaceFieldV1::ArtifactBids.schema(),
                &expected.owner_key,
                StateNamespaceV1::Business,
            )
            .unwrap();
        audit_bid_index_tree(child, &values, &memory, &expected_ids);
        assert_eq!(
            read_bid_ids(child, &values, &memory, expected_ids.len() as u64).ids,
            expected_ids
        );
    }

    let delete_ranks = [0, 17, 8, 3, 13, 1, 10, 5, 0, 6, 2, 4, 3, 0, 2, 1, 0, 0, 0];
    for ordinal in delete_ranks {
        let stable_id = expected_ids[ordinal];
        let order_id = 10_000 + stable_id;
        let bidder_agent_id = format!("rotation/{stable_id}");
        let guard = transaction.guard();
        expected = apply_bid_change(
            &mut transaction,
            &mut values,
            &memory,
            &expected,
            ModuleArtifactBidChangeV1::Delete {
                ordinal: ordinal as u64,
                expected_order_id: order_id,
                expected_bidder_agent_id: &bidder_agent_id,
            },
            &guard,
            &live,
            &mut read_io,
            &mut write_io,
            &mut read_key_work,
            &mut write_key_work,
            &mut remaining,
        )
        .unwrap();
        expected_ids.remove(ordinal);
        let child = expected
            .validate_binding(
                &ModuleMarketplaceFieldV1::ArtifactBids.schema(),
                &expected.owner_key,
                StateNamespaceV1::Business,
            )
            .unwrap();
        audit_bid_index_tree(child, &values, &memory, &expected_ids);
        assert_eq!(
            read_bid_ids(child, &values, &memory, expected_ids.len() as u64).ids,
            expected_ids
        );
    }

    let guard = transaction.guard();
    let prepared = transaction.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.reads.len(), 2);
    assert_eq!(prepared.record.writes.len(), 1);
    TransitionVerifier {
        parent: &parent,
        operation: [36; 32],
        source: &state_pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 32 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
}

#[test]
fn marketplace_bid_overlay_keeps_vec_order_and_removes_duplicate_matches_one_at_a_time() {
    let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
    let (mut values, parent, mut state_pages, mut expected) = bid_overlay_parent(&memory, 9);
    let live = parent.fence().unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent.clone(),
        [34; 32],
        &state_pages,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let mut read_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 64 << 20,
    };
    let mut write_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 64 << 20,
    };
    let mut read_key_work = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_key_work = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut remaining = 20_000;
    let mut expected_ids = (0..9).collect::<Vec<_>>();

    let inserted = ModuleArtifactBidState {
        order_id: 555,
        bidder_agent_id: "inserted/雪".into(),
        price_kind: ResourceKind::Data,
        price_amount: i64::MIN,
        bid_at: u64::MAX,
    };
    let mut guard = transaction.guard();
    let mut operation_nodes = read_io.nodes_remaining;
    let mut operation_bytes = read_io.read_bytes_remaining;
    let mut operation_reads = values.read_counts();
    let mut operation_puts = values.put_count();
    expected = apply_bid_change(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::Insert {
            ordinal: 5,
            bid: &inserted,
        },
        &guard,
        &live,
        &mut read_io,
        &mut write_io,
        &mut read_key_work,
        &mut write_key_work,
        &mut remaining,
    )
    .unwrap();
    let mutation_nodes = operation_nodes - read_io.nodes_remaining;
    let mutation_bytes = operation_bytes - read_io.read_bytes_remaining;
    let mutation_reads = values.read_counts().0 - operation_reads.0;
    let mutation_state_reads = values.read_counts().1 - operation_reads.1;
    let mutation_puts = values.put_count() - operation_puts;
    expected_ids.insert(5, 9);
    let child = expected
        .validate_binding(
            &ModuleMarketplaceFieldV1::ArtifactBids.schema(),
            &expected.owner_key,
            StateNamespaceV1::Business,
        )
        .unwrap();
    let oracle = read_bid_ids(child, &values, &memory, expected_ids.len() as u64);
    assert_eq!(oracle.ids, expected_ids, "sequence after insertion");
    audit_bid_index_tree(child, &values, &memory, &expected_ids);
    eprintln!(
        "mutator insert: nodes={mutation_nodes} bytes={mutation_bytes} value_reads={mutation_reads} state_reads={mutation_state_reads} puts={mutation_puts}"
    );
    eprintln!(
        "insert full readback oracle: nodes={} bytes={} value_reads={} state_reads={}",
        oracle.nodes, oracle.bytes, oracle.value_reads, oracle.state_reads
    );

    for ordinal in [0, 3, 7] {
        guard = transaction.guard();
        operation_nodes = read_io.nodes_remaining;
        operation_bytes = read_io.read_bytes_remaining;
        operation_reads = values.read_counts();
        operation_puts = values.put_count();
        let change_result = apply_bid_change(
            &mut transaction,
            &mut values,
            &memory,
            &expected,
            ModuleArtifactBidChangeV1::Delete {
                ordinal,
                expected_order_id: 77,
                expected_bidder_agent_id: "duplicate/β",
            },
            &guard,
            &live,
            &mut read_io,
            &mut write_io,
            &mut read_key_work,
            &mut write_key_work,
            &mut remaining,
        );
        expected = change_result.unwrap_or_else(|error| {
            panic!(
                "delete ordinal {ordinal} failed with {error:?}; read nodes/bytes remaining={}/{}, write nodes/read-bytes remaining={}/{}, puts={}; operation read nodes={} bytes={} value_reads={} state_reads={} puts={}",
                read_io.nodes_remaining,
                read_io.read_bytes_remaining,
                write_io.nodes_remaining,
                write_io.read_bytes_remaining,
                values.put_count(),
                operation_nodes - read_io.nodes_remaining,
                operation_bytes - read_io.read_bytes_remaining,
                values.read_counts().0 - operation_reads.0,
                values.read_counts().1 - operation_reads.1,
                values.put_count() - operation_puts,
            )
        });
        let mutation_nodes = operation_nodes - read_io.nodes_remaining;
        let mutation_bytes = operation_bytes - read_io.read_bytes_remaining;
        let mutation_reads = values.read_counts().0 - operation_reads.0;
        let mutation_state_reads = values.read_counts().1 - operation_reads.1;
        let mutation_puts = values.put_count() - operation_puts;
        expected_ids.remove(ordinal as usize);
        let child = expected
            .validate_binding(
                &ModuleMarketplaceFieldV1::ArtifactBids.schema(),
                &expected.owner_key,
                StateNamespaceV1::Business,
            )
            .unwrap();
        let oracle = read_bid_ids(child, &values, &memory, expected_ids.len() as u64);
        assert_eq!(
            oracle.ids, expected_ids,
            "sequence after deleting ordinal {ordinal}"
        );
        audit_bid_index_tree(child, &values, &memory, &expected_ids);
        eprintln!(
            "mutator delete {ordinal}: nodes={mutation_nodes} bytes={mutation_bytes} value_reads={mutation_reads} state_reads={mutation_state_reads} puts={mutation_puts}"
        );
        eprintln!(
            "delete {ordinal} full readback oracle: nodes={} bytes={} value_reads={} state_reads={}",
            oracle.nodes, oracle.bytes, oracle.value_reads, oracle.state_reads
        );
    }

    guard = transaction.guard();
    let prepared = transaction.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.reads.len(), 2);
    assert_eq!(prepared.record.writes.len(), 1);
    assert_eq!(
        prepared.record.writes[0].namespace,
        StateNamespaceV1::Business
    );
    assert_eq!(
        prepared.record.writes[0].mutation.before,
        Some(
            parent
                .roots
                .state_root(StateNamespaceV1::Business)
                .get(&prepared.record.writes[0].mutation.key, &state_pages)
                .unwrap()
                .unwrap()
        )
    );
    TransitionVerifier {
        parent: &parent,
        operation: [34; 32],
        source: &state_pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 16 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
    for page in &prepared.new_nodes {
        state_pages
            .0
            .insert(page.digest.clone(), page.bytes.clone());
    }

    let descriptor = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    let outer_key = WorldFieldKeyV1::new(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        Some({
            let mut semantic = Vec::new();
            write_semantic_key_v1(&"bid-map-key", &mut semantic).unwrap();
            *blake3::hash(&semantic).as_bytes()
        }),
    )
    .unwrap();
    let outer_root = prepared
        .record
        .after
        .state_root(StateNamespaceV1::Business)
        .get(outer_key.as_bytes(), &state_pages)
        .unwrap()
        .unwrap();
    let (outer_info, outer_semantic, outer_payload) =
        read_member(&outer_root, &outer_key, &values, &memory);
    assert_eq!(outer_info.codec, MemberPayloadCodecV1::RecordRoot);
    assert_eq!(outer_semantic, {
        let mut semantic = Vec::new();
        write_semantic_key_v1(&"bid-map-key", &mut semantic).unwrap();
        semantic
    });
    assert_eq!(outer_payload, expected.canonical_bytes().unwrap());

    let child = expected
        .validate_binding(
            &ModuleMarketplaceFieldV1::ArtifactBids.schema(),
            &expected.owner_key,
            StateNamespaceV1::Business,
        )
        .unwrap();
    assert_eq!(child.leaf_count, 3 + 7 * 7);
    let oracle = read_bid_ids(child, &values, &memory, 7);
    eprintln!(
        "independent read_bid_ids oracle: nodes={} bytes={} value_reads={} state_reads={}",
        oracle.nodes, oracle.bytes, oracle.value_reads, oracle.state_reads
    );
    assert_eq!(oracle.ids, [1, 2, 3, 9, 5, 6, 7]);
    let bids_field = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    let expected_order_ids = [101, 102, 103, 555, 105, 106, 107];
    for (stable_id, order_id) in [1, 2, 3, 9, 5, 6, 7].into_iter().zip(expected_order_ids) {
        let key = |field| (("bid", stable_id), field);
        assert_eq!(
            child_payload(child, bids_field, &key("order_id"), &values, &memory),
            (
                MemberPayloadCodecV1::U64,
                (order_id as u64).to_be_bytes().to_vec()
            )
        );
        if stable_id == 9 {
            assert_eq!(
                child_payload(child, bids_field, &key("bidder_agent_id"), &values, &memory,),
                (
                    MemberPayloadCodecV1::Text,
                    "inserted/雪".as_bytes().to_vec()
                )
            );
            assert_eq!(
                child_payload(child, bids_field, &key("price_amount"), &values, &memory),
                (MemberPayloadCodecV1::I64, i64::MIN.to_be_bytes().to_vec())
            );
        }
    }
    for deleted_id in [0u64, 4, 8] {
        let semantic_key = (("bid", deleted_id), "present");
        let mut semantic = Vec::new();
        write_semantic_key_v1(&semantic_key, &mut semantic).unwrap();
        let deleted_key = WorldFieldKeyV1::new(
            bids_field,
            WorldFieldRecordKindV1::Entry,
            Some(*blake3::hash(&semantic).as_bytes()),
        )
        .unwrap();
        assert!(
            child
                .get(deleted_key.as_bytes(), &values)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn marketplace_bid_overlay_expected_mismatch_poisoned_after_prior_prefix() {
    let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
    let (mut values, parent, state_pages, mut expected) = bid_overlay_parent(&memory, 9);
    let live = parent.fence().unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent,
        [35; 32],
        &state_pages,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let mut read_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 64 << 20,
    };
    let mut write_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 64 << 20,
    };
    let mut read_key_work = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_key_work = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut remaining = 20_000;
    let inserted = ModuleArtifactBidState {
        order_id: 555,
        bidder_agent_id: "inserted".into(),
        price_kind: ResourceKind::Electricity,
        price_amount: 10,
        bid_at: 11,
    };
    let guard = transaction.guard();
    expected = apply_bid_change(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::Insert {
            ordinal: 5,
            bid: &inserted,
        },
        &guard,
        &live,
        &mut read_io,
        &mut write_io,
        &mut read_key_work,
        &mut write_key_work,
        &mut remaining,
    )
    .unwrap();
    let puts_before_bad_change = values.put_count();
    let guard = transaction.guard();
    let error = apply_bid_change(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::Delete {
            ordinal: 0,
            expected_order_id: 999,
            expected_bidder_agent_id: "duplicate/β",
        },
        &guard,
        &live,
        &mut read_io,
        &mut write_io,
        &mut read_key_work,
        &mut write_key_work,
        &mut remaining,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
            "marketplace bid delete expected value"
        ))
    ));
    assert_eq!(values.put_count(), puts_before_bad_change);
    assert!(matches!(
        transaction.seal(&guard, &live, 0),
        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
    ));
}
