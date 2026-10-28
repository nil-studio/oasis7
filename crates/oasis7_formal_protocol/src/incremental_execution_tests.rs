use super::*;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Default)]
struct Pages {
    nodes: BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>,
    reads: Cell<usize>,
}
impl MerkleNodeReader for Pages {
    type Node = Vec<u8>;
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, String> {
        self.reads.set(self.reads.get() + 1);
        let Some(page) = self.nodes.get(digest) else {
            return Ok(None);
        };
        if page.len() > max_bytes {
            return Err("source page bound".into());
        }
        // The caller owns its already admitted per-walk scratch. Cache data
        // keeps its independent charge while this temporary copy is consumed.
        Ok(Some(page.as_slice().to_vec()))
    }
}
impl Pages {
    fn persist(&mut self, prepared: &PreparedTransition, memory: &MemoryBudget) {
        for node in &prepared.new_nodes {
            let charge = memory
                .reserve(node.bytes.len() + 512, MemoryClass::Ordinary)
                .unwrap();
            self.nodes.insert(
                node.digest.clone(),
                Arc::new(charge.retain(node.bytes.clone())),
            );
        }
    }
    fn fork(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            reads: Cell::new(0),
        }
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 4096,
        max_writes: 1024,
        max_facts: 4096,
        max_pending_node_bytes: 32 * 1024 * 1024,
    }
}
fn empty_parent() -> ExecutionParent {
    ExecutionParent {
        commit: [7; 32],
        roots: RootSetV1::empty("fixture-world".into(), [8; 32]).unwrap(),
        authority: AuthorityBinding {
            token: [9; 32],
            epoch: 3,
        },
    }
}
fn memory() -> MemoryBudget {
    MemoryBudget::new(128 * 1024 * 1024, 2 * 1024 * 1024).unwrap()
}
fn fact(kind: ExecutionFactKindV1, n: u8) -> ExecutionFactRefV1 {
    ExecutionFactRefV1 { kind, id: [n; 32] }
}

#[test]
fn single_overlay_coalesces_all_namespaces_but_retains_original_fact_order() {
    let pages = Pages::default();
    let memory = memory();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, limits()).unwrap();
    let old = txn.guard();
    assert_eq!(txn.read(StateNamespaceV1::Business, b"a").unwrap(), None);
    txn.append_fact(fact(ExecutionFactKindV1::Input, 1))
        .unwrap();
    txn.write(StateNamespaceV1::Business, b"a", Some([1; 32]))
        .unwrap();
    txn.write(StateNamespaceV1::Business, b"a", Some([2; 32]))
        .unwrap();
    assert_eq!(
        txn.read(StateNamespaceV1::Business, b"a").unwrap(),
        Some([2; 32])
    );
    txn.write(StateNamespaceV1::Business, b"a", None).unwrap();
    txn.append_fact(fact(ExecutionFactKindV1::Event, 2))
        .unwrap();
    txn.append_fact(fact(ExecutionFactKindV1::Result, 3))
        .unwrap();
    for (index, namespace) in NAMESPACES.into_iter().enumerate().skip(1) {
        txn.write(namespace, b"control", Some([index as u8; 32]))
            .unwrap();
    }
    assert!(matches!(
        txn.validate_guard(&old, &live),
        Err(ExecutionError::StaleGuard)
    ));
    let guard = txn.guard();
    let reads = pages.reads.get();
    txn.validate_guard(&guard, &live).unwrap();
    assert_eq!(pages.reads.get(), reads);
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.before, parent.roots);
    assert_eq!(
        (
            prepared.record.after.height,
            prepared.record.after.simulation_tick
        ),
        (1, 0)
    );
    assert_eq!(prepared.record.reads.len(), 6);
    assert_eq!(prepared.record.writes.len(), 5);
    assert!(
        prepared
            .record
            .writes
            .iter()
            .all(|write| write.namespace != StateNamespaceV1::Business)
    );
    assert_eq!(
        prepared.record.facts,
        vec![
            fact(ExecutionFactKindV1::Input, 1),
            fact(ExecutionFactKindV1::Event, 2),
            fact(ExecutionFactKindV1::Result, 3)
        ]
    );
    let encoded = prepared
        .record
        .canonical_bytes(&memory, MemoryClass::Ordinary)
        .unwrap();
    let decoded =
        TransitionRecordV1::from_canonical_bytes(&encoded, &memory, MemoryClass::Ordinary).unwrap();
    assert_eq!(*decoded, prepared.record);
    let verified = TransitionVerifier {
        parent: &parent,
        operation: [1; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: limits().max_pending_node_bytes,
    }
    .verify(&decoded)
    .unwrap();
    assert_eq!(*verified, *prepared);
    assert_eq!(
        RootSetV1::from_canonical_bytes(&decoded.after.canonical_bytes().unwrap()).unwrap(),
        decoded.after
    );
    drop(encoded);
    drop(decoded);
    drop(verified);
    drop(prepared);
    drop(old);
    drop(guard);
    assert_eq!(memory.used(), 0);
}

