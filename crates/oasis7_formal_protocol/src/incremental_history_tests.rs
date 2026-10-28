use super::*;
use crate::incremental_execution::{ExecutionParent, ExecutionTransaction, TransactionLimits};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Default)]
struct Pages {
    values: BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>,
    reads: Cell<usize>,
}
impl MerkleNodeReader for Pages {
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, String> {
        self.reads.set(self.reads.get() + 1);
        let Some(bytes) = self.values.get(digest) else {
            return Ok(None);
        };
        if bytes.len() > max_bytes {
            return Err("source bound".into());
        }
        Ok(Some(bytes.as_slice().to_vec()))
    }
}
impl Pages {
    fn insert(&mut self, id: String, bytes: &[u8], memory: &MemoryBudget) {
        let charge = memory
            .reserve(bytes.len() + 512, MemoryClass::Ordinary)
            .unwrap();
        self.values
            .insert(id, Arc::new(charge.retain(bytes.to_vec())));
    }
    fn catalog(&mut self, delta: &CatalogDeltaV1, memory: &MemoryBudget) {
        for node in &delta.new_nodes {
            self.insert(node.digest.clone(), &node.bytes, memory);
        }
    }
}
fn memory() -> MemoryBudget {
    MemoryBudget::new(256 * 1024 * 1024, 2 * 1024 * 1024).unwrap()
}
fn config() -> GenesisConfigV1 {
    GenesisConfigV1 {
        world_id: "history-fixture".into(),
        format_profile: [1; 32],
        initial_content: [2; 32],
        activation_config: [3; 32],
        execution_environment: [4; 32],
    }
}
fn authority() -> AuthorityBinding {
    AuthorityBinding {
        token: [5; 32],
        epoch: 1,
    }
}
fn genesis(
    config: &GenesisConfigV1,
    checkpoint: bool,
    memory: &MemoryBudget,
) -> ReservedValue<PreparedHistoryV1> {
    let scope = config.scope().unwrap();
    HeaderPreparation {
        config,
        catalog: &CatalogRootV1::empty(scope.clone()).unwrap(),
        roots: &RootSetV1::empty(scope.world_id, scope.genesis_config).unwrap(),
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &[],
        operation: [6; 32],
        authority: authority(),
        execution_environment: config.execution_environment,
        checkpoint,
    }
    .prepare(memory)
    .unwrap()
}

#[test]
fn genesis_identity_and_current_catalog_have_no_self_hash_cycle() {
    let memory = memory();
    let config = config();
    let value = genesis(&config, true, &memory);
    assert_ne!(config.digest().unwrap(), value.body.digest().unwrap());
    assert_eq!(value.body.scope.genesis_config, config.digest().unwrap());
    assert_eq!(value.body.catalog_before.count, 0);
    assert_eq!(value.prefix.previous, None);
    assert_eq!(value.segment.transition, None);
    assert_eq!(
        value.checkpoint.as_ref().unwrap().history,
        value.prefix.digest().unwrap()
    );
    let mut pages = Pages::default();
    let delta = value
        .body
        .catalog_before
        .extend(&value.body, &pages, &memory, 2 * 1024 * 1024)
        .unwrap();
    assert_eq!(delta.after.count, 1);
    assert_eq!(delta.entry.commit, value.body.digest().unwrap());
    delta
        .verify(
            &value.body.catalog_before,
            &value.body,
            &pages,
            &memory,
            2 * 1024 * 1024,
        )
        .unwrap();
    pages.catalog(&delta, &memory);
    for query in [
        CatalogQueryV1::Height(0),
        CatalogQueryV1::Commit(value.body.digest().unwrap()),
    ] {
        let proof = delta.after.prove(query, &pages, &memory).unwrap();
        assert_eq!(
            *proof.verify(&delta.after, query, &memory).unwrap(),
            Some(delta.entry.clone())
        );
    }
    assert_eq!(
        *delta
            .after
            .prove(CatalogQueryV1::Height(1), &pages, &memory)
            .unwrap()
            .verify(&delta.after, CatalogQueryV1::Height(1), &memory)
            .unwrap(),
        None
    );
}

