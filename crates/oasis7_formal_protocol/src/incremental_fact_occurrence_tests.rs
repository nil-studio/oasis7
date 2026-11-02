//! Raw kind binding; these opaque bytes intentionally assert no Result semantics.
use super::*;

const RESULT: &[u8] = b"opaque formal Result original";
struct Current {
    s: Setup,
    history: ReservedValue<PreparedHistoryV1>,
    objects: Objects,
    pages: Pages,
    proof: formal::FormalQualifiedCommitProofV1,
    candidate: VerifiedCandidateContextV1,
    core: ReservedValue<CheckedCommitCore>,
    originals: ReservedValue<CheckedFactOriginals>,
}
fn current() -> Current {
    let s = setup();
    let _scratch = s.memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let id = *blake3::hash(RESULT).as_bytes();
    let mut facts = s.history.facts.facts.clone();
    facts.extend([
        ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id,
        },
        // Identical bytes do not erase the authenticated occurrence kind.
        ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id,
        },
    ]);
    let history = HeaderPreparation {
        config: &s.config,
        catalog: &CatalogRootV1::empty(s.config.scope().unwrap()).unwrap(),
        roots: &s.roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &facts,
        operation: s.history.body.operation,
        authority: s.history.body.authority,
        execution_environment: s.config.execution_environment,
        checkpoint: true,
    }
    .prepare(&s.memory)
    .unwrap();
    let mut record = s.commit.record().clone();
    record.artifact_roots.insert(
        ArtifactRole::IncrementalCommitBody,
        hex::encode(history.body.digest().unwrap()),
    );
    record.artifact_roots.insert(
        ArtifactRole::IncrementalHistoryPrefix,
        hex::encode(history.prefix.digest().unwrap()),
    );
    let proof = fixture::proof(
        record,
        &s.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let candidate = candidate(&s, proof.decision.record.clone());
    let (mut objects, pages) = sources(&s, &history);
    let charge = s
        .memory
        .reserve(RESULT.len(), MemoryClass::Ordinary)
        .unwrap();
    objects
        .values
        .insert(id, Arc::new(charge.retain(RESULT.to_vec())));
    let core = verify_commit_core(
        &request(&s, &candidate),
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    let stopped = Cell::new(false);
    let originals = verify_fact_originals(
        &core,
        &mut EventStream {
            objects: &mut objects,
            mode: 0,
            final_cancel: &stopped,
        },
        &s.memory,
        &mut FactBudget {
            facts_remaining: 3,
            original_bytes_remaining: 4096,
            max_fact_bytes: 4096,
        },
        || false,
    )
    .unwrap();
    Current {
        s,
        history,
        objects,
        pages,
        proof,
        candidate,
        core,
        originals,
    }
}

#[test]
fn generic_fact_current_kind_and_candidate_acceptance_are_exact() {
    let f = current();
    let req = CandidateFactRequestV1 {
        kind: ExecutionFactKindV1::Result,
        candidate: &f.candidate,
        core: &f.core,
        originals: &f.originals,
        ordinal: 1,
    };
    let checked = check_candidate_fact_v1(&req, &f.s.memory, || false).unwrap();
    assert_eq!(checked.kind(), ExecutionFactKindV1::Result);
    assert_eq!(
        checked.location().original,
        *blake3::hash(RESULT).as_bytes()
    );
    assert_eq!(checked.original_bytes(), RESULT.len() as u64);
    for (kind, ordinal) in [
        (ExecutionFactKindV1::Event, 1),
        (ExecutionFactKindV1::Input, 1),
        (ExecutionFactKindV1::Result, 0),
        (ExecutionFactKindV1::Result, 3),
    ] {
        assert!(matches!(
            check_candidate_fact_v1(
                &CandidateFactRequestV1 {
                    kind,
                    ordinal,
                    ..req
                },
                &f.s.memory,
                || false
            ),
            Err(CoreError::Invalid(_))
        ));
    }
    assert!(matches!(
        check_candidate_event_fact_v1(
            &CandidateEventFactRequestV1 {
                candidate: &f.candidate,
                core: &f.core,
                originals: &f.originals,
                ordinal: 1,
            },
            &f.s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    let event = check_candidate_event_fact_v1(
        &CandidateEventFactRequestV1 {
            candidate: &f.candidate,
            core: &f.core,
            originals: &f.originals,
            ordinal: 2,
        },
        &f.s.memory,
        || false,
    )
    .unwrap();
    assert_eq!(event.location().original, checked.location().original);
    let input = check_candidate_fact_v1(
        &CandidateFactRequestV1 {
            kind: ExecutionFactKindV1::Input,
            ordinal: 0,
            ..req
        },
        &f.s.memory,
        || false,
    )
    .unwrap();
    assert_eq!(input.kind(), ExecutionFactKindV1::Input);
    assert!(matches!(
        promote_candidate_fact_v1(&checked, &f.s.commit, &f.s.memory, || false),
        Err(CoreError::Invalid(_))
    ));
    // No new accepted head is obtained until after the candidate check.
    let head = formal::verify_formal_qualified(&f.proof, &f.s.authority, None, None).unwrap();
    let accepted = promote_candidate_fact_v1(&checked, &head, &f.s.memory, || false).unwrap();
    let current = bind_checked_current_fact_v1(
        &CurrentFactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            head: &head,
            core: &f.core,
            originals: &f.originals,
            ordinal: 1,
        },
        &f.s.memory,
        || false,
    )
    .unwrap();
    assert_eq!(current.kind(), accepted.kind());
    assert_eq!(current.location(), accepted.location());
    assert_eq!(current.head_record_id(), checked.candidate_record_id());
    assert_eq!(current.fact_body_id(), f.history.body.digest().unwrap());
    let mut other_record = f.proof.decision.record.clone();
    other_record.projection_root = hex::encode([91; 32]);
    let other = candidate(&f.s, other_record);
    assert!(matches!(
        check_candidate_fact_v1(
            &CandidateFactRequestV1 {
                candidate: &other,
                ..req
            },
            &f.s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    assert!(matches!(
        check_candidate_fact_v1(&req, &foreign, || false),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        promote_candidate_fact_v1(&checked, &head, &foreign, || false),
        Err(CoreError::Invalid(_))
    ));
    for boundary in [1, 2] {
        let used = f.s.memory.used();
        let mut calls = 0;
        assert!(matches!(
            check_candidate_fact_v1(&req, &f.s.memory, || {
                calls += 1;
                calls == boundary
            }),
            Err(CoreError::Cancelled)
        ));
        let mut calls = 0;
        assert!(matches!(
            promote_candidate_fact_v1(&checked, &head, &f.s.memory, || {
                calls += 1;
                calls == boundary
            }),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(f.s.memory.used(), used);
    }
    let limit =
        f.s.memory
            .reserve(usize::MAX, MemoryClass::Ordinary)
            .unwrap_err()
            .admission_limit;
    let _held =
        f.s.memory
            .reserve(limit - f.s.memory.used(), MemoryClass::Ordinary)
            .unwrap();
    assert!(matches!(
        check_candidate_fact_v1(&req, &f.s.memory, || false),
        Err(CoreError::Execution(ExecutionError::Memory(_)))
    ));
}

#[test]
fn generic_result_history_rejects_wrong_kind_and_zero_original_budget() {
    let mut f = current();
    let head = formal::verify_formal_qualified(&f.proof, &f.s.authority, None, None).unwrap();
    let location = FactLocationV1 {
        height: 0,
        ordinal: 1,
        original: *blake3::hash(RESULT).as_bytes(),
    };
    let stopped = Cell::new(false);
    let mut source = EventStream {
        objects: &mut f.objects,
        mode: 0,
        final_cancel: &stopped,
    };
    let req = FactRequestV1 {
        kind: ExecutionFactKindV1::Result,
        head: &head,
        roots: &f.s.roots,
        location,
    };
    let result = verify_fact_occurrence_v1(
        &req,
        &mut FactReadContextV1 {
            originals: &mut source,
            catalog_pages: &f.pages,
            memory: &f.s.memory,
            class: MemoryClass::Ordinary,
            core_budget: &mut budget(),
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    )
    .unwrap();
    assert_eq!(result.kind(), ExecutionFactKindV1::Result);
    assert_eq!(result.original_bytes(), RESULT.len() as u64);
    for kind in [ExecutionFactKindV1::Input, ExecutionFactKindV1::Event] {
        assert!(matches!(
            verify_fact_occurrence_v1(
                &FactRequestV1 { kind, ..req },
                &mut FactReadContextV1 {
                    originals: &mut source,
                    catalog_pages: &f.pages,
                    memory: &f.s.memory,
                    class: MemoryClass::Ordinary,
                    core_budget: &mut budget(),
                    fact_budget: &mut event_budget(),
                    cancelled: || false,
                }
            ),
            Err(CoreError::Invalid(_))
        ));
    }
    assert!(matches!(
        verify_event_fact_occurrence_v1(
            &EventFactRequestV1 {
                head: &head,
                roots: &f.s.roots,
                location
            },
            &mut EventFactReadContextV1 {
                originals: &mut source,
                catalog_pages: &f.pages,
                memory: &f.s.memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut budget(),
                fact_budget: &mut event_budget(),
                cancelled: || false,
            }
        ),
        Err(CoreError::Invalid(_))
    ));
    let mut zero = event_budget();
    zero.facts_remaining = 0;
    assert!(matches!(
        verify_fact_occurrence_v1(
            &req,
            &mut FactReadContextV1 {
                originals: &mut source,
                catalog_pages: &f.pages,
                memory: &f.s.memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut budget(),
                fact_budget: &mut zero,
                cancelled: || false,
            }
        ),
        Err(CoreError::ResourceLimited(_))
    ));
    source.mode = 3;
    assert!(matches!(
        verify_fact_occurrence_v1(
            &req,
            &mut FactReadContextV1 {
                originals: &mut source,
                catalog_pages: &f.pages,
                memory: &f.s.memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut budget(),
                fact_budget: &mut event_budget(),
                cancelled: || stopped.get(),
            }
        ),
        Err(CoreError::Cancelled)
    ));
}

fn normal_extension(
    f: &mut Current,
) -> (
    VerifiedFormalCommitV1,
    ReservedValue<CheckedCommitCore>,
    ReservedValue<RootSetV1>,
) {
    use crate::incremental_execution::{ExecutionTransaction, TransactionLimits};
    let _scratch =
        f.s.memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .unwrap();
    let catalog = f
        .history
        .body
        .catalog_before
        .extend(&f.history.body, &f.pages, &f.s.memory, 4 << 20)
        .unwrap();
    for node in &catalog.new_nodes {
        let charge =
            f.s.memory
                .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                .unwrap();
        f.pages.values.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
    let parent = ExecutionParent {
        roots: f.s.roots.clone(),
        commit: f.history.body.digest().unwrap(),
        authority: f.history.body.authority,
    };
    let fence = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent,
        [60; 32],
        &f.pages,
        &f.s.memory,
        TransactionLimits {
            max_reads: 4,
            max_writes: 4,
            max_facts: 1,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let input = *blake3::hash(b"next original input").as_bytes();
    tx.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input,
    })
    .unwrap();
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &fence, 1).unwrap();
    let next = HeaderPreparation {
        config: &f.s.config,
        catalog: &catalog.after,
        roots: &sealed.record.after,
        parent: Some(&f.history.body),
        parent_history: Some(&f.history.prefix),
        transition: Some(&sealed.record),
        genesis_facts: &[],
        operation: [60; 32],
        authority: f.history.body.authority,
        execution_environment: f.s.config.execution_environment,
        checkpoint: false,
    }
    .prepare(&f.s.memory)
    .unwrap();
    let mut frozen = fixture::incremental::frozen(&f.s);
    frozen.reservation.body.runtime_manifest_digest =
        f.s.commit.record().runtime_manifest_digest.clone();
    frozen.reservation.body.payload_digest = hex::encode(input);
    frozen.reservation.signature_hex = fixture::sign(
        formal::frozen_signing_bytes(&frozen.reservation.body).unwrap(),
        4,
    );
    let reservation = frozen.reservation.body.reservation_id().unwrap();
    for (receipt, key) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
        receipt.body.reservation_id = reservation.clone();
        receipt.signature_hex = fixture::sign(
            formal::frozen_receipt_signing_bytes(&receipt.body).unwrap(),
            key,
        );
    }
    let mut record = fixture::execution(&f.s.authority, &frozen);
    record.schema_version = incremental::IWS_RECORD_SCHEMA;
    record.after_state_root = hex::encode(sealed.record.after.digest().unwrap());
    record.artifact_roots = f.s.commit.record().artifact_roots.clone();
    for (role, id) in [
        (ArtifactRole::Input, input),
        (
            ArtifactRole::IncrementalRootSet,
            sealed.record.after.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalCommitBody,
            next.body.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalHistoryPrefix,
            next.prefix.digest().unwrap(),
        ),
    ] {
        record.artifact_roots.insert(role, hex::encode(id));
    }
    let proof = fixture::proof(
        record,
        &f.s.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let signed = formal::SignedCanonicalCandidateV1 {
        record: proof.decision.record.clone(),
        writer_signature_hex: proof.decision.writer_signature_hex.clone(),
    };
    let candidate = formal::verify_formal_candidate(
        &signed,
        proof.decision.frozen.as_ref(),
        &f.s.authority,
        Some(&f.s.commit),
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let after_catalog = next
        .body
        .catalog_before
        .extend(&next.body, &f.pages, &f.s.memory, 4 << 20)
        .unwrap();
    for node in &after_catalog.new_nodes {
        let charge =
            f.s.memory
                .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                .unwrap();
        f.pages.values.insert(
            node.digest.clone(),
            Arc::new(charge.retain(node.bytes.clone())),
        );
    }
    for bytes in [
        next.body.canonical_bytes().unwrap(),
        next.prefix.canonical_bytes().unwrap(),
        next.segment.canonical_bytes().unwrap(),
        next.facts.canonical_bytes(&f.s.memory).unwrap().to_vec(),
        sealed.record.after.canonical_bytes().unwrap(),
        next.transition_bytes.as_ref().unwrap().to_vec(),
        after_catalog.after.canonical_bytes().unwrap(),
        b"next original input".to_vec(),
    ] {
        let id = *blake3::hash(&bytes).as_bytes();
        let charge =
            f.s.memory
                .reserve(bytes.capacity(), MemoryClass::Ordinary)
                .unwrap();
        f.objects.values.insert(id, Arc::new(charge.retain(bytes)));
    }
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &f.s.authority,
            parent: Some(&f.s.commit),
            max_pending_node_bytes: 4 << 20,
        },
        &mut f.objects,
        &f.pages,
        &f.s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    let head =
        formal::verify_formal_qualified(&proof, &f.s.authority, Some(&f.s.commit), None).unwrap();
    let roots =
        f.s.memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .unwrap()
            .retain(sealed.record.after.clone());
    (head, core, roots)
}

#[test]
fn generic_result_catalog_seek_and_direct_prefix_inheritance_preserve_kind() {
    let mut f = current();
    let checked = check_candidate_fact_v1(
        &CandidateFactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            candidate: &f.candidate,
            core: &f.core,
            originals: &f.originals,
            ordinal: 1,
        },
        &f.s.memory,
        || false,
    )
    .unwrap();
    f.s.commit = formal::verify_formal_qualified(&f.proof, &f.s.authority, None, None).unwrap();
    let old = promote_candidate_fact_v1(&checked, &f.s.commit, &f.s.memory, || false).unwrap();
    let (head, core, roots) = normal_extension(&mut f);
    let extension = verify_fact_history_extension_v1(
        &FactHistoryExtensionRequestV1 {
            parent: &f.s.commit,
            head: &head,
            core: &core,
        },
        &f.s.memory,
        || false,
    )
    .unwrap();
    let reads = (f.objects.reads, f.pages.reads.get());
    let inherited = inherit_fact_occurrence_v1(&old, &extension, &f.s.memory, || false).unwrap();
    assert_eq!((f.objects.reads, f.pages.reads.get()), reads);
    assert_eq!(inherited.kind(), ExecutionFactKindV1::Result);
    assert_eq!(inherited.location(), old.location());
    assert_eq!(inherited.original_bytes(), old.original_bytes());
    assert_eq!(inherited.fact_body_id(), old.fact_body_id());
    assert_eq!(inherited.roots_id(), roots.digest().unwrap());
    assert_ne!(inherited.head_record_id(), old.head_record_id());
    assert!(matches!(
        inherit_fact_occurrence_v1(&inherited, &extension, &f.s.memory, || false),
        Err(CoreError::Invalid(_))
    ));
    let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    assert!(matches!(
        inherit_fact_occurrence_v1(&old, &extension, &foreign, || false),
        Err(CoreError::Invalid(_))
    ));
    for boundary in [1, 2] {
        let mut calls = 0;
        assert!(matches!(
            inherit_fact_occurrence_v1(&old, &extension, &f.s.memory, || {
                calls += 1;
                calls == boundary
            }),
            Err(CoreError::Cancelled)
        ));
    }
    let stopped = Cell::new(false);
    let mut stream = EventStream {
        objects: &mut f.objects,
        mode: 0,
        final_cancel: &stopped,
    };
    let before = f.pages.reads.get();
    let cold = verify_fact_occurrence_v1(
        &FactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            head: &head,
            roots: &roots,
            location: old.location(),
        },
        &mut FactReadContextV1 {
            originals: &mut stream,
            catalog_pages: &f.pages,
            memory: &f.s.memory,
            class: MemoryClass::Ordinary,
            core_budget: &mut budget(),
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    )
    .unwrap();
    assert!(f.pages.reads.get() > before && f.pages.reads.get() - before <= 257);
    assert_eq!(cold.kind(), inherited.kind());
    assert_eq!(cold.location(), inherited.location());
    assert_eq!(cold.fact_body_id(), inherited.fact_body_id());
    assert_eq!(cold.head_record_id(), inherited.head_record_id());
}

#[test]
fn raw_recovery_fact_and_legacy_event_cannot_implicitly_become_ordinary() {
    let mut f = current();
    let _fixture =
        f.s.memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .unwrap();
    f.s.commit = formal::verify_formal_qualified(&f.proof, &f.s.authority, None, None).unwrap();
    let (head, core, _) = normal_extension(&mut f);
    let extension = verify_fact_history_extension_v1(
        &FactHistoryExtensionRequestV1 {
            parent: &f.s.commit,
            head: &head,
            core: &core,
        },
        &f.s.memory,
        || false,
    )
    .unwrap();
    // The real raw verifier must succeed exclusively from Recovery credits.
    let hold =
        f.s.memory
            .reserve((28 << 20) - f.s.memory.used(), MemoryClass::Ordinary)
            .unwrap();
    assert!(f.s.memory.reserve(1, MemoryClass::Ordinary).is_err());
    f.objects.class = MemoryClass::Recovery;
    f.pages.class = MemoryClass::Recovery;
    let stopped = Cell::new(false);
    let mut source = EventStream {
        objects: &mut f.objects,
        mode: 0,
        final_cancel: &stopped,
    };
    let location = FactLocationV1 {
        height: 0,
        ordinal: 1,
        original: *blake3::hash(RESULT).as_bytes(),
    };
    let generic = verify_fact_occurrence_v1(
        &FactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            head: &f.s.commit,
            roots: &f.s.roots,
            location,
        },
        &mut FactReadContextV1 {
            originals: &mut source,
            catalog_pages: &f.pages,
            memory: &f.s.memory,
            class: MemoryClass::Recovery,
            core_budget: &mut budget(),
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    )
    .unwrap();
    let legacy = verify_event_fact_occurrence_v1(
        &EventFactRequestV1 {
            head: &f.s.commit,
            roots: &f.s.roots,
            location: FactLocationV1 {
                ordinal: 2,
                ..location
            },
        },
        &mut FactReadContextV1 {
            originals: &mut source,
            catalog_pages: &f.pages,
            memory: &f.s.memory,
            class: MemoryClass::Recovery,
            core_budget: &mut budget(),
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    )
    .unwrap();
    assert_eq!(generic.allocation_class(), MemoryClass::Recovery);
    assert_eq!(legacy.allocation_class(), MemoryClass::Recovery);
    let memory_before = f.s.memory.used();
    for result in [
        readmit_fact_occurrence_ordinary_v1(&generic, &f.s.commit, &f.s.roots, &f.s.memory, || {
            false
        })
        .map(|_| ()),
        readmit_event_fact_occurrence_ordinary_v1(
            &legacy,
            &f.s.commit,
            &f.s.roots,
            &f.s.memory,
            || false,
        )
        .map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(CoreError::Execution(ExecutionError::Memory(_)))),
            "{result:?}"
        );
    }
    assert_eq!(f.s.memory.used(), memory_before);
    // Free ordinary space: failure must be the class boundary, not a lucky
    // allocation failure. Both paths are observed before the final assertion.
    drop(hold);
    let generic_result = inherit_fact_occurrence_v1(&generic, &extension, &f.s.memory, || false);
    let legacy_result =
        inherit_event_fact_occurrence_v1(&legacy, &extension, &f.s.memory, || false);
    assert!(
        matches!(generic_result, Err(CoreError::Invalid(_)))
            && matches!(legacy_result, Err(CoreError::Invalid(_))),
        "Recovery inheritance: generic={generic_result:?}, legacy={legacy_result:?}"
    );
    let reads = (f.objects.reads, f.pages.reads.get());
    // Distinct genuinely accepted record, but the same RootSet and body roles:
    // this must hit the exact record-ID check, not only head/root validation.
    let mut other_record = f.s.commit.record().clone();
    other_record.request_digest = hex::encode([83; 32]);
    let other_proof = fixture::proof(
        other_record,
        &f.s.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let other_head =
        formal::verify_formal_qualified(&other_proof, &f.s.authority, None, None).unwrap();
    for role in [
        ArtifactRole::IncrementalRootSet,
        ArtifactRole::IncrementalCommitBody,
    ] {
        assert_eq!(
            other_head.record().artifact_roots.get(&role),
            f.s.commit.record().artifact_roots.get(&role)
        );
    }
    for result in [
        readmit_fact_occurrence_ordinary_v1(&generic, &other_head, &f.s.roots, &f.s.memory, || {
            false
        })
        .map(|_| ()),
        readmit_event_fact_occurrence_ordinary_v1(
            &legacy,
            &other_head,
            &f.s.roots,
            &f.s.memory,
            || false,
        )
        .map(|_| ()),
    ] {
        assert!(
            matches!(
                result,
                Err(CoreError::Invalid(
                    "fact readmission exact accepted binding"
                ))
            ),
            "{result:?}"
        );
    }
    let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    for result in [
        readmit_fact_occurrence_ordinary_v1(&generic, &f.s.commit, &f.s.roots, &foreign, || false)
            .map(|_| ()),
        readmit_event_fact_occurrence_ordinary_v1(
            &legacy,
            &f.s.commit,
            &f.s.roots,
            &foreign,
            || false,
        )
        .map(|_| ()),
        readmit_fact_occurrence_ordinary_v1(&generic, &head, &f.s.roots, &f.s.memory, || false)
            .map(|_| ()),
        readmit_event_fact_occurrence_ordinary_v1(&legacy, &head, &f.s.roots, &f.s.memory, || {
            false
        })
        .map(|_| ()),
    ] {
        assert!(matches!(result, Err(CoreError::Invalid(_))), "{result:?}");
    }
    let mut wrong_roots = f.s.roots.clone();
    wrong_roots.simulation_tick += 1;
    assert!(matches!(
        readmit_fact_occurrence_ordinary_v1(
            &generic,
            &f.s.commit,
            &wrong_roots,
            &f.s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        readmit_event_fact_occurrence_ordinary_v1(
            &legacy,
            &f.s.commit,
            &wrong_roots,
            &f.s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    let baseline = f.s.memory.used();
    for boundary in [1, 2] {
        let mut calls = 0;
        assert!(matches!(
            readmit_fact_occurrence_ordinary_v1(
                &generic,
                &f.s.commit,
                &f.s.roots,
                &f.s.memory,
                || {
                    calls += 1;
                    calls == boundary
                }
            ),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(calls, boundary);
        assert_eq!(f.s.memory.used(), baseline);
        let mut calls = 0;
        assert!(matches!(
            readmit_event_fact_occurrence_ordinary_v1(
                &legacy,
                &f.s.commit,
                &f.s.roots,
                &f.s.memory,
                || {
                    calls += 1;
                    calls == boundary
                }
            ),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(calls, boundary);
        assert_eq!(f.s.memory.used(), baseline);
    }
    let admitted =
        readmit_fact_occurrence_ordinary_v1(&generic, &f.s.commit, &f.s.roots, &f.s.memory, || {
            false
        })
        .unwrap();
    let event_admitted = readmit_event_fact_occurrence_ordinary_v1(
        &legacy,
        &f.s.commit,
        &f.s.roots,
        &f.s.memory,
        || false,
    )
    .unwrap();
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(event_admitted.allocation_class(), MemoryClass::Ordinary);
    assert!(matches!(
        readmit_fact_occurrence_ordinary_v1(
            &admitted,
            &f.s.commit,
            &f.s.roots,
            &f.s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        readmit_event_fact_occurrence_ordinary_v1(
            &event_admitted,
            &f.s.commit,
            &f.s.roots,
            &f.s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    assert_eq!(admitted.head_record_id(), generic.head_record_id());
    assert_eq!(admitted.roots_id(), generic.roots_id());
    assert_eq!(admitted.head_body_id(), generic.head_body_id());
    assert_eq!(admitted.fact_body_id(), generic.fact_body_id());
    assert_eq!(admitted.kind(), generic.kind());
    assert_eq!(admitted.location(), generic.location());
    assert_eq!(admitted.original_bytes(), generic.original_bytes());
    assert_eq!(event_admitted.head_record_id(), legacy.head_record_id());
    assert_eq!(event_admitted.roots_id(), legacy.roots_id());
    assert_eq!(event_admitted.head_body_id(), legacy.head_body_id());
    assert_eq!(event_admitted.event_body_id(), legacy.event_body_id());
    assert_eq!(event_admitted.location(), legacy.location());
    assert_eq!(event_admitted.original_bytes(), legacy.original_bytes());
    let admitted_bytes = admitted.reserved_bytes() + event_admitted.reserved_bytes();
    assert_eq!(f.s.memory.used(), baseline + admitted_bytes);
    let old_bytes = generic.reserved_bytes() + legacy.reserved_bytes();
    drop(generic);
    drop(legacy);
    assert_eq!(f.s.memory.used(), baseline + admitted_bytes - old_bytes);
    assert_eq!(
        (f.objects.reads, f.pages.reads.get()),
        reads,
        "readmission did source IO"
    );
    let inherited =
        inherit_fact_occurrence_v1(&admitted, &extension, &f.s.memory, || false).unwrap();
    let event_inherited =
        inherit_event_fact_occurrence_v1(&event_admitted, &extension, &f.s.memory, || false)
            .unwrap();
    assert_eq!(inherited.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(event_inherited.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(inherited.kind(), ExecutionFactKindV1::Result);
    assert_eq!(inherited.location(), location);
    assert_eq!(event_inherited.location().ordinal, 2);
    assert_eq!((f.objects.reads, f.pages.reads.get()), reads);
}