#[test]
fn guards_bind_revision_parent_roots_and_authority_without_serializing_state() {
    let pages = Pages::default();
    let memory = memory();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, limits()).unwrap();
    let guard = txn.guard();
    for changed in 0..4 {
        let mut other = live;
        match changed {
            0 => other.parent_commit[0] ^= 1,
            1 => other.parent_roots[0] ^= 1,
            2 => other.authority.token[0] ^= 1,
            _ => other.authority.epoch += 1,
        }
        assert!(matches!(
            txn.validate_guard(&guard, &other),
            Err(ExecutionError::ParentConflict)
        ));
    }
    let other = ExecutionTransaction::begin(parent, [1; 32], &pages, &memory, limits()).unwrap();
    assert!(matches!(
        other.validate_guard(&guard, &live),
        Err(ExecutionError::StaleGuard)
    ));
    // Even a same-tick write that coalesces back to absence invalidates prepare.
    txn.write(StateNamespaceV1::Scheduler, b"wake", None)
        .unwrap();
    assert!(matches!(
        txn.validate_guard(&guard, &live),
        Err(ExecutionError::StaleGuard)
    ));
    assert_eq!(pages.reads.get(), 0);
    drop(other);
    drop(txn);
    assert_eq!(memory.used(), 64);
    drop(guard);
    assert_eq!(memory.used(), 0);
}

#[test]
fn altered_read_control_write_and_after_roots_do_not_verify() {
    let pages = Pages::default();
    let memory = memory();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, limits()).unwrap();
    txn.write(StateNamespaceV1::Business, b"balance", Some([3; 32]))
        .unwrap();
    txn.write(StateNamespaceV1::Allocator, b"next", Some([4; 32]))
        .unwrap();
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [1; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: limits().max_pending_node_bytes,
    };
    for mutation in 0..8 {
        let mut bad = prepared.record.clone();
        match mutation {
            0 => bad.reads[0].before = Some([9; 32]),
            1 => {
                bad.writes.pop();
            }
            2 => bad.writes.reverse(),
            3 => bad.after.state[0] = parent.roots.state[0].clone(),
            4 => bad.before.world_id = "other-world".into(),
            5 => bad.after.genesis_config[0] ^= 1,
            6 => bad.authority.epoch += 1,
            _ => bad.operation[0] ^= 1,
        }
        assert!(verifier.verify(&bad).is_err(), "mutation={mutation}");
    }
}

#[test]
fn quotas_reject_before_semantic_mutation_and_release_owned_memory() {
    let pages = Pages::default();
    let memory = memory();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut caps = limits();
    caps.max_reads = 1;
    caps.max_writes = 1;
    caps.max_facts = 1;
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, caps).unwrap();
    txn.write(StateNamespaceV1::Business, b"a", Some([1; 32]))
        .unwrap();
    let guard = txn.guard();
    assert!(matches!(
        txn.write(StateNamespaceV1::Business, b"b", Some([2; 32])),
        Err(ExecutionError::ResourceLimited(_))
    ));
    assert!(matches!(
        txn.validate_guard(&guard, &live),
        Err(ExecutionError::Aborted)
    ));
    assert!(matches!(
        txn.append_fact(fact(ExecutionFactKindV1::Input, 1)),
        Err(ExecutionError::Aborted)
    ));
    assert!(matches!(
        txn.seal(&guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));
    drop(guard);
    assert_eq!(memory.used(), 0);

    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, caps).unwrap();
    txn.append_fact(fact(ExecutionFactKindV1::Input, 1))
        .unwrap();
    let guard = txn.guard();
    assert!(matches!(
        txn.append_fact(fact(ExecutionFactKindV1::Event, 2)),
        Err(ExecutionError::ResourceLimited(_))
    ));
    assert!(matches!(
        txn.seal(&guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));
    drop(guard);
    assert_eq!(memory.used(), 0);

    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, caps).unwrap();
    txn.write(StateNamespaceV1::Business, b"a", Some([1; 32]))
        .unwrap();
    txn.append_fact(fact(ExecutionFactKindV1::Input, 1))
        .unwrap();
    let guard = txn.guard();
    let batch = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(batch.record.writes.len(), 1);
    drop(batch);
    drop(guard);
    assert_eq!(memory.used(), 0);

    let small = MemoryBudget::new(512 * 1024, 0).unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [1; 32], &pages, &small, limits()).unwrap();
    txn.write(StateNamespaceV1::Effects, b"pending", Some([1; 32]))
        .unwrap();
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &live, 0),
        Err(ExecutionError::Memory(_))
    ));
    drop(guard);
    assert_eq!(small.used(), 0);
}