#[test]
fn all_header_codecs_roundtrip_original_bytes_and_reject_other_domains() {
    let memory = memory();
    let config = config();
    let value = genesis(&config, true, &memory);
    let bytes = config.canonical_bytes().unwrap();
    assert_eq!(
        GenesisConfigV1::from_canonical_bytes(&bytes).unwrap(),
        config
    );
    let bytes = value.body.canonical_bytes().unwrap();
    assert_eq!(
        CommitBodyV1::from_canonical_bytes(&bytes).unwrap(),
        value.body
    );
    assert!(HistoryPrefixV1::from_canonical_bytes(&bytes).is_err());
    let bytes = value.prefix.canonical_bytes().unwrap();
    assert_eq!(
        HistoryPrefixV1::from_canonical_bytes(&bytes).unwrap(),
        value.prefix
    );
    let bytes = value.segment.canonical_bytes().unwrap();
    assert_eq!(
        HistorySegmentV1::from_canonical_bytes(&bytes).unwrap(),
        value.segment
    );
    let cp = value.checkpoint.as_ref().unwrap();
    assert_eq!(
        CheckpointDescriptorV1::from_canonical_bytes(&cp.canonical_bytes().unwrap()).unwrap(),
        *cp
    );
    let facts = value.facts.canonical_bytes(&memory).unwrap();
    assert_eq!(
        *FactSetV1::from_canonical_bytes(&facts, &memory).unwrap(),
        value.facts
    );
    let mut trailing = value.body.canonical_bytes().unwrap();
    trailing.push(0);
    assert!(CommitBodyV1::from_canonical_bytes(&trailing).is_err());
    let mut wrong_version = value.prefix.canonical_bytes().unwrap();
    wrong_version[0] ^= 1;
    assert!(HistoryPrefixV1::from_canonical_bytes(&wrong_version).is_err());
}

#[test]
fn real_transition_is_bound_to_original_facts_roots_parent_and_history() {
    let memory = memory();
    let config = config();
    let initial = genesis(&config, false, &memory);
    let mut pages = Pages::default();
    let delta = initial
        .body
        .catalog_before
        .extend(&initial.body, &pages, &memory, 2 * 1024 * 1024)
        .unwrap();
    pages.catalog(&delta, &memory);
    let roots = RootSetV1::empty(config.world_id.clone(), config.digest().unwrap()).unwrap();
    let parent = ExecutionParent {
        commit: initial.body.digest().unwrap(),
        roots,
        authority: authority(),
    };
    let live = parent.fence().unwrap();
    let limits = TransactionLimits {
        max_reads: 10,
        max_writes: 10,
        max_facts: 10,
        max_pending_node_bytes: 2 * 1024 * 1024,
    };
    let mut txn = ExecutionTransaction::begin(parent, [7; 32], &pages, &memory, limits).unwrap();
    txn.write(
        crate::incremental_state::StateNamespaceV1::Allocator,
        b"next",
        Some([8; 32]),
    )
    .unwrap();
    txn.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: [9; 32],
    })
    .unwrap();
    let guard = txn.guard();
    let transition = txn.seal(&guard, &live, 0).unwrap();
    let mut preparation = HeaderPreparation {
        config: &config,
        catalog: &delta.after,
        roots: &transition.record.after,
        parent: Some(&initial.body),
        parent_history: Some(&initial.prefix),
        transition: Some(&transition.record),
        genesis_facts: &[],
        operation: [7; 32],
        authority: authority(),
        execution_environment: [4; 32],
        checkpoint: true,
    };
    let value = preparation.prepare(&memory).unwrap();
    assert_eq!(value.body.height, 1);
    assert_eq!(value.prefix.count, 2);
    assert_eq!(
        value.prefix.previous,
        Some(initial.prefix.digest().unwrap())
    );
    assert_eq!(value.facts.facts, transition.record.facts);
    assert_eq!(value.segment.transition, value.body.transition);
    assert_eq!(
        value.body.transition,
        Some(*blake3::hash(value.transition_bytes.as_ref().unwrap()).as_bytes())
    );
    preparation.operation = [99; 32];
    assert!(preparation.prepare(&memory).is_err());
    preparation.operation = [7; 32];
    preparation.authority.epoch += 1;
    assert!(preparation.prepare(&memory).is_err());
}

