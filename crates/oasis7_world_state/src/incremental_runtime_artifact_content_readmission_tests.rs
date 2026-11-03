use super::*;
use std::{cell::Cell, io::Write};

struct RecoveryPages<'a> {
    pages: &'a Pages,
    memory: MemoryBudget,
    reads: Cell<usize>,
}

impl RecoveryPages<'_> {
    fn copy_page(
        &self,
        bytes: &ReservedValue<Vec<u8>>,
        max_bytes: usize,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if bytes.len() > max_bytes || !bytes.uses_budget(&self.memory) {
            return Err(ValueTreeError::Invalid("recovery fixture page bound"));
        }
        let charge_bytes = bytes
            .len()
            .checked_add(4096)
            .ok_or(ValueTreeError::ResourceLimited(
                "recovery fixture page size",
            ))?;
        let charge = self.memory.reserve(charge_bytes, MemoryClass::Recovery)?;
        let mut copy = Vec::with_capacity(bytes.len());
        copy.extend_from_slice(bytes);
        if copy.capacity() > charge.bytes() {
            return Err(ValueTreeError::Invalid("recovery fixture page charge"));
        }
        self.reads.set(self.reads.get() + 1);
        Ok(Arc::new(charge.retain(copy)))
    }
}

impl ValueNodeReader for RecoveryPages<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if !memory.same_pool(&self.memory) || class != MemoryClass::Recovery {
            return Err(ValueTreeError::Invalid("recovery fixture pool/class"));
        }
        let node = self
            .pages
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("missing recovery fixture page"))?;
        self.copy_page(&node, max_bytes)
    }
}

impl MerkleNodeReader for RecoveryPages<'_> {
    type Node = Node;

    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }

    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }

    fn read_node(&self, id: &str, max_bytes: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|error| error.to_string())?
            .as_bytes();
        let node = self.pages.nodes.borrow().get(&id).cloned();
        node.map(|node| {
            self.copy_page(&node, max_bytes)
                .map(Node)
                .map_err(|error| format!("{error:?}"))
        })
        .transpose()
    }
}

struct RecoveryOriginals<'a> {
    memory: &'a MemoryBudget,
    calls: usize,
}

impl OriginalObjectReader for RecoveryOriginals<'_> {
    type Error = &'static str;

    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }

    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Recovery
    }

    fn read_original(
        &mut self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        panic!("stream only")
    }
}

impl oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader
    for RecoveryOriginals<'_>
{
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        _: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        self.calls += 1;
        if id != blake3::hash(ORIGINAL_BYTES).as_bytes() || max != ORIGINAL_BYTES.len() as u64 {
            return Err("unexpected recovery original");
        }
        output
            .write_all(ORIGINAL_BYTES)
            .map_err(|_| "recovery output refused")?;
        Ok(ORIGINAL_BYTES.len() as u64)
    }
}

fn qualify_recovery(
    pages: &RecoveryPages<'_>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    originals: &mut RecoveryOriginals<'_>,
) -> Result<
    ReservedValue<RuntimeArtifactContentQualificationV1>,
    RuntimeArtifactOriginalErrorV1<&'static str>,
> {
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    qualify_runtime_artifact_content_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: pages,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Recovery,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        originals,
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 2,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        },
    )
}