#[test]
fn authenticated_transition_suffix_restores_roots_and_accepts_further_work() {
    let memory = memory();
    let mut pages = Pages::default();
    let mut parent = empty_parent();
    let authority = parent.authority;
    let mut suffix = Vec::new();
    let mut checkpoint = None;
    for height in 1..=6u8 {
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [height; 32], &pages, &memory, limits())
                .unwrap();
        for namespace in [
            StateNamespaceV1::Business,
            StateNamespaceV1::Allocator,
            StateNamespaceV1::Scheduler,
        ] {
            assert_eq!(
                txn.read(namespace, b"counter").unwrap(),
                if height == 1 {
                    None
                } else {
                    Some([height - 1; 32])
                }
            );
            txn.write(namespace, b"counter", Some([height; 32]))
                .unwrap();
        }
        let guard = txn.guard();
        let batch = txn.seal(&guard, &live, u64::from(height / 2)).unwrap();
        pages.persist(&batch, &memory);
        parent = ExecutionParent {
            commit: [100 + height; 32],
            roots: batch.record.after.clone(),
            authority,
        };
        if height == 2 {
            checkpoint = Some((parent.clone(), pages.fork()));
        }
        if (3..=5).contains(&height) {
            suffix.push(batch.record.clone());
        }
    }
    let (mut restored, mut recovered_pages) = checkpoint.unwrap();
    for record in suffix {
        let verified = TransitionVerifier {
            parent: &restored,
            operation: record.operation,
            source: &recovered_pages,
            memory: &memory,
            class: MemoryClass::Recovery,
            max_pending_node_bytes: limits().max_pending_node_bytes,
        }
        .verify(&record)
        .unwrap();
        recovered_pages.persist(&verified, &memory);
        restored = ExecutionParent {
            commit: [100 + record.operation[0]; 32],
            roots: record.after,
            authority,
        };
    }
    let live = restored.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(restored, [6; 32], &recovered_pages, &memory, limits())
            .unwrap();
    for namespace in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Allocator,
        StateNamespaceV1::Scheduler,
    ] {
        assert_eq!(txn.read(namespace, b"counter").unwrap(), Some([5; 32]));
        txn.write(namespace, b"counter", Some([6; 32])).unwrap();
    }
    let guard = txn.guard();
    let continued = txn.seal(&guard, &live, 3).unwrap();
    assert_eq!(continued.record.after, parent.roots);
    assert_eq!(continued.record.after.height, 6);
    assert_eq!(continued.record.after.simulation_tick, 3);
}

#[test]
fn codec_rejects_bad_counts_tags_trailing_bytes_and_missing_root_namespaces() {
    let memory = memory();
    let pages = Pages::default();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [2; 32], &pages, &memory, limits()).unwrap();
    txn.write(StateNamespaceV1::Allocator, b"id", Some([3; 32]))
        .unwrap();
    txn.append_fact(fact(ExecutionFactKindV1::Input, 2))
        .unwrap();
    let guard = txn.guard();
    let batch = txn.seal(&guard, &live, 0).unwrap();
    let encoded = batch
        .record
        .canonical_bytes(&memory, MemoryClass::Ordinary)
        .unwrap();
    let read_count_offset = b"oasis7.iws.transition.v1\0".len()
        + 4
        + 104
        + 2
        + batch.record.before.canonical_bytes().unwrap().len()
        + 2
        + batch.record.after.canonical_bytes().unwrap().len();
    let constrained = MemoryBudget::new(encoded.len() * 2 + 64 * 1024, 0).unwrap();
    for count in [4096u16, u16::MAX] {
        let mut bad = (*encoded).clone();
        bad[read_count_offset..read_count_offset + 2].copy_from_slice(&count.to_be_bytes());
        // Refuse before allocating the declared read sequence, even when the
        // process has only the fixed decode workspace available.
        assert!(matches!(
            TransitionRecordV1::from_canonical_bytes(&bad, &constrained, MemoryClass::Ordinary),
            Err(ExecutionError::Invalid(_))
        ));
        assert_eq!(constrained.used(), 0);
    }
    for change in 0..4 {
        let mut bad = (*encoded).clone();
        match change {
            0 => bad[0] ^= 1,
            1 => bad.push(0),
            2 => {
                bad.pop();
            }
            _ => {
                let tag = bad.len() - 33;
                bad[tag] = 4;
            }
        }
        assert!(
            TransitionRecordV1::from_canonical_bytes(&bad, &memory, MemoryClass::Ordinary).is_err()
        );
    }
    let mut bad = parent.roots.clone();
    bad.state.swap(0, 1);
    assert!(bad.validate().is_err());
    let root_bytes = parent.roots.canonical_bytes().unwrap();
    assert!(RootSetV1::from_canonical_bytes(&root_bytes[..root_bytes.len() - 1]).is_err());
}