#[test]
fn old_height_and_commit_seek_work_stays_fixed_while_catalog_grows() {
    let memory = memory();
    let config = config();
    let mut current = genesis(&config, true, &memory);
    let mut pages = Pages::default();
    let first_commit = current.body.digest().unwrap();
    let mut roots = RootSetV1::empty(config.world_id.clone(), config.digest().unwrap()).unwrap();
    let mut previous_entry = None;
    let mut catalog = current.body.catalog_before.clone();
    let mut commits = Vec::new();
    for height in 0..128u64 {
        if height > 0 {
            let operation = *blake3::hash(&height.to_be_bytes()).as_bytes();
            let parent = ExecutionParent {
                commit: current.body.digest().unwrap(),
                roots: roots.clone(),
                authority: authority(),
            };
            let live = parent.fence().unwrap();
            let txn = ExecutionTransaction::begin(
                parent,
                operation,
                &pages,
                &memory,
                TransactionLimits {
                    max_reads: 1,
                    max_writes: 1,
                    max_facts: 1,
                    max_pending_node_bytes: 2 * 1024 * 1024,
                },
            )
            .unwrap();
            let guard = txn.guard();
            let transition = txn.seal(&guard, &live, 0).unwrap();
            roots = transition.record.after.clone();
            let next = HeaderPreparation {
                config: &config,
                catalog: &catalog,
                roots: &roots,
                parent: Some(&current.body),
                parent_history: Some(&current.prefix),
                transition: Some(&transition.record),
                genesis_facts: &[],
                operation,
                authority: authority(),
                execution_environment: config.execution_environment,
                checkpoint: height % 11 == 0,
            }
            .prepare(&memory)
            .unwrap();
            current = next;
        }
        let delta = catalog
            .extend(&current.body, &pages, &memory, 2 * 1024 * 1024)
            .unwrap();
        commits.push(current.body.digest().unwrap());
        let expected_latest = height / 11 * 11;
        assert_eq!(
            delta.entry.latest_checkpoint.unwrap().height,
            expected_latest
        );
        if height % 11 != 0 {
            assert_eq!(delta.entry.latest_checkpoint, previous_entry);
        }
        previous_entry = delta.entry.latest_checkpoint;
        // Stored candidate bytes do not authorize reuse as a parent; only
        // the old authenticated tree decides the final-new-node write set.
        pages.catalog(&delta, &memory);
        delta
            .verify(&catalog, &current.body, &pages, &memory, 2 * 1024 * 1024)
            .unwrap();
        catalog = delta.after.clone();
        if [0, 7, 31, 127].contains(&height) {
            for (query, expected_reads) in [
                (CatalogQueryV1::Height(0), 257),
                (CatalogQueryV1::Commit(first_commit), 514),
            ] {
                pages.reads.set(0);
                let proof = catalog.prove(query, &pages, &memory).unwrap();
                assert_eq!(pages.reads.get(), expected_reads);
                let entry = proof.verify(&catalog, query, &memory).unwrap();
                assert_eq!(entry.as_ref().unwrap().height, 0);
                assert_eq!(entry.as_ref().unwrap().commit, first_commit);
                let encoded = proof.canonical_bytes(&memory).unwrap();
                let decoded =
                    CatalogProofV1::from_canonical_bytes(&encoded, &catalog, query, &memory)
                        .unwrap();
                assert_eq!(*decoded, *proof);
                assert_eq!(*decoded.verify(&catalog, query, &memory).unwrap(), *entry);
            }
        }
    }
    for (height, commit) in commits.into_iter().enumerate() {
        let entry = catalog
            .lookup(CatalogQueryV1::Height(height as u64), &pages)
            .unwrap()
            .unwrap();
        assert_eq!(entry.commit, commit);
        assert_eq!(
            entry.latest_checkpoint.unwrap().height,
            height as u64 / 11 * 11
        );
    }
    let query = CatalogQueryV1::Commit([255; 32]);
    let proof = catalog.prove(query, &pages, &memory).unwrap();
    assert_eq!(*proof.verify(&catalog, query, &memory).unwrap(), None);
}

