use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn literal_pair_digest(order_id: u64, bidder_agent_id: &str) -> [u8; 32] {
    let mut preimage = b"oasis7.iws.market.bid-membership.v1\0".to_vec();
    preimage.extend_from_slice(&order_id.to_be_bytes());
    preimage.extend_from_slice(bidder_agent_id.as_bytes());
    *blake3::hash(&preimage).as_bytes()
}

fn literal_pair_group_key(digest: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut key = String::from("bid_group/");
    for byte in digest {
        key.push(char::from(HEX[(byte >> 4) as usize]));
        key.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    key
}

fn literal_group_child(
    bids: &StateRootV1,
    order_id: u64,
    bidder_agent_id: &str,
    values: &SharedValues,
    memory: &MemoryBudget,
) -> Option<(StateRootV1, [u8; 32])> {
    let pair_digest = literal_pair_digest(order_id, bidder_agent_id);
    let group_key = literal_pair_group_key(&pair_digest);
    let descriptor = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    let mut semantic = Vec::new();
    write_semantic_key_v1(&group_key, &mut semantic).unwrap();
    let field_key = WorldFieldKeyV1::new(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic).as_bytes()),
    )
    .unwrap();
    let value_root = bids.get(field_key.as_bytes(), values).unwrap()?;
    let (info, actual_semantic, payload) = read_member(&value_root, &field_key, values, memory);
    assert_eq!(info.codec, MemberPayloadCodecV1::RecordRoot);
    assert_eq!(actual_semantic, semantic);
    assert_eq!(
        payload.len(),
        oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES
    );
    let reference = RecordRootRefV1::from_canonical_bytes(&payload).unwrap();
    let schema = *blake3::hash(b"oasis7.iws.market.bid-membership-group.v1\0").as_bytes();
    assert_eq!(reference.schema, schema);
    assert_eq!(reference.owner_key, pair_digest);
    let child = reference
        .validate_binding(&schema, &pair_digest, StateNamespaceV1::Business)
        .unwrap()
        .clone();
    Some((child, pair_digest))
}

fn begin_bid_transaction<'a>(
    parent: &ExecutionParent,
    state_pages: &'a StatePages,
    memory: &'a MemoryBudget,
    operation: u8,
) -> ExecutionTransaction<'a, StatePages> {
    ExecutionTransaction::begin(
        parent.clone(),
        [operation; 32],
        state_pages,
        memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 32 << 20,
        },
    )
    .unwrap()
}

fn bid_io_budgets() -> (ValueIoBudget, ValueIoBudget) {
    let make = || ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 256 << 20,
    };
    (make(), make())
}

fn bid_key_work() -> (SemanticKeyBudget, SemanticKeyBudget) {
    let make = || SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    (make(), make())
}