#[test]
fn recovery_content_family_readmission_rejects_wrong_scope_and_feeds_ordinary_receiver() {
    use oasis7_formal_protocol::incremental_execution::*;

    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots) = seed(&memory);

    let mut ordinary_reads = io();
    let mut ordinary_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let ordinary_source = qualify_runtime_artifact_content_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut ordinary_reads,
            },
            key_work: &mut ordinary_keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut Originals {
            memory: &memory,
            missing: false,
            calls: 0,
        },
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 2,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        },
    )
    .unwrap();
    assert_eq!(ordinary_source.allocation_class(), MemoryClass::Ordinary);
    let before_reject = memory.used();
    assert!(matches!(
        readmit_runtime_artifact_content_ordinary_v1(&ordinary_source, &roots, &memory, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    assert_eq!(memory.used(), before_reject);
    drop(ordinary_source);

    let recovered = RecoveryPages {
        pages: &pages,
        memory: memory.clone(),
        reads: Cell::new(0),
    };
    let mut recovery_originals = RecoveryOriginals {
        memory: &memory,
        calls: 0,
    };
    let recovery_source =
        qualify_recovery(&recovered, &roots, &memory, &mut recovery_originals).unwrap();
    assert_eq!(recovery_source.allocation_class(), MemoryClass::Recovery);
    assert_eq!(recovery_originals.calls, 2);
    assert!(recovered.reads.get() > 0);

    let source_reads = recovered.reads.get();
    let source_original_reads = recovery_originals.calls;
    let before_reject = memory.used();
    let mut wrong_roots = roots.clone();
    wrong_roots.height = wrong_roots.height.saturating_add(1);
    assert!(matches!(
        readmit_runtime_artifact_content_ordinary_v1(
            &recovery_source,
            &wrong_roots,
            &memory,
            || false
        ),
        Err(ValueTreeError::Invalid(_))
    ));
    assert_eq!(memory.used(), before_reject);

    let foreign = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let foreign_before = foreign.used();
    assert!(matches!(
        readmit_runtime_artifact_content_ordinary_v1(&recovery_source, &roots, &foreign, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    assert_eq!(foreign.used(), foreign_before);

    assert!(matches!(
        readmit_runtime_artifact_content_ordinary_v1(&recovery_source, &roots, &memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
    let mut cancellation_checks = 0;
    let before_late_cancel = memory.used();
    assert!(matches!(
        readmit_runtime_artifact_content_ordinary_v1(&recovery_source, &roots, &memory, || {
            cancellation_checks += 1;
            cancellation_checks == 2
        }),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(cancellation_checks, 2);
    assert_eq!(memory.used(), before_late_cancel);

    let ordinary_limit = (64 << 20) - (4 << 20);
    let used_before_pressure = memory.used();
    assert!(used_before_pressure < ordinary_limit);
    let pressure = memory
        .reserve(ordinary_limit - used_before_pressure, MemoryClass::Ordinary)
        .unwrap();
    let before_failed_admission = memory.used();
    assert!(matches!(
        readmit_runtime_artifact_content_ordinary_v1(&recovery_source, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    assert_eq!(memory.used(), before_failed_admission);
    drop(pressure);
    assert_eq!(memory.used(), used_before_pressure);

    // The bridge has no state/value/original reader input. This assertion pins
    // the reader side counters so later test refactors cannot accidentally add
    // source IO to the class-only operation.
    let before_bridge = memory.used();
    let ordinary =
        readmit_runtime_artifact_content_ordinary_v1(&recovery_source, &roots, &memory, || false)
            .unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(ordinary.reserved_bytes(), 512);
    assert_eq!(memory.used() - before_bridge, ordinary.reserved_bytes());
    assert_eq!(recovered.reads.get(), source_reads);
    assert_eq!(recovery_originals.calls, source_original_reads);
    ordinary.validate_binding(&roots).unwrap();
    drop(recovery_source);

    let operation = [0x72; 32];
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [0x71; 32],
        authority: AuthorityBinding {
            token: [0x73; 32],
            epoch: 8,
        },
    };
    let live = parent.fence().unwrap();
    let transaction = ExecutionTransaction::begin(
        parent.clone(),
        operation,
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 4,
            max_facts: 0,
            max_pending_node_bytes: 1 << 20,
        },
    )
    .unwrap();
    let guard = transaction.guard();
    let prepared = transaction.seal(&guard, &live, 0).unwrap();
    assert!(prepared.record.writes.is_empty());
    let verifier = TransitionVerifier {
        parent: &parent,
        operation,
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 1 << 20,
    };
    verifier.verify(&prepared.record).unwrap();
    let mut receiver_reads = io();
    let mut receiver_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let accepted = accept_runtime_artifact_content_transition_v1(
        &ordinary,
        &verifier,
        &prepared.record,
        &mut WorldFieldReadContextV1 {
            root: &prepared.record.after.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut receiver_reads,
            },
            key_work: &mut receiver_keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut Originals {
            memory: &memory,
            missing: false,
            calls: 0,
        },
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 2,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        },
    )
    .unwrap();
    accepted.validate_binding(&prepared.record.after).unwrap();
    assert_eq!(accepted.allocation_class(), MemoryClass::Ordinary);
}
