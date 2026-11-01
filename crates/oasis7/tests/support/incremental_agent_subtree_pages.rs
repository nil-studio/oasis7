use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use std::{cell::Cell, sync::atomic::AtomicBool};
struct Mirror<'a, A, B> {
    first: &'a mut A,
    second: &'a mut B,
    memory: &'a MemoryBudget,
}
impl<A: ValueNodeWriter, B: ValueNodeWriter> ValueNodeWriter for Mirror<'_, A, B> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let copy = self
            .memory
            .reserve(bytes.len(), MemoryClass::Ordinary)?
            .retain(bytes.to_vec());
        self.first.put_node(id, bytes)?;
        self.second.put_node(id, copy)
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 1000,
    }
}
fn structure() -> AgentSubtreeScanWorkV1 {
    AgentSubtreeScanWorkV1 {
        max_agents: 1,
        cells_remaining: 1000,
        max_scalar_bytes: 128 << 10,
    }
}
fn transfer_pair(paths: &[std::path::PathBuf; 2], cancel_final: bool) -> RootSetV1 {
    let source_path = paths[0].with_extension("source");
    let roots = {
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&source_path, memory.clone(), PageCompression::Raw)
                .unwrap();
        seed(&store, &memory)
    };
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let source = IncrementalPageStore::open(
        &source_path,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let first =
        IncrementalPageStore::create(&paths[0], memory.clone(), PageCompression::Raw).unwrap();
    let second =
        IncrementalPageStore::create(&paths[1], memory.clone(), PageCompression::Zstd).unwrap();
    let state = source
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = source.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root: &roots.state[0],
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 128 << 10,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut scan = scan();
    let mut work = structure();
    let limited = mark_agent_subtree_pages_v1(
        &roots,
        &mut context,
        &mut AgentSubtreePagesWorkV1 {
            scan: &mut scan,
            structure: &mut work,
            max_pages: 0,
            max_originals: 0,
        },
    );
    assert!(matches!(limited, Err(ValueTreeError::ResourceLimited(_))));
    let stopped = AtomicBool::new(false);
    let mut never = || false;
    let final_phase = Cell::new(false);
    let before = Cell::new(0usize);
    let recipes = second.root().join("recipes");
    let count = || std::fs::read_dir(&recipes).unwrap().count();
    let mut last_cancel = || final_phase.get() && count() > before.get();
    let mut first_io = physical();
    let mut second_io = physical();
    let mut a = first
        .buffered_page_writer(&mut first_io, &stopped, &mut never)
        .unwrap();
    let mut b = second
        .buffered_page_writer(&mut second_io, &stopped, &mut last_cancel)
        .unwrap();
    let marked = transfer_agent_subtree_pages_v1(
        &roots,
        &mut context,
        &mut AgentSubtreePagesWorkV1 {
            scan: &mut scan,
            structure: &mut work,
            max_pages: 20_000,
            max_originals: 0,
        },
        &mut Mirror {
            first: &mut a,
            second: &mut b,
            memory: &memory,
        },
    )
    .unwrap();
    assert_eq!(marked.structure().agent_count(), 1);
    assert_eq!(marked.original_count(), 0);
    marked
        .visit_external_originals(|_| -> Result<(), ValueTreeError> {
            panic!("empty original dependency set")
        })
        .unwrap();
    assert!(marked.page_count() > 0);
    assert!(a.pending_reserved_bytes() > 0);
    assert!(b.pending_reserved_bytes() > 0);
    a.finish().unwrap();
    before.set(count());
    final_phase.set(cancel_final);
    let accepted = b.finish().map(|_| marked);
    if cancel_final {
        assert!(matches!(accepted, Err(PageStoreError::Cancelled)));
        assert!(count() > before.get());
        eprintln!("agent final flush cancelled after new physical recipe");
    } else {
        let marked = accepted.unwrap();
        let original = source.value_session(physical());
        let a = first.value_session(physical());
        let b = second.value_session(physical());
        marked
            .visit_pages(|id| -> Result<(), ValueTreeError> {
                let bytes =
                    original.read_node(id, VALUE_MAX_NODE_BYTES, &memory, MemoryClass::Ordinary)?;
                assert_eq!(blake3::hash(&bytes).as_bytes(), id);
                for sink in [&a, &b] {
                    let got =
                        sink.read_node(id, VALUE_MAX_NODE_BYTES, &memory, MemoryClass::Ordinary)?;
                    assert_eq!(got.as_slice(), bytes.as_slice());
                }
                Ok(())
            })
            .unwrap();
        eprintln!(
            "agent {} state/value pages copied to both formats",
            marked.page_count()
        );
    }
    roots
}
struct HashOutput {
    hash: blake3::Hasher,
    bytes: usize,
}
impl std::io::Write for HashOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.hash.update(bytes);
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn typed_agent_state_transfer_finishes_both_formats_and_rejects_real_final_flush_cancellation() {
    let directory = Directory::new();
    let positive = [
        directory.0.join("raw-positive"),
        directory.0.join("zstd-positive"),
    ];
    let negative = [
        directory.0.join("raw-cancelled"),
        directory.0.join("zstd-cancelled"),
    ];
    let roots = transfer_pair(&positive, false);
    assert_eq!(roots, transfer_pair(&negative, true));
    // The cancelled attempt grants no receipt. A fresh receiver must prove
    // whatever complete bytes were actually published, using its own budget.
    for (path, compression) in positive.iter().chain(negative.iter()).zip([
        PageCompression::Raw,
        PageCompression::Zstd,
        PageCompression::Raw,
        PageCompression::Zstd,
    ]) {
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 128 << 10,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let proof = qualify_agent_subtree_structure_v1(
            &roots,
            &mut context,
            &mut scan(),
            &mut structure(),
            &mut |_| panic!("empty original dependencies"),
        )
        .unwrap();
        assert_eq!(proof.agent_count(), 1);
        assert_eq!(proof.mailbox_reference_count(), 0);
        let mut output = HashOutput {
            hash: blake3::Hasher::new(),
            bytes: 0,
        };
        assert_eq!(
            read_agent_subtree_scalar_raw_v1(
                &roots,
                AgentSubtreeScalarRequestV1 {
                    agent: "agent 界",
                    field: AgentFieldV1::IntentSummary,
                    ordinal: 0
                },
                &mut context,
                128 << 10,
                &mut output
            )
            .unwrap(),
            Some(AgentSubtreeValueV1::Text { utf8_bytes: 66_000 })
        );
        let mut expected = blake3::Hasher::new();
        for _ in 0..22_000 {
            expected.update("界".as_bytes());
        }
        assert_eq!(output.bytes, 66_000);
        assert_eq!(output.hash.finalize(), expected.finalize());
    }
}
