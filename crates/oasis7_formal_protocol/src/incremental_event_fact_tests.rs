use super::*;
use crate::incremental_execution::ExecutionFactRefV1;
use std::io::Write;

struct EventStream<'a> {
    objects: &'a mut Objects,
    mode: u8,
    final_cancel: &'a Cell<bool>,
}
impl OriginalObjectReader for EventStream<'_> {
    type Error = std::io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.objects.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        self.objects.class
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> std::io::Result<Arc<ReservedValue<Vec<u8>>>> {
        self.objects.read_original(id, max, memory)
    }
}
impl OriginalObjectStreamReader for EventStream<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        _: u64,
        _: &MemoryBudget,
        output: &mut dyn Write,
    ) -> std::io::Result<u64> {
        let bytes = self
            .objects
            .values
            .get(id)
            .ok_or(std::io::ErrorKind::NotFound)?;
        match self.mode {
            1 => {
                output.write_all(b"substitution")?;
                Ok(12)
            }
            2 => {
                output.write_all(bytes)?;
                Err(std::io::ErrorKind::UnexpectedEof.into())
            }
            3 => {
                output.write_all(bytes)?;
                self.final_cancel.set(true);
                Ok(bytes.len() as u64)
            }
            4 => {
                let _ = output.write_all(bytes);
                Ok(bytes.len() as u64)
            }
            _ => {
                output.write_all(bytes)?;
                Ok(bytes.len() as u64)
            }
        }
    }
}
fn event_history(s: &Setup, id: [u8; 32]) -> ReservedValue<PreparedHistoryV1> {
    let _fixture = s.memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let mut facts = s.history.facts.facts.clone();
    facts.push(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Event,
        id,
    });
    HeaderPreparation {
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
    .unwrap()
}
fn accepted(s: &Setup, history: &PreparedHistoryV1) -> VerifiedFormalCommitV1 {
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
    formal::verify_formal_qualified(&proof, &s.authority, None, None).unwrap()
}
fn event_sources(s: &Setup, history: &PreparedHistoryV1, id: [u8; 32]) -> (Objects, Pages) {
    let (mut objects, pages) = sources(s, history);
    let charge = s.memory.reserve(64, MemoryClass::Ordinary).unwrap();
    objects.values.insert(
        id,
        Arc::new(charge.retain(b"original Event fact bytes".to_vec())),
    );
    (objects, pages)
}
fn event_budget() -> FactBudget {
    FactBudget {
        facts_remaining: 1,
        original_bytes_remaining: 4096,
        max_fact_bytes: 4096,
    }
}
#[test]
fn event_fact_proof_requires_exact_accepted_head_root_and_ordered_occurrence() {
    let s = setup();
    let id = *blake3::hash(b"original Event fact bytes").as_bytes();
    let history = event_history(&s, id);
    let head = accepted(&s, &history);
    let (mut objects, pages) = event_sources(&s, &history, id);
    let stopped = Cell::new(false);
    let mut stream = EventStream {
        objects: &mut objects,
        mode: 0,
        final_cancel: &stopped,
    };
    let location = EventFactLocationV1 {
        height: 0,
        ordinal: 1,
        original: id,
    };
    let mut quota = budget();
    let mut facts = event_budget();
    let result = verify_event_fact_occurrence_v1(
        &EventFactRequestV1 {
            head: &head,
            roots: &s.roots,
            location,
        },
        &mut EventFactReadContextV1 {
            originals: &mut stream,
            catalog_pages: &pages,
            memory: &s.memory,
            class: MemoryClass::Ordinary,
            core_budget: &mut quota,
            fact_budget: &mut facts,
            cancelled: || false,
        },
    )
    .unwrap();
    assert_eq!(result.roots_id(), s.roots.digest().unwrap());
    assert_eq!(result.head_body_id(), history.body.digest().unwrap());
    assert_eq!(result.event_body_id(), history.body.digest().unwrap());
    assert_eq!(result.location(), location);
    assert_eq!(result.original_bytes(), 25);
    assert_eq!(facts.facts_remaining, 0);
    assert_eq!(pages.reads.get(), 0); // current height requires no ancestor walk
    for bad in [
        EventFactLocationV1 {
            ordinal: 0,
            ..location
        },
        EventFactLocationV1 {
            original: [3; 32],
            ..location
        },
        EventFactLocationV1 {
            height: 1,
            ..location
        },
    ] {
        let result = verify_event_fact_occurrence_v1(
            &EventFactRequestV1 {
                head: &head,
                roots: &s.roots,
                location: bad,
            },
            &mut EventFactReadContextV1 {
                originals: &mut stream,
                catalog_pages: &pages,
                memory: &s.memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut budget(),
                fact_budget: &mut event_budget(),
                cancelled: || false,
            },
        );
        assert!(matches!(result, Err(CoreError::Invalid(_))), "{result:?}");
    }
    let mut wrong = s.roots.clone();
    wrong.simulation_tick = 9;
    let result = verify_event_fact_occurrence_v1(
        &EventFactRequestV1 {
            head: &head,
            roots: &wrong,
            location,
        },
        &mut EventFactReadContextV1 {
            originals: &mut stream,
            catalog_pages: &pages,
            memory: &s.memory,
            class: MemoryClass::Ordinary,
            core_budget: &mut budget(),
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    );
    assert!(matches!(result, Err(CoreError::Invalid(_))));
}
#[test]
fn event_fact_stream_failure_budget_and_final_cancellation_never_issue_proof() {
    let s = setup();
    let id = *blake3::hash(b"original Event fact bytes").as_bytes();
    let history = event_history(&s, id);
    let head = accepted(&s, &history);
    for mode in 1..=5 {
        let (mut objects, pages) = event_sources(&s, &history, id);
        let stopped = Cell::new(false);
        let mut stream = EventStream {
            objects: &mut objects,
            mode,
            final_cancel: &stopped,
        };
        let mut facts = event_budget();
        if mode == 4 {
            facts.original_bytes_remaining = 2;
        }
        if mode == 5 {
            facts.facts_remaining = 0;
        }
        let result = verify_event_fact_occurrence_v1(
            &EventFactRequestV1 {
                head: &head,
                roots: &s.roots,
                location: EventFactLocationV1 {
                    height: 0,
                    ordinal: 1,
                    original: id,
                },
            },
            &mut EventFactReadContextV1 {
                originals: &mut stream,
                catalog_pages: &pages,
                memory: &s.memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut budget(),
                fact_budget: &mut facts,
                cancelled: || stopped.get(),
            },
        );
        match mode {
            1 => assert!(matches!(result, Err(CoreError::Invalid(_)))),
            2 => assert!(matches!(result, Err(CoreError::Object(_)))),
            3 => assert!(matches!(result, Err(CoreError::Cancelled))),
            4 | 5 => assert!(matches!(result, Err(CoreError::ResourceLimited(_)))),
            _ => unreachable!(),
        }
    }
}

#[test]
fn historical_event_uses_authenticated_catalog_path_without_replaying_ancestors() {
    use crate::incremental_execution::{ExecutionTransaction, TransactionLimits};
    let mut s = setup();
    let id = *blake3::hash(b"original Event fact bytes").as_bytes();
    let old = event_history(&s, id);
    s.commit = accepted(&s, &old);
    let (mut objects, mut pages) = event_sources(&s, &old, id);
    let catalog = old
        .body
        .catalog_before
        .extend(&old.body, &pages, &s.memory, 4 << 20)
        .unwrap();
    let authority = old.body.authority;
    let parent = ExecutionParent {
        roots: s.roots.clone(),
        commit: old.body.digest().unwrap(),
        authority,
    };
    let live = parent.fence().unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent,
        [60; 32],
        &pages,
        &s.memory,
        TransactionLimits {
            max_reads: 4,
            max_writes: 4,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let input = *blake3::hash(b"next original input").as_bytes();
    transaction
        .append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: input,
        })
        .unwrap();
    let guard = transaction.guard();
    let sealed = transaction.seal(&guard, &live, 1).unwrap();
    let next = HeaderPreparation {
        config: &s.config,
        catalog: &catalog.after,
        roots: &sealed.record.after,
        parent: Some(&old.body),
        parent_history: Some(&old.prefix),
        transition: Some(&sealed.record),
        genesis_facts: &[],
        operation: [60; 32],
        authority,
        execution_environment: s.config.execution_environment,
        checkpoint: false,
    }
    .prepare(&s.memory)
    .unwrap();
    let mut frozen = fixture::incremental::frozen(&s);
    frozen.reservation.body.runtime_manifest_digest =
        s.commit.record().runtime_manifest_digest.clone();
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
    let mut record = fixture::execution(&s.authority, &frozen);
    record.schema_version = formal::incremental::IWS_RECORD_SCHEMA;
    record.after_state_root = hex::encode(sealed.record.after.digest().unwrap());
    record.artifact_roots = s.commit.record().artifact_roots.clone();
    for (role, value) in [
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
        record.artifact_roots.insert(role, hex::encode(value));
    }
    let proof = fixture::proof(
        record,
        &s.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let head =
        formal::verify_formal_qualified(&proof, &s.authority, Some(&s.commit), None).unwrap();
    let _metadata = s.memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    for bytes in [
        next.body.canonical_bytes().unwrap(),
        next.prefix.canonical_bytes().unwrap(),
        next.segment.canonical_bytes().unwrap(),
        next.facts.canonical_bytes(&s.memory).unwrap().to_vec(),
    ] {
        let hash = *blake3::hash(&bytes).as_bytes();
        let charge = s
            .memory
            .reserve(bytes.capacity(), MemoryClass::Ordinary)
            .unwrap();
        objects.values.insert(hash, Arc::new(charge.retain(bytes)));
    }
    let stop = Cell::new(false);
    let mut stream = EventStream {
        objects: &mut objects,
        mode: 0,
        final_cancel: &stop,
    };
    let request = EventFactRequestV1 {
        head: &head,
        roots: &sealed.record.after,
        location: EventFactLocationV1 {
            height: 0,
            ordinal: 1,
            original: id,
        },
    };
    let mut quota = budget();
    let result = verify_event_fact_occurrence_v1(
        &request,
        &mut EventFactReadContextV1 {
            originals: &mut stream,
            catalog_pages: &pages,
            memory: &s.memory,
            class: MemoryClass::Ordinary,
            core_budget: &mut quota,
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    )
    .unwrap();
    assert_eq!(result.head_body_id(), next.body.digest().unwrap());
    assert_eq!(result.event_body_id(), old.body.digest().unwrap());
    assert!(pages.reads.get() > 0 && pages.reads.get() <= 257);
    let mut exhausted = budget();
    exhausted.merkle_reads_remaining = 0;
    let result = verify_event_fact_occurrence_v1(
        &request,
        &mut EventFactReadContextV1 {
            originals: &mut stream,
            catalog_pages: &pages,
            memory: &s.memory,
            class: MemoryClass::Ordinary,
            core_budget: &mut exhausted,
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    );
    assert!(matches!(
        result,
        Err(CoreError::ResourceLimited("Merkle read allowance"))
    ));
    assert!(
        pages
            .values
            .remove(&next.body.catalog_before.node_root)
            .is_some()
    );
    let result = verify_event_fact_occurrence_v1(
        &request,
        &mut EventFactReadContextV1 {
            originals: &mut stream,
            catalog_pages: &pages,
            memory: &s.memory,
            class: MemoryClass::Ordinary,
            core_budget: &mut budget(),
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    );
    assert!(
        matches!(result, Err(CoreError::Execution(_))),
        "missing prefix page: {result:?}"
    );
}

#[test]
fn event_fact_metadata_and_summary_can_use_recovery_only_admission() {
    let s = setup();
    let id = *blake3::hash(b"original Event fact bytes").as_bytes();
    let history = event_history(&s, id);
    let head = accepted(&s, &history);
    let (source, pages) = event_sources(&s, &history, id);
    // No Ordinary allocation is possible in this independent consumer pool.
    let memory = MemoryBudget::new(1 << 20, 1 << 20).unwrap();
    assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
    let metadata = memory.reserve(64 << 10, MemoryClass::Recovery).unwrap();
    let mut objects = Objects {
        memory: memory.clone(),
        class: MemoryClass::Recovery,
        values: BTreeMap::new(),
        reads: 0,
        _metadata: metadata,
    };
    for (id, original) in &source.values {
        let charge = memory
            .reserve(original.len(), MemoryClass::Recovery)
            .unwrap();
        objects
            .values
            .insert(*id, Arc::new(charge.retain(original.to_vec())));
    }
    let page_metadata = memory.reserve(64 << 10, MemoryClass::Recovery).unwrap();
    let mut recovery_pages = Pages {
        memory: memory.clone(),
        values: BTreeMap::new(),
        reads: Cell::new(0),
        class: MemoryClass::Recovery,
        _metadata: page_metadata,
    };
    for (id, original) in &pages.values {
        let charge = memory
            .reserve(original.len(), MemoryClass::Recovery)
            .unwrap();
        recovery_pages
            .values
            .insert(id.clone(), Arc::new(charge.retain(original.to_vec())));
    }
    let stop = Cell::new(false);
    let mut stream = EventStream {
        objects: &mut objects,
        mode: 0,
        final_cancel: &stop,
    };
    let result = verify_event_fact_occurrence_v1(
        &EventFactRequestV1 {
            head: &head,
            roots: &s.roots,
            location: EventFactLocationV1 {
                height: 0,
                ordinal: 1,
                original: id,
            },
        },
        &mut EventFactReadContextV1 {
            originals: &mut stream,
            catalog_pages: &recovery_pages,
            memory: &memory,
            class: MemoryClass::Recovery,
            core_budget: &mut budget(),
            fact_budget: &mut event_budget(),
            cancelled: || false,
        },
    )
    .unwrap();
    assert!(result.uses_budget(&memory));
    assert_eq!(result.location().original, id);
}

struct PatternStream<'a> {
    objects: &'a mut Objects,
    original: [u8; 32],
    length: usize,
    chunk: ReservedValue<Vec<u8>>,
    peak: &'a Cell<usize>,
    full_reads: &'a Cell<u64>,
}
impl OriginalObjectReader for PatternStream<'_> {
    type Error = std::io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.objects.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        self.objects.class
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> std::io::Result<Arc<ReservedValue<Vec<u8>>>> {
        if *id == self.original {
            self.full_reads.set(self.full_reads.get() + 1);
            return Err(std::io::Error::other("whole event read forbidden"));
        }
        self.objects.read_original(id, max, memory)
    }
}
impl OriginalObjectStreamReader for PatternStream<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        _: &MemoryBudget,
        output: &mut dyn Write,
    ) -> std::io::Result<u64> {
        assert_eq!(*id, self.original);
        assert!(self.length as u64 <= max);
        let mut remaining = self.length;
        while remaining != 0 {
            let n = remaining.min(self.chunk.len());
            output.write_all(&self.chunk[..n])?;
            self.peak
                .set(self.peak.get().max(self.objects.memory.used()));
            remaining -= n;
        }
        Ok(self.length as u64)
    }
}
#[test]
fn event_fact_raw_stream_crosses_32_and_64mib_without_whole_event_reads() {
    let mut observed = Vec::new();
    for length in [64, 1 << 20, (32 << 20) + 17, (64 << 20) + 17] {
        let s = setup();
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
        let (mut objects, pages) = sources(&s, &history); // event original intentionally has no whole-object backing
        let before = s.memory.used();
        let peak = Cell::new(before);
        let full_reads = Cell::new(0);
        let mut stream = PatternStream {
            objects: &mut objects,
            original: id,
            length,
            chunk,
            peak: &peak,
            full_reads: &full_reads,
        };
        let result = verify_event_fact_occurrence_v1(
            &EventFactRequestV1 {
                head: &head,
                roots: &s.roots,
                location: EventFactLocationV1 {
                    height: 0,
                    ordinal: 1,
                    original: id,
                },
            },
            &mut EventFactReadContextV1 {
                originals: &mut stream,
                catalog_pages: &pages,
                memory: &s.memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut budget(),
                fact_budget: &mut FactBudget {
                    facts_remaining: 1,
                    original_bytes_remaining: length as u64,
                    max_fact_bytes: length as u64,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        assert_eq!(result.original_bytes(), length as u64);
        assert_eq!(full_reads.get(), 0);
        let extra = peak.get() - before;
        eprintln!("event fact original_bytes={length} additional_peak_reserved={extra}");
        observed.push(extra);
    }
    for peak in &observed[1..] {
        assert!(*peak <= observed[0] + 4096, "{observed:?}");
    }
}

#[test]
fn event_fact_source_contract_and_returned_original_credit_are_checked() {
    let s = setup();
    let id = *blake3::hash(b"original Event fact bytes").as_bytes();
    let history = event_history(&s, id);
    let head = accepted(&s, &history);
    let foreign = MemoryBudget::new(4 << 20, 0).unwrap();
    for mode in 0..4 {
        let (mut objects, pages) = event_sources(&s, &history, id);
        let stop = Cell::new(false);
        let bad = mode >= 2;
        if bad {
            let body_id = history.body.digest().unwrap();
            let original = objects.values.get(&body_id).unwrap();
            let bytes = original.to_vec();
            let charge = if mode == 2 {
                foreign
                    .reserve(bytes.capacity(), MemoryClass::Ordinary)
                    .unwrap()
            } else {
                s.memory.reserve(1, MemoryClass::Ordinary).unwrap()
            };
            objects
                .values
                .insert(body_id, Arc::new(charge.retain(bytes)));
        }
        let mut stream = EventStream {
            objects: &mut objects,
            mode: 0,
            final_cancel: &stop,
        };
        let mut quota = budget();
        let mut facts = event_budget();
        let result = verify_event_fact_occurrence_v1(
            &EventFactRequestV1 {
                head: &head,
                roots: &s.roots,
                location: EventFactLocationV1 {
                    height: 0,
                    ordinal: 1,
                    original: id,
                },
            },
            &mut EventFactReadContextV1 {
                originals: &mut stream,
                catalog_pages: &pages,
                memory: if mode == 0 { &foreign } else { &s.memory },
                class: if mode == 1 {
                    MemoryClass::Recovery
                } else {
                    MemoryClass::Ordinary
                },
                core_budget: &mut quota,
                fact_budget: &mut facts,
                cancelled: || false,
            },
        );
        assert!(
            matches!(result, Err(CoreError::Invalid(_))),
            "source mode={mode}: {result:?}"
        );
        assert_eq!(facts.facts_remaining, 1);
        if !bad {
            assert_eq!(quota.objects_remaining, budget().objects_remaining);
            assert_eq!(objects.reads, 0);
        }
    }
}
