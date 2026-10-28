use super::*;
use crate::incremental_execution::ExecutionFactRefV1;
use std::io::Write;

struct Stream<'a> {
    objects: &'a mut Objects,
    mode: u8,
    order: Vec<[u8; 32]>,
}
impl OriginalObjectReader for Stream<'_> {
    type Error = std::io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.objects.memory
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
impl OriginalObjectStreamReader for Stream<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        _: u64,
        _: &MemoryBudget,
        output: &mut dyn Write,
    ) -> std::io::Result<u64> {
        self.order.push(*id);
        if self.mode == 1 {
            return Err(std::io::ErrorKind::NotFound.into());
        }
        let bytes = self.objects.values.get(id).unwrap();
        if self.mode == 2 {
            output.write_all(b"substitution")?;
            return Ok(12);
        }
        if self.mode == 3 {
            let _ = output.write_all(bytes);
            return Ok(bytes.len() as u64);
        }
        output.write_all(bytes)?;
        if self.mode == 4 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        if self.mode == 5 {
            panic!("source panic after output");
        }
        Ok(bytes.len() as u64 + u64::from(self.mode == 6))
    }
}
fn allowance(bytes: u64) -> FactBudget {
    FactBudget {
        facts_remaining: 8,
        original_bytes_remaining: bytes,
        max_fact_bytes: bytes,
    }
}
fn checked(s: &Setup, objects: &mut Objects, pages: &Pages) -> ReservedValue<CheckedCommitCore> {
    let candidate = candidate(s, s.commit.record().clone());
    verify_commit_core(
        &request(s, &candidate),
        objects,
        pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap()
}
#[test]
fn exact_original_identity_and_byte_allowance() {
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let core = checked(&s, &mut objects, &pages);
    let id = core.facts().facts[0].id;
    let length = objects.values[&id].len() as u64;
    let mut stream = Stream {
        objects: &mut objects,
        mode: 0,
        order: vec![],
    };
    let mut quota = allowance(length);
    quota.facts_remaining = 1;
    let result =
        verify_fact_originals(&core, &mut stream, &s.memory, &mut quota, || false).unwrap();
    assert_eq!(result.counts(), [1, 0, 0]);
    assert_eq!(result.original_bytes(), length);
    assert_eq!(result.body_id(), s.history.body.digest().unwrap());
    assert_eq!(result.fact_set_id(), s.history.body.facts);
    assert_eq!(stream.order, vec![id]);
    assert_eq!(
        (quota.facts_remaining, quota.original_bytes_remaining),
        (0, 0)
    );
}
#[test]
fn missing_substituted_late_failure_and_lying_length_fail_closed() {
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let core = checked(&s, &mut objects, &pages);
    for mode in [1, 2, 4, 6] {
        let mut stream = Stream {
            objects: &mut objects,
            mode,
            order: vec![],
        };
        let mut quota = allowance(1 << 20);
        let before = s.memory.used();
        let result = verify_fact_originals(&core, &mut stream, &s.memory, &mut quota, || false);
        assert!(result.is_err());
        assert_eq!(quota.original_bytes_remaining, 0);
        assert_eq!(quota.facts_remaining, 7);
        assert_eq!(s.memory.used(), before);
    }
}
#[test]
fn ignored_sink_error_cannot_bypass_byte_or_cancel_limits() {
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let core = checked(&s, &mut objects, &pages);
    let mut stream = Stream {
        objects: &mut objects,
        mode: 3,
        order: vec![],
    };
    assert!(matches!(
        verify_fact_originals(&core, &mut stream, &s.memory, &mut allowance(1), || false),
        Err(CoreError::ResourceLimited(_))
    ));
    let mut calls = 0;
    assert!(matches!(
        verify_fact_originals(
            &core,
            &mut stream,
            &s.memory,
            &mut allowance(1 << 20),
            || {
                calls += 1;
                calls >= 2
            }
        ),
        Err(CoreError::Cancelled)
    ));
}
#[test]
fn unwind_spends_admission_and_zero_fact_budget_does_not_read() {
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let core = checked(&s, &mut objects, &pages);
    let mut stream = Stream {
        objects: &mut objects,
        mode: 5,
        order: vec![],
    };
    let mut quota = allowance(1 << 20);
    let before = s.memory.used();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            verify_fact_originals(&core, &mut stream, &s.memory, &mut quota, || false)
        }))
        .is_err()
    );
    assert_eq!(quota.original_bytes_remaining, 0);
    assert_eq!(s.memory.used(), before);
    stream.order.clear();
    quota.facts_remaining = 0;
    assert!(matches!(
        verify_fact_originals(&core, &mut stream, &s.memory, &mut quota, || false),
        Err(CoreError::ResourceLimited(_))
    ));
    assert!(stream.order.is_empty());
}

#[test]
fn mixed_duplicate_and_empty_facts_preserve_occurrence_order() {
    let s = setup();
    let extra = b"result and event original".to_vec();
    let extra_id = *blake3::hash(&extra).as_bytes();
    let empty_id = *blake3::hash(&[]).as_bytes();
    let mut facts = s.history.facts.facts.clone();
    facts.extend([
        ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Event,
            id: extra_id,
        },
        ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id: empty_id,
        },
        ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id: extra_id,
        },
    ]);
    let history = HeaderPreparation {
        config: &s.config,
        catalog: &s.history.body.catalog_before,
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
    let (mut objects, pages) = sources(&s, &history);
    for bytes in [extra.clone(), vec![]] {
        let id = *blake3::hash(&bytes).as_bytes();
        let charge = s
            .memory
            .reserve(bytes.capacity() + 64, MemoryClass::Ordinary)
            .unwrap();
        objects.values.insert(id, Arc::new(charge.retain(bytes)));
    }
    let mut record = s.commit.record().clone();
    record.artifact_roots.insert(
        ArtifactRole::IncrementalCommitBody,
        hex::encode(history.body.digest().unwrap()),
    );
    record.artifact_roots.insert(
        ArtifactRole::IncrementalHistoryPrefix,
        hex::encode(history.prefix.digest().unwrap()),
    );
    let candidate = candidate(&s, record);
    let core = verify_commit_core(
        &request(&s, &candidate),
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    let expected = facts
        .iter()
        .map(|f| objects.values[&f.id].len() as u64)
        .sum();
    let mut stream = Stream {
        objects: &mut objects,
        mode: 0,
        order: vec![],
    };
    let mut quota = allowance(expected);
    let result =
        verify_fact_originals(&core, &mut stream, &s.memory, &mut quota, || false).unwrap();
    assert_eq!(result.counts(), [1, 2, 1]);
    assert_eq!(result.original_bytes(), expected);
    assert_eq!(stream.order, facts.iter().map(|f| f.id).collect::<Vec<_>>());
    assert_eq!(quota.original_bytes_remaining, 0);
}

#[test]
fn foreign_pool_is_rejected_before_source_or_allowance_consumption() {
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let core = checked(&s, &mut objects, &pages);
    objects.memory = MemoryBudget::new(32 << 20, 0).unwrap();
    let mut stream = Stream {
        objects: &mut objects,
        mode: 0,
        order: vec![],
    };
    let mut quota = allowance(1000);
    assert!(matches!(
        verify_fact_originals(&core, &mut stream, &s.memory, &mut quota, || false),
        Err(CoreError::Invalid("fact source memory pools"))
    ));
    assert!(stream.order.is_empty());
    assert_eq!(
        (quota.facts_remaining, quota.original_bytes_remaining),
        (8, 1000)
    );
}
