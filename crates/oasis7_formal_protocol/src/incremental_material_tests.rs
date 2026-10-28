use super::*;
use crate::controlled_authority::formal::{
    self,
    test_fixtures::{
        self as fixture,
        incremental::{Setup, core_objects, setup},
    },
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::{cell::Cell, collections::BTreeMap};

struct Objects {
    memory: MemoryBudget,
    values: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    reads: usize,
    _metadata: MemoryReservation,
}
impl OriginalObjectReader for Objects {
    type Error = std::io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.memory
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> std::io::Result<Arc<ReservedValue<Vec<u8>>>> {
        self.reads += 1;
        let bytes = self.values.get(id).ok_or(std::io::ErrorKind::NotFound)?;
        if bytes.len() > max || !self.memory.same_pool(memory) {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        Ok(bytes.clone())
    }
}
struct Page(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct Pages {
    memory: MemoryBudget,
    values: BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>,
    reads: Cell<usize>,
    class: MemoryClass,
    _metadata: MemoryReservation,
}
impl MerkleNodeReader for Pages {
    type Node = Page;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Page>, String> {
        self.reads.set(self.reads.get() + 1);
        self.values
            .get(id)
            .map(|bytes| {
                if bytes.len() <= max {
                    Ok(Page(bytes.clone()))
                } else {
                    Err("fixture page bound".into())
                }
            })
            .transpose()
    }
}
fn sources(s: &Setup, history: &PreparedHistoryV1) -> (Objects, Pages) {
    let (blobs, catalog) = core_objects(s, history);
    let metadata = s.memory.reserve(64 * 1024, MemoryClass::Ordinary).unwrap();
    let mut objects = Objects {
        memory: s.memory.clone(),
        values: BTreeMap::new(),
        reads: 0,
        _metadata: metadata,
    };
    for bytes in blobs.iter() {
        objects
            .values
            .insert(*blake3::hash(bytes).as_bytes(), bytes.clone());
    }
    let metadata = s.memory.reserve(512 * 1024, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages {
        memory: s.memory.clone(),
        values: BTreeMap::new(),
        reads: Cell::new(0),
        class: MemoryClass::Ordinary,
        _metadata: metadata,
    };
    for node in &catalog.new_nodes {
        let charge = s
            .memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages.values.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
    (objects, pages)
}
fn candidate(s: &Setup, record: formal::CanonicalRecordV1) -> VerifiedCandidateContextV1 {
    let signed = formal::SignedCanonicalCandidateV1 {
        writer_signature_hex: fixture::sign(
            formal::canonical_candidate_signing_bytes(&record).unwrap(),
            4,
        ),
        record,
    };
    formal::verify_formal_candidate(
        &signed,
        None,
        &s.authority,
        None,
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap()
}
fn request<'a>(s: &'a Setup, candidate: &'a VerifiedCandidateContextV1) -> CoreRequest<'a> {
    CoreRequest {
        candidate,
        original_authority: &s.authority,
        parent: None,
        max_pending_node_bytes: 4 << 20,
    }
}
fn budget() -> CoreBudget {
    CoreBudget {
        objects_remaining: 32,
        original_bytes_remaining: 1024 * 1024,
        new_pages_remaining: 4096,
        merkle_reads_remaining: 10000,
    }
}

#[test]
fn reads_exact_original_core_reproduces_history_and_retains_charges() {
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let candidate = candidate(&s, s.commit.record().clone());
    let before = s.memory.used();
    let checked = verify_commit_core(
        &request(&s, &candidate),
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    assert_eq!(checked.body(), &s.history.body);
    assert_eq!(checked.roots(), &s.roots);
    assert_eq!(checked.history(), &s.history.prefix);
    assert_eq!(
        checked.catalog().entry.commit,
        s.history.body.digest().unwrap()
    );
    assert!(checked.transition().is_none());
    assert_eq!(checked.facts().facts, s.history.facts.facts);
    assert_eq!(objects.reads, 8); // config, roots, body, prefix, facts, segment, CP, catalog
    assert!(s.memory.used() > before);
    drop(checked);
    assert_eq!(s.memory.used(), before);
}

#[test]
fn missing_or_substituted_actual_originals_and_new_pages_never_pass() {
    for mode in 0..4 {
        let s = setup();
        let (mut objects, mut pages) = sources(&s, &s.history);
        let body = s.history.body.digest().unwrap();
        match mode {
            0 => {
                objects.values.remove(&body);
            }
            1 => {
                let mut bytes = s.history.body.canonical_bytes().unwrap();
                bytes[0] ^= 1;
                let charge = s
                    .memory
                    .reserve(bytes.capacity() + 64, MemoryClass::Ordinary)
                    .unwrap();
                objects.values.insert(body, Arc::new(charge.retain(bytes)));
            }
            2 => {
                pages.values.clear();
            }
            _ => {
                let first = pages.values.first_entry().unwrap();
                let charge = s.memory.reserve(1, MemoryClass::Ordinary).unwrap();
                first
                    .into_mut()
                    .clone_from(&Arc::new(charge.retain(vec![0])));
            }
        }
        let candidate = candidate(&s, s.commit.record().clone());
        assert!(
            verify_commit_core(
                &request(&s, &candidate),
                &mut objects,
                &pages,
                &s.memory,
                &mut budget(),
                || false
            )
            .is_err()
        );
    }
}

#[test]
fn signed_body_with_wrong_checkpoint_or_lost_original_input_is_rejected() {
    for mode in 0..2 {
        let s = setup();
        let scope = s.config.scope().unwrap();
        let history = if mode == 0 {
            None
        } else {
            Some(
                HeaderPreparation {
                    config: &s.config,
                    catalog: &CatalogRootV1::empty(scope).unwrap(),
                    roots: &s.roots,
                    parent: None,
                    parent_history: None,
                    transition: None,
                    genesis_facts: &[],
                    operation: s.history.body.operation,
                    authority: s.history.body.authority,
                    execution_environment: s.config.execution_environment,
                    checkpoint: true,
                }
                .prepare(&s.memory)
                .unwrap(),
            )
        };
        let material = history.as_deref().unwrap_or(&s.history);
        let (mut objects, pages) = sources(&s, material);
        let mut record = s.commit.record().clone();
        let mut body = material.body.clone();
        if mode == 0 {
            let mut cp = material.checkpoint.clone().unwrap();
            cp.format_profile = [99; 32];
            let bytes = cp.canonical_bytes().unwrap();
            let id = *blake3::hash(&bytes).as_bytes();
            let charge = s
                .memory
                .reserve(bytes.capacity() + 64, MemoryClass::Ordinary)
                .unwrap();
            objects.values.insert(id, Arc::new(charge.retain(bytes)));
            body.checkpoint = Some(id);
        }
        let bytes = body.canonical_bytes().unwrap();
        let id = *blake3::hash(&bytes).as_bytes();
        let charge = s
            .memory
            .reserve(bytes.capacity() + 64, MemoryClass::Ordinary)
            .unwrap();
        objects.values.insert(id, Arc::new(charge.retain(bytes)));
        record
            .artifact_roots
            .insert(ArtifactRole::IncrementalCommitBody, hex::encode(id));
        record.artifact_roots.insert(
            ArtifactRole::IncrementalHistoryPrefix,
            hex::encode(material.prefix.digest().unwrap()),
        );
        let candidate = candidate(&s, record);
        let error = verify_commit_core(
            &request(&s, &candidate),
            &mut objects,
            &pages,
            &s.memory,
            &mut budget(),
            || false,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            CoreError::Invalid("candidate core does not reproduce producer")
                | CoreError::Invalid("original input absent from facts")
        ));
    }
}

#[test]
fn allowances_cancel_pool_and_recovery_class_are_explicit() {
    for mode in 0..6 {
        let s = setup();
        let (mut objects, mut pages) = sources(&s, &s.history);
        let candidate = candidate(&s, s.commit.record().clone());
        let mut quota = budget();
        if mode == 0 {
            quota.objects_remaining = 0;
        }
        if mode == 1 {
            quota.original_bytes_remaining = 1;
        }
        if mode == 2 {
            quota.new_pages_remaining = 0;
        }
        if mode == 4 {
            pages.class = MemoryClass::Recovery;
        }
        if mode == 5 {
            objects.memory = MemoryBudget::new(32 << 20, 0).unwrap();
        }
        let before = s.memory.used();
        assert!(
            verify_commit_core(
                &request(&s, &candidate),
                &mut objects,
                &pages,
                &s.memory,
                &mut quota,
                || mode == 3
            )
            .is_err()
        );
        assert_eq!(s.memory.used(), before);
        if mode >= 3 {
            assert_eq!(objects.reads, 0);
        }
    }
}

#[test]
fn exact_core_allowances_pass_and_one_less_cannot_finish() {
    let s = setup();
    let (objects, pages) = sources(&s, &s.history);
    let input = role::<std::io::Error>(s.commit.record(), ArtifactRole::Input).unwrap();
    let bytes: u64 = objects
        .values
        .iter()
        .filter(|(id, _)| **id != input)
        .map(|(_, bytes)| bytes.len() as u64)
        .sum();
    let count = objects.values.len() as u64 - 1;
    let page_count = pages.values.len() as u64;
    for mode in 0..5 {
        let (mut objects, pages) = sources(&s, &s.history);
        let candidate = candidate(&s, s.commit.record().clone());
        let mut quota = CoreBudget {
            objects_remaining: count - u64::from(mode == 1),
            original_bytes_remaining: bytes - u64::from(mode == 2),
            new_pages_remaining: page_count - u64::from(mode == 3),
            merkle_reads_remaining: page_count - u64::from(mode == 4),
        };
        let outcome = verify_commit_core(
            &request(&s, &candidate),
            &mut objects,
            &pages,
            &s.memory,
            &mut quota,
            || false,
        );
        if mode == 0 {
            outcome.unwrap();
            assert_eq!(
                (
                    quota.objects_remaining,
                    quota.original_bytes_remaining,
                    quota.new_pages_remaining
                ),
                (0, 0, 0)
            );
        } else {
            assert!(outcome.is_err());
        }
    }
}

fn execution_sources(
    s: &Setup,
    value: [u8; 32],
) -> (
    Objects,
    Pages,
    VerifiedCandidateContextV1,
    ReservedValue<crate::incremental_execution::PreparedTransition>,
) {
    use crate::incremental_execution::{
        ExecutionFactKindV1, ExecutionFactRefV1, ExecutionTransaction, TransactionLimits,
    };
    let _operation_workspace = s.memory.reserve(128 * 1024, MemoryClass::Ordinary).unwrap();
    let (mut objects, mut pages) = sources(s, &s.history);
    let authority = incremental::authority_binding(&s.authority, &s.memory).unwrap();
    let parent = ExecutionParent {
        commit: s.history.body.digest().unwrap(),
        roots: s.roots.clone(),
        authority,
    };
    let fence = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(
        parent,
        [60; 32],
        &pages,
        &s.memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 8,
            max_facts: 8,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    txn.write(
        crate::incremental_state::StateNamespaceV1::Business,
        b"counter",
        Some(value),
    )
    .unwrap();
    txn.write(
        crate::incremental_state::StateNamespaceV1::Business,
        b"counter-two",
        Some(value),
    )
    .unwrap();
    for namespace in [
        crate::incremental_state::StateNamespaceV1::Allocator,
        crate::incremental_state::StateNamespaceV1::Scheduler,
        crate::incremental_state::StateNamespaceV1::Authorization,
        crate::incremental_state::StateNamespaceV1::Effects,
        crate::incremental_state::StateNamespaceV1::Artifacts,
    ] {
        txn.write(namespace, b"control", Some(value)).unwrap();
    }
    txn.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: [61; 32],
    })
    .unwrap();
    let guard = txn.guard();
    let transition = txn.seal(&guard, &fence, 0).unwrap();
    for node in &transition.new_nodes {
        let charge = s
            .memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages.values.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
    let (_, parent_catalog) = core_objects(s, &s.history);
    let history = HeaderPreparation {
        config: &s.config,
        catalog: &parent_catalog.after,
        roots: &transition.record.after,
        parent: Some(&s.history.body),
        parent_history: Some(&s.history.prefix),
        transition: Some(&transition.record),
        genesis_facts: &[],
        operation: [60; 32],
        authority,
        execution_environment: s.config.execution_environment,
        checkpoint: false,
    }
    .prepare(&s.memory)
    .unwrap();
    let mut record = s.commit.record().clone();
    record.kind = formal::FormalRecordKindV1::Execution;
    record.position = 2;
    record.parent_record_id = s.commit.head().record_id;
    record.request_digest = fixture::h(60);
    record.payload_digest = fixture::h(61);
    record.runtime_manifest_digest = fixture::h(62);
    let frozen = fixture::incremental::frozen(s);
    record.frozen_reservation_id = Some(frozen.reservation.body.reservation_id().unwrap());
    record.after_state_root = hex::encode(transition.record.after.digest().unwrap());
    for (key, value) in [
        (ArtifactRole::Input, record.payload_digest.clone()),
        (
            ArtifactRole::ExecutionManifest,
            record.runtime_manifest_digest.clone(),
        ),
        (
            ArtifactRole::IncrementalRootSet,
            record.after_state_root.clone(),
        ),
        (
            ArtifactRole::IncrementalCommitBody,
            hex::encode(history.body.digest().unwrap()),
        ),
        (
            ArtifactRole::IncrementalHistoryPrefix,
            hex::encode(history.prefix.digest().unwrap()),
        ),
    ] {
        record.artifact_roots.insert(key, value);
    }
    let signed = formal::SignedCanonicalCandidateV1 {
        writer_signature_hex: fixture::sign(
            formal::canonical_candidate_signing_bytes(&record).unwrap(),
            4,
        ),
        record,
    };
    let candidate = formal::verify_formal_candidate(
        &signed,
        Some(&frozen),
        &s.authority,
        Some(&s.commit),
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let mut insert = |bytes: Vec<u8>| {
        let id = *blake3::hash(&bytes).as_bytes();
        let charge = s
            .memory
            .reserve(bytes.capacity() + 128, MemoryClass::Ordinary)
            .unwrap();
        objects.values.insert(id, Arc::new(charge.retain(bytes)));
    };
    insert(transition.record.after.canonical_bytes().unwrap());
    insert(history.body.canonical_bytes().unwrap());
    insert(history.prefix.canonical_bytes().unwrap());
    insert(history.segment.canonical_bytes().unwrap());
    insert(
        history
            .facts
            .canonical_bytes(&s.memory)
            .unwrap()
            .as_slice()
            .to_vec(),
    );
    insert(
        history
            .transition_bytes
            .as_ref()
            .unwrap()
            .as_slice()
            .to_vec(),
    );
    let delta = history
        .body
        .catalog_before
        .extend(&history.body, &pages, &s.memory, 4 << 20)
        .unwrap();
    insert(delta.after.canonical_bytes().unwrap());
    for node in &delta.new_nodes {
        let charge = s
            .memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap();
        pages.values.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
    (objects, pages, candidate, transition)
}

#[test]
fn nonempty_transition_is_rebuilt_from_parent_and_target_state_pages_are_required() {
    let s = setup();
    let (mut objects, mut pages, candidate, transition) = execution_sources(&s, [9; 32]);
    let request = CoreRequest {
        candidate: &candidate,
        original_authority: &s.authority,
        parent: Some(&s.commit),
        max_pending_node_bytes: 4 << 20,
    };
    let checked = verify_commit_core(
        &request,
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    assert_eq!(checked.transition().unwrap(), &transition.record);
    assert_eq!(checked.roots(), &transition.record.after);
    drop(checked);
    pages.values.remove(&transition.new_nodes[0].digest);
    assert!(
        verify_commit_core(
            &request,
            &mut objects,
            &pages,
            &s.memory,
            &mut budget(),
            || false
        )
        .is_err()
    );
}

#[test]
fn cancellation_during_page_walk_is_typed_and_keeps_spent_work() {
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let candidate = candidate(&s, s.commit.record().clone());
    let mut quota = budget();
    let before = s.memory.used();
    let mut calls = 0;
    let result = verify_commit_core(
        &request(&s, &candidate),
        &mut objects,
        &pages,
        &s.memory,
        &mut quota,
        || {
            calls += 1;
            calls >= 10
        },
    );
    assert!(matches!(result, Err(CoreError::Cancelled)));
    assert!(pages.reads.get() > 0);
    assert_eq!(
        10000 - quota.merkle_reads_remaining,
        pages.reads.get() as u64
    );
    assert_eq!(s.memory.used(), before);
}

#[test]
fn caught_source_unwind_does_not_refund_merkle_allowance() {
    struct PanicPages<'a>(&'a Pages);
    impl MerkleNodeReader for PanicPages<'_> {
        type Node = Page;
        fn allocation_budget(&self) -> Option<&MemoryBudget> {
            Some(&self.0.memory)
        }
        fn allocation_class(&self) -> Option<MemoryClass> {
            Some(MemoryClass::Ordinary)
        }
        fn read_node(&self, _: &str, _: usize) -> Result<Option<Page>, String> {
            panic!("injected source unwind");
        }
    }
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let candidate = candidate(&s, s.commit.record().clone());
    let mut quota = budget();
    let before = s.memory.used();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        verify_commit_core(
            &request(&s, &candidate),
            &mut objects,
            &PanicPages(&pages),
            &s.memory,
            &mut quota,
            || false,
        )
    }));
    assert!(result.is_err());
    assert_eq!(quota.merkle_reads_remaining, 9999);
    assert_eq!(s.memory.used(), before);
}

#[path = "incremental_fact_material_tests.rs"]
mod fact_originals;

#[path = "incremental_written_values_tests.rs"]
mod written_value_tests;