#[test]
fn administrative_root_set_and_transition_match_frozen_manual_encoding() {
    fn expected_roots(height: u64) -> Vec<u8> {
        let mut payload = 13u16.to_be_bytes().to_vec();
        payload.extend_from_slice(b"fixture-world");
        payload.extend_from_slice(&[8; 32]);
        payload.extend_from_slice(&height.to_be_bytes());
        payload.extend_from_slice(&0u64.to_be_bytes());
        let empty = hex::decode("647e04d7deacf195db202f3836807908b9c7061687033623b3fa1da1e1a83d96")
            .unwrap();
        for namespace in 1..=6u16 {
            payload.extend_from_slice(b"oasis7.iws.state-root.v1\0");
            payload.extend_from_slice(&42u32.to_be_bytes());
            payload.extend_from_slice(&namespace.to_be_bytes());
            payload.extend_from_slice(&0u64.to_be_bytes());
            payload.extend_from_slice(&empty);
        }
        let mut bytes = b"oasis7.iws.root-set.v1\0".to_vec();
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend(payload);
        bytes
    }
    let memory = memory();
    let pages = Pages::default();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    assert_eq!(parent.roots.canonical_bytes().unwrap(), expected_roots(0));
    let txn = ExecutionTransaction::begin(parent, [1; 32], &pages, &memory, limits()).unwrap();
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    let mut payload = [7u8; 32].to_vec();
    payload.extend_from_slice(&[1; 32]);
    payload.extend_from_slice(&[9; 32]);
    payload.extend_from_slice(&3u64.to_be_bytes());
    for roots in [expected_roots(0), expected_roots(1)] {
        payload.extend_from_slice(&(roots.len() as u16).to_be_bytes());
        payload.extend(roots);
    }
    payload.extend_from_slice(&[0; 6]);
    let mut expected = b"oasis7.iws.transition.v1\0".to_vec();
    expected.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    expected.extend(payload);
    let actual = prepared
        .record
        .canonical_bytes(&memory, MemoryClass::Ordinary)
        .unwrap();
    assert_eq!(*actual, expected);
    assert_eq!(
        hex::encode(prepared.record.before.digest().unwrap()),
        "b2f9a7f75db45951e3f4fde15054337e72f9737472dc2a6956966bbea02e56d3"
    );
    assert_eq!(
        hex::encode(prepared.record.after.digest().unwrap()),
        "e081129e1adf2cd3bd3f6dab7eabcd49b3319cc5db97d9bf0b55005425d5aea4"
    );
    assert_eq!(
        blake3::hash(&actual).to_hex().as_str(),
        "bff56725591b2266c72faa4b92959df4589053e81bd6bcf29bf4ab6bce7b1830"
    );
}

#[test]
fn coalesced_write_and_read_order_does_not_change_transition_bytes() {
    let memory = memory();
    let pages = Pages::default();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut first =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, limits()).unwrap();
    let mut second =
        ExecutionTransaction::begin(parent, [1; 32], &pages, &memory, limits()).unwrap();
    for key in [b"a", b"b", b"c"] {
        first
            .write(StateNamespaceV1::Business, key, Some([1; 32]))
            .unwrap();
        first
            .write(StateNamespaceV1::Business, key, Some([2; 32]))
            .unwrap();
    }
    for key in [b"c", b"b", b"a"] {
        second
            .write(StateNamespaceV1::Business, key, Some([2; 32]))
            .unwrap();
    }
    let first_guard = first.guard();
    let second_guard = second.guard();
    let first = first.seal(&first_guard, &live, 0).unwrap();
    let second = second.seal(&second_guard, &live, 0).unwrap();
    assert_eq!(*first, *second);
    assert_eq!(
        *first
            .record
            .canonical_bytes(&memory, MemoryClass::Ordinary)
            .unwrap(),
        *second
            .record
            .canonical_bytes(&memory, MemoryClass::Ordinary)
            .unwrap()
    );
}