#[test]
fn marketplace_bid_identity_migration_and_delete_matching_preserve_vec_and_pair_groups() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let (mut values, parent, mut state_pages, mut expected) = bid_overlay_parent(&memory, 9);
    let live = parent.fence().unwrap();
    let mut transaction = begin_bid_transaction(&parent, &state_pages, &memory, 61);
    let (mut read_io, mut write_io) = bid_io_budgets();
    let (mut read_key_work, mut write_key_work) = bid_key_work();
    let mut remaining = 50_000;
    let original = expected.clone();
    let old_order = 101;
    let old_bidder = "bidder/1";
    let new_order = 77;
    let new_bidder = "duplicate/β";

    let guard = transaction.guard();
    expected = apply_bid_change(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::UpdateIdentity {
            ordinal: 1,
            expected_order_id: old_order,
            expected_bidder_agent_id: old_bidder,
            after_order_id: new_order,
            after_bidder_agent_id: new_bidder,
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
    assert_ne!(
        expected, original,
        "identity migration must change the root"
    );

    let bids = expected
        .validate_binding(
            &ModuleMarketplaceFieldV1::ArtifactBids.schema(),
            &expected.owner_key,
            StateNamespaceV1::Business,
        )
        .unwrap();
    assert_eq!(
        read_bid_ids(bids, &values, &memory, 9).ids,
        (0..9).collect::<Vec<_>>()
    );
    assert_eq!(
        child_payload(
            bids,
            ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap(),
            &(("bid", 1u64), "order_id"),
            &values,
            &memory,
        ),
        (MemberPayloadCodecV1::U64, new_order.to_be_bytes().to_vec())
    );
    assert_eq!(
        child_payload(
            bids,
            ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap(),
            &(("bid", 1u64), "bidder_agent_id"),
            &values,
            &memory,
        ),
        (MemberPayloadCodecV1::Text, new_bidder.as_bytes().to_vec())
    );
    assert!(literal_group_child(bids, old_order, old_bidder, &values, &memory).is_none());
    let (new_group, _) =
        literal_group_child(bids, new_order, new_bidder, &values, &memory).unwrap();
    assert_eq!(
        child_payload(
            &new_group,
            ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap(),
            &"order_id",
            &values,
            &memory,
        ),
        (MemberPayloadCodecV1::U64, new_order.to_be_bytes().to_vec())
    );
    assert_eq!(
        child_payload(
            &new_group,
            ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap(),
            &"bidder_agent_id",
            &values,
            &memory,
        ),
        (MemberPayloadCodecV1::Text, new_bidder.as_bytes().to_vec())
    );
    assert_eq!(
        child_payload(
            &new_group,
            ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap(),
            &"length",
            &values,
            &memory,
        ),
        (MemberPayloadCodecV1::U64, 4u64.to_be_bytes().to_vec())
    );
    audit_bid_index_tree(&new_group, &values, &memory, &[0, 1, 4, 8]);

    let guard = transaction.guard();
    expected = apply_bid_change(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::DeleteMatching {
            order_id: new_order,
            bidder_agent_id: new_bidder,
            expected_matches: 4,
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

    let bids = expected
        .validate_binding(
            &ModuleMarketplaceFieldV1::ArtifactBids.schema(),
            &expected.owner_key,
            StateNamespaceV1::Business,
        )
        .unwrap();
    let remaining_ids = [2, 3, 5, 6, 7];
    assert_eq!(read_bid_ids(bids, &values, &memory, 5).ids, remaining_ids);
    audit_bid_index_tree(bids, &values, &memory, &remaining_ids);
    assert!(literal_group_child(bids, new_order, new_bidder, &values, &memory).is_none());
    for (stable_id, order_id) in remaining_ids.into_iter().zip([102u64, 103, 105, 106, 107]) {
        assert_eq!(
            child_payload(
                bids,
                ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap(),
                &(("bid", stable_id), "order_id"),
                &values,
                &memory,
            ),
            (MemberPayloadCodecV1::U64, order_id.to_be_bytes().to_vec())
        );
    }
    for deleted_id in [0u64, 1, 4, 8] {
        for name in ["present", "order_id", "bidder_agent_id"] {
            let mut semantic = Vec::new();
            write_semantic_key_v1(&(("bid", deleted_id), name), &mut semantic).unwrap();
            let key = WorldFieldKeyV1::new(
                ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap(),
                WorldFieldRecordKindV1::Entry,
                Some(*blake3::hash(&semantic).as_bytes()),
            )
            .unwrap();
            assert!(bids.get(key.as_bytes(), &values).unwrap().is_none());
        }
    }

    let guard = transaction.guard();
    let prepared = transaction.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.reads.len(), 2);
    assert_eq!(prepared.record.writes.len(), 1);
    TransitionVerifier {
        parent: &parent,
        operation: [61; 32],
        source: &state_pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 32 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
    for page in &prepared.new_nodes {
        state_pages
            .0
            .insert(page.digest.clone(), page.bytes.clone());
    }
}

#[test]
fn marketplace_bid_noop_is_read_only_and_checks_final_cancellation() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let (mut values, parent, state_pages, expected) = bid_overlay_parent(&memory, 9);
    let live = parent.fence().unwrap();
    let mut transaction = begin_bid_transaction(&parent, &state_pages, &memory, 62);
    let (mut read_io, mut write_io) = bid_io_budgets();
    let (mut read_key_work, mut write_key_work) = bid_key_work();
    let mut remaining = 50_000;
    let guard = transaction.guard();
    let result = apply_bid_change(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::UpdateIdentity {
            ordinal: 2,
            expected_order_id: 102,
            expected_bidder_agent_id: "bidder/2",
            after_order_id: 102,
            after_bidder_agent_id: "bidder/2",
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
    assert_eq!(result, expected);
    let guard = transaction.guard();
    let prepared = transaction.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.reads.len(), 2);
    assert!(prepared.record.writes.is_empty());

    let (mut values, parent, state_pages, expected) = bid_overlay_parent(&memory, 9);
    let live = parent.fence().unwrap();
    let mut transaction = begin_bid_transaction(&parent, &state_pages, &memory, 63);
    let (mut read_io, mut write_io) = bid_io_budgets();
    let (mut read_key_work, mut write_key_work) = bid_key_work();
    let mut remaining = 50_000;
    let probes = AtomicUsize::new(0);
    let guard = transaction.guard();
    let _ = apply_bid_change_with_cancel(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::UpdateIdentity {
            ordinal: 2,
            expected_order_id: 102,
            expected_bidder_agent_id: "bidder/2",
            after_order_id: 102,
            after_bidder_agent_id: "bidder/2",
        },
        &guard,
        &live,
        &mut read_io,
        &mut write_io,
        &mut read_key_work,
        &mut write_key_work,
        &mut remaining,
        || {
            probes.fetch_add(1, Ordering::SeqCst);
            false
        },
        || false,
    )
    .unwrap();
    let successful_probes = probes.load(Ordering::SeqCst);
    assert!(successful_probes > 0);

    let (mut values, parent, state_pages, expected) = bid_overlay_parent(&memory, 9);
    let live = parent.fence().unwrap();
    let mut transaction = begin_bid_transaction(&parent, &state_pages, &memory, 64);
    let (mut read_io, mut write_io) = bid_io_budgets();
    let (mut read_key_work, mut write_key_work) = bid_key_work();
    let mut remaining = 50_000;
    let calls = AtomicUsize::new(0);
    let puts_before_cancelled_noop = values.put_count();
    let guard = transaction.guard();
    let error = apply_bid_change_with_cancel(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::UpdateIdentity {
            ordinal: 2,
            expected_order_id: 102,
            expected_bidder_agent_id: "bidder/2",
            after_order_id: 102,
            after_bidder_agent_id: "bidder/2",
        },
        &guard,
        &live,
        &mut read_io,
        &mut write_io,
        &mut read_key_work,
        &mut write_key_work,
        &mut remaining,
        || calls.fetch_add(1, Ordering::SeqCst) + 1 == successful_probes,
        || false,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
            ValueTreeError::Cancelled
        )
    ));
    assert_eq!(calls.load(Ordering::SeqCst), successful_probes);
    assert_eq!(values.put_count(), puts_before_cancelled_noop);
    assert!(matches!(
        transaction.seal(&guard, &live, 0),
        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
    ));
}

#[test]
fn marketplace_bid_matching_delete_rejects_wrong_count_without_writes() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let (mut values, parent, state_pages, expected) = bid_overlay_parent(&memory, 9);
    let live = parent.fence().unwrap();
    let mut transaction = begin_bid_transaction(&parent, &state_pages, &memory, 65);
    let (mut read_io, mut write_io) = bid_io_budgets();
    let (mut read_key_work, mut write_key_work) = bid_key_work();
    let mut remaining = 50_000;
    let puts_before = values.put_count();
    let guard = transaction.guard();
    let error = apply_bid_change(
        &mut transaction,
        &mut values,
        &memory,
        &expected,
        ModuleArtifactBidChangeV1::DeleteMatching {
            order_id: 77,
            bidder_agent_id: "duplicate/β",
            expected_matches: 2,
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
            "marketplace bid matching delete expected count"
        ))
    ));
    assert_eq!(values.put_count(), puts_before);
    assert!(matches!(
        transaction.seal(&guard, &live, 0),
        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
    ));
}