#[test]
fn incomplete_catalog_and_wrong_proof_queries_cannot_claim_absence() {
    let memory = memory();
    let config = config();
    let value = genesis(&config, false, &memory);
    let mut pages = Pages::default();
    let delta = value
        .body
        .catalog_before
        .extend(&value.body, &pages, &memory, 2 * 1024 * 1024)
        .unwrap();
    pages.catalog(&delta, &memory);
    let query = CatalogQueryV1::Height(0);
    let proof = delta.after.prove(query, &pages, &memory).unwrap();
    assert!(
        proof
            .verify(&delta.after, CatalogQueryV1::Height(1), &memory)
            .is_err()
    );
    for mutation in 0..4 {
        let mut bad = (*proof).clone();
        match mutation {
            0 => {
                bad.nodes.pop();
            }
            1 => bad.nodes.reverse(),
            2 => bad.nodes.push(bad.nodes[0].clone()),
            _ => bad.nodes[0].bytes[0] ^= 1,
        }
        assert!(bad.verify(&delta.after, query, &memory).is_err());
    }
    let encoded = proof.canonical_bytes(&memory).unwrap();
    assert!(
        CatalogProofV1::from_canonical_bytes(
            &encoded,
            &delta.after,
            CatalogQueryV1::Height(1),
            &memory
        )
        .is_err()
    );
    let mut wrong_world = delta.after.clone();
    wrong_world.scope.world_id.push('x');
    assert!(proof.verify(&wrong_world, query, &memory).is_err());
    for mutation in 0..4 {
        let mut bad = (*delta).clone();
        match mutation {
            0 => bad.after.count += 1,
            1 => bad.new_nodes.reverse(),
            2 => {
                bad.new_nodes.pop();
            }
            _ => bad.entry.commit[0] ^= 1,
        }
        assert!(
            bad.verify(
                &value.body.catalog_before,
                &value.body,
                &Pages::default(),
                &memory,
                2 * 1024 * 1024
            )
            .is_err()
        );
    }
    pages.values.remove(&delta.after.node_root);
    assert!(delta.after.lookup(query, &pages).is_err());
    assert!(
        delta
            .after
            .lookup(CatalogQueryV1::Commit(value.body.digest().unwrap()), &pages)
            .is_err()
    );
    assert!(delta.after.prove(query, &pages, &memory).is_err());
}

#[test]
fn header_and_catalog_profile_vectors_and_counter_overflow_are_fixed() {
    let memory = memory();
    let config = config();
    let value = genesis(&config, true, &memory);
    let mut payload = (config.world_id.len() as u16).to_be_bytes().to_vec();
    payload.extend_from_slice(config.world_id.as_bytes());
    for byte in [1, 2, 3, 4] {
        payload.extend_from_slice(&[byte; 32]);
    }
    let mut expected = b"oasis7.iws.genesis-config.v1\0".to_vec();
    expected.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    expected.extend(payload);
    assert_eq!(config.canonical_bytes().unwrap(), expected);
    let delta = value
        .body
        .catalog_before
        .extend(&value.body, &Pages::default(), &memory, 2 * 1024 * 1024)
        .unwrap();
    assert_eq!(
        hex::encode(config.digest().unwrap()),
        "4ccf2e453c933b6abb62b7add6e90bf98a20af7e75539fef1a89bacffc935249"
    );
    assert_eq!(
        hex::encode(value.segment.digest().unwrap()),
        "89e48fbd562b6f01aa417823b85632803bbe92d58a7a7c3ce8f33adf3be62e9e"
    );
    assert_eq!(
        hex::encode(value.prefix.digest().unwrap()),
        "348ebd8f22585289e7970d02aeaa8308d60d8fa19df2dbef800db0cb5dc9354d"
    );
    assert_eq!(
        hex::encode(value.checkpoint.as_ref().unwrap().digest().unwrap()),
        "3ddd7845ddfee9935c413c51377eaf07373d1ca299d5e71d0bbfed6675fc33cc"
    );
    assert_eq!(
        hex::encode(value.body.digest().unwrap()),
        "c672a04101378cf60d107ee84cf4ea6dcf82de7aeb455aab188525e8004c392b"
    );
    assert_eq!(
        hex::encode(delta.after.digest().unwrap()),
        "68599ddb41ae91549c8b4b20483f1f0daae4133a20336431f9ac019114cda058"
    );
    let mut exhausted = delta.after.clone();
    exhausted.count = u64::MAX;
    let mut body = value.body.clone();
    body.height = u64::MAX;
    body.parent_commit = exhausted.last_commit;
    body.transition = Some([8; 32]);
    body.catalog_before = exhausted.clone();
    assert!(matches!(
        exhausted.extend(&body, &Pages::default(), &memory, 2 * 1024 * 1024),
        Err(ExecutionError::ResourceLimited(_))
    ));
    let tiny = MemoryBudget::new(1, 0).unwrap();
    assert!(matches!(
        value
            .body
            .catalog_before
            .extend(&value.body, &Pages::default(), &tiny, 2 * 1024 * 1024),
        Err(ExecutionError::Memory(_))
    ));
}