#[test]
fn resource_failure_cannot_seal_a_partial_transaction() {
    let memory = memory();
    let pages = Pages::default();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut caps = limits();
    caps.max_writes = 1;
    let mut txn = ExecutionTransaction::begin(parent, [1; 32], &pages, &memory, caps).unwrap();
    txn.write(StateNamespaceV1::Business, b"debit", Some([1; 32]))
        .unwrap();
    assert!(matches!(
        txn.write(
            StateNamespaceV1::Effects,
            b"required-receipt",
            Some([2; 32])
        ),
        Err(ExecutionError::ResourceLimited(_))
    ));
    let guard = txn.guard();
    assert!(
        txn.seal(&guard, &live, 0).is_err(),
        "must not publish debit without required receipt after a quota failure"
    );
}

#[test]
fn missing_parent_page_and_memory_failure_also_prevent_partial_seal() {
    let memory = memory();
    let mut pages = Pages::default();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut first =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, limits()).unwrap();
    first
        .write(StateNamespaceV1::Business, b"balance", Some([1; 32]))
        .unwrap();
    let guard = first.guard();
    let committed = first.seal(&guard, &live, 0).unwrap();
    pages.persist(&committed, &memory);
    let parent = ExecutionParent {
        commit: [101; 32],
        roots: committed.record.after.clone(),
        authority: parent.authority,
    };
    let live = parent.fence().unwrap();
    pages.nodes.remove(
        &parent
            .roots
            .state_root(StateNamespaceV1::Business)
            .node_root,
    );
    let mut txn = ExecutionTransaction::begin(parent, [2; 32], &pages, &memory, limits()).unwrap();
    txn.write(StateNamespaceV1::Effects, b"receipt", Some([2; 32]))
        .unwrap();
    assert!(matches!(
        txn.read(StateNamespaceV1::Business, b"balance"),
        Err(ExecutionError::Tree(_))
    ));
    let reads = pages.reads.get();
    assert!(matches!(
        txn.read(StateNamespaceV1::Effects, b"receipt"),
        Err(ExecutionError::Aborted)
    ));
    assert_eq!(pages.reads.get(), reads);
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));

    let tiny = MemoryBudget::new(64 * 1024 + 64 + 513, 0).unwrap();
    let original = empty_parent();
    let live = original.fence().unwrap();
    let source = Pages::default();
    let mut txn = ExecutionTransaction::begin(original, [3; 32], &source, &tiny, limits()).unwrap();
    txn.write(StateNamespaceV1::Business, b"a", Some([1; 32]))
        .unwrap();
    assert!(matches!(
        txn.append_fact(fact(ExecutionFactKindV1::Result, 3)),
        Err(ExecutionError::Memory(_))
    ));
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));
    drop(guard);
    assert_eq!(tiny.used(), 0);
}

#[test]
fn caught_reader_panic_cannot_reenable_partial_seal() {
    struct Panics;
    impl MerkleNodeReader for Panics {
        type Node = Vec<u8>;
        fn read_node(&self, _digest: &str, _max_bytes: usize) -> Result<Option<Vec<u8>>, String> {
            panic!("injected reader panic")
        }
    }
    let memory = memory();
    let pages = Pages::default();
    let parent = empty_parent();
    let live = parent.fence().unwrap();
    let mut initial =
        ExecutionTransaction::begin(parent.clone(), [1; 32], &pages, &memory, limits()).unwrap();
    initial
        .write(StateNamespaceV1::Business, b"balance", Some([1; 32]))
        .unwrap();
    let guard = initial.guard();
    let first = initial.seal(&guard, &live, 0).unwrap();
    let parent = ExecutionParent {
        commit: [101; 32],
        roots: first.record.after.clone(),
        authority: parent.authority,
    };
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [2; 32], &Panics, &memory, limits()).unwrap();
    txn.write(StateNamespaceV1::Effects, b"receipt", Some([2; 32]))
        .unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        txn.read(StateNamespaceV1::Business, b"balance")
    }));
    assert!(panic.is_err());
    let guard = txn.guard();
    assert!(
        txn.seal(&guard, &live, 0).is_err(),
        "caught panic must leave transaction aborted"
    );
}
