use super::*;

#[test]
fn candidate_event_observation_requires_exact_candidate_and_actual_acceptance_to_promote() {
    let s = setup();
    let (head, core, originals) = observed_current_events(&s, 2);
    let signed = candidate(&s, head.record().clone());
    let req = CandidateEventFactRequestV1 {
        candidate: &signed,
        core: &core,
        originals: &originals,
        ordinal: 2,
    };
    let checked = check_candidate_event_fact_v1(&req, &s.memory, || false).unwrap();
    assert_eq!(checked.candidate_record_id(), core.candidate_record_id());
    assert_eq!(checked.body_id(), core.body().digest().unwrap());
    assert_eq!(checked.roots_id(), core.roots().digest().unwrap());
    assert_eq!(checked.location().ordinal, 2);
    assert_eq!(checked.original_bytes(), 25);
    let accepted = promote_candidate_event_fact_v1(&checked, &head, &s.memory, || false).unwrap();
    assert_eq!(accepted.head_record_id(), checked.candidate_record_id());
    assert_eq!(accepted.location(), checked.location());
    assert_eq!(accepted.original_bytes(), checked.original_bytes());
    assert!(matches!(
        check_candidate_event_fact_v1(
            &CandidateEventFactRequestV1 { ordinal: 0, ..req },
            &s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        check_candidate_event_fact_v1(
            &CandidateEventFactRequestV1 { ordinal: 3, ..req },
            &s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    // Same body and all Event metadata, but a genuinely different signed record.
    let mut other_record = head.record().clone();
    other_record.projection_root = hex::encode([91; 32]);
    let other = candidate(&s, other_record.clone());
    assert!(matches!(
        check_candidate_event_fact_v1(
            &CandidateEventFactRequestV1 {
                candidate: &other,
                ..req
            },
            &s.memory,
            || false
        ),
        Err(CoreError::Invalid("current event core record identity"))
    ));
    let proof = fixture::proof(
        other_record,
        &s.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let other_head = formal::verify_formal_qualified(&proof, &s.authority, None, None).unwrap();
    assert!(matches!(
        promote_candidate_event_fact_v1(&checked, &other_head, &s.memory, || false),
        Err(CoreError::Invalid(_))
    ));
    let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    assert!(matches!(
        check_candidate_event_fact_v1(&req, &foreign, || false),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        promote_candidate_event_fact_v1(&checked, &head, &foreign, || false),
        Err(CoreError::Invalid(_))
    ));
    for final_boundary in [false, true] {
        let used = s.memory.used();
        let mut calls = 0;
        assert!(matches!(
            check_candidate_event_fact_v1(&req, &s.memory, || {
                calls += 1;
                calls == if final_boundary { 2 } else { 1 }
            }),
            Err(CoreError::Cancelled)
        ));
        let mut calls = 0;
        assert!(matches!(
            promote_candidate_event_fact_v1(&checked, &head, &s.memory, || {
                calls += 1;
                calls == if final_boundary { 2 } else { 1 }
            }),
            Err(CoreError::Cancelled)
        ));
        assert_eq!(s.memory.used(), used);
    }
    let limit = s
        .memory
        .reserve(usize::MAX, MemoryClass::Ordinary)
        .err()
        .unwrap()
        .admission_limit;
    let held = s
        .memory
        .reserve(limit - s.memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        check_candidate_event_fact_v1(&req, &s.memory, || false),
        Err(CoreError::Execution(ExecutionError::Memory(_)))
    ));
    assert!(matches!(
        promote_candidate_event_fact_v1(&checked, &head, &s.memory, || false),
        Err(CoreError::Execution(ExecutionError::Memory(_)))
    ));
    drop(held);
}

#[test]
fn current_observations_preserve_duplicate_event_ordinals() {
    let s = setup();
    let (head, core, originals) = observed_current_events(&s, 2);
    assert_eq!(originals.counts(), [1, 0, 2]);
    for ordinal in [1, 2] {
        let proof = bind_checked_current_event_fact_v1(
            &CurrentEventFactRequestV1 {
                head: &head,
                core: &core,
                originals: &originals,
                ordinal,
            },
            &s.memory,
            || false,
        )
        .unwrap();
        assert_eq!(proof.location().ordinal, ordinal);
        assert_eq!(
            proof.location().original,
            *blake3::hash(b"original Event fact bytes").as_bytes()
        );
        assert_eq!(proof.original_bytes(), 25);
    }
}

#[test]
fn current_event_binds_observed_original_after_sources_are_dropped() {
    let s = setup();
    let (head, core, originals) = observed_current_event(&s);
    assert_eq!(originals.counts(), [1, 0, 1]);
    // All object/page sessions were destroyed inside the helper. The only
    // source of the per-occurrence length is actual prior stream verification.
    let baseline = s.memory.used();
    let proof = bind_checked_current_event_fact_v1(
        &CurrentEventFactRequestV1 {
            head: &head,
            core: &core,
            originals: &originals,
            ordinal: 1,
        },
        &s.memory,
        || false,
    )
    .unwrap();
    assert_eq!(proof.original_bytes(), 25);
    assert_eq!(
        proof.location(),
        EventFactLocationV1 {
            height: 0,
            ordinal: 1,
            original: *blake3::hash(b"original Event fact bytes").as_bytes(),
        }
    );
    assert_eq!(proof.roots_id(), core.roots().digest().unwrap());
    assert_eq!(proof.head_body_id(), core.body().digest().unwrap());
    assert_eq!(proof.event_body_id(), proof.head_body_id());
    assert_eq!(s.memory.used(), baseline + proof.reserved_bytes());
    drop(proof);
    assert_eq!(s.memory.used(), baseline);
}

#[test]
fn current_event_rejects_input_missing_observation_foreign_pool_and_cancellation() {
    let s = setup();
    let (head, core, originals) = observed_current_event(&s);
    for ordinal in [0, 2, u64::MAX] {
        assert!(matches!(
            bind_checked_current_event_fact_v1(
                &CurrentEventFactRequestV1 {
                    head: &head,
                    core: &core,
                    originals: &originals,
                    ordinal,
                },
                &s.memory,
                || false
            ),
            Err(CoreError::Invalid(_))
        ));
    }
    let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    assert!(matches!(
        bind_checked_current_event_fact_v1(
            &CurrentEventFactRequestV1 {
                head: &head,
                core: &core,
                originals: &originals,
                ordinal: 1,
            },
            &foreign,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
    for stop in [1, 2] {
        let calls = Cell::new(0);
        assert!(matches!(
            bind_checked_current_event_fact_v1(
                &CurrentEventFactRequestV1 {
                    head: &head,
                    core: &core,
                    originals: &originals,
                    ordinal: 1,
                },
                &s.memory,
                || {
                    calls.set(calls.get() + 1);
                    calls.get() >= stop
                }
            ),
            Err(CoreError::Cancelled)
        ));
    }
    // This head is cryptographically valid but names the original different
    // body/facts, so an observation from the new Event cannot attach to it.
    assert!(matches!(
        bind_checked_current_event_fact_v1(
            &CurrentEventFactRequestV1 {
                head: &s.commit,
                core: &core,
                originals: &originals,
                ordinal: 1,
            },
            &s.memory,
            || false
        ),
        Err(CoreError::Invalid(_))
    ));
}

#[test]
fn current_observation_binding_streams_large_events_under_an_eight_mib_pool() {
    for length in [(32 << 20) + 17, (64 << 20) + 17] {
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let s = fixture::incremental::setup_with_memory(memory);
        let charge = s
            .memory
            .reserve(
                crate::incremental_value::VALUE_CHUNK_BYTES,
                MemoryClass::Ordinary,
            )
            .unwrap();
        let chunk = charge.retain(vec![b'x'; crate::incremental_value::VALUE_CHUNK_BYTES]);
        let mut hash = blake3::Hasher::new();
        let mut remaining = length;
        while remaining != 0 {
            let n = remaining.min(chunk.len());
            hash.update(&chunk[..n]);
            remaining -= n;
        }
        let id = *hash.finalize().as_bytes();
        let history = event_history(&s, id);
        let head = accepted(&s, &history);
        let candidate = candidate(&s, head.record().clone());
        let (mut objects, pages) = sources(&s, &history);
        let peak = Cell::new(s.memory.used());
        let full_reads = Cell::new(0);
        let mut stream = PatternStream {
            objects: &mut objects,
            original: id,
            length,
            chunk,
            peak: &peak,
            full_reads: &full_reads,
        };
        let core = verify_commit_core(
            &request(&s, &candidate),
            &mut stream,
            &pages,
            &s.memory,
            &mut budget(),
            || false,
        )
        .unwrap();
        let observed = verify_fact_originals(
            &core,
            &mut stream,
            &s.memory,
            &mut FactBudget {
                facts_remaining: 2,
                original_bytes_remaining: length as u64 + (1 << 20),
                max_fact_bytes: length as u64,
            },
            || false,
        )
        .unwrap();
        drop(stream);
        drop(objects);
        drop(pages);
        let proof = bind_checked_current_event_fact_v1(
            &CurrentEventFactRequestV1 {
                head: &head,
                core: &core,
                originals: &observed,
                ordinal: 1,
            },
            &s.memory,
            || false,
        )
        .unwrap();
        assert_eq!(proof.original_bytes(), length as u64);
        assert_eq!(proof.location().original, id);
        assert_eq!(full_reads.get(), 0);
        eprintln!(
            "current raw Event bytes={length} pool_limit={} stream_observed_reserved_peak={}",
            8 << 20,
            peak.get()
        );
    }
}
