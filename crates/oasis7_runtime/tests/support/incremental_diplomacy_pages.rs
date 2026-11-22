use super::*;
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader, incremental_value::ValueNodeWriter,
};
use std::cell::Cell;
#[path = "incremental_organization_flush.rs"]
mod flush;
struct FaultSink<'a, W> {
    sink: &'a mut W,
    written: &'a Cell<usize>,
    fail: bool,
}
impl<W: ValueNodeWriter> ValueNodeWriter for FaultSink<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.fail && self.written.get() == 3 {
            return Err(ValueTreeError::Io(std::io::Error::other(
                "injected diplomacy sink failure",
            )));
        }
        self.sink.put_node(id, bytes)?;
        self.written.set(self.written.get() + 1);
        Ok(())
    }
}
struct MissingChild<'a, M> {
    source: &'a M,
    digest: &'a str,
    reached: Cell<bool>,
}
impl<M: MerkleNodeReader> MerkleNodeReader for MissingChild<'_, M> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if digest == self.digest {
            self.reached.set(true);
            return Ok(None);
        }
        self.source.read_node(digest, max)
    }
}

#[test]
fn diplomacy_page_failures_never_return_a_completed_family() {
    let original = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let _source_admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &original.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = roots(&build(
        &store,
        &memory,
        &produce(&store, &memory, &source()),
    ));
    let auth = roots
        .state
        .iter()
        .find(|root| root.namespace == Ns::Business)
        .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut value_io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut outer = scan();
    let mut inner = scan();
    let mut cells = 1_000_000;
    let mut work = DiplomacyPagesWorkV1 {
        outer_scan: &mut outer,
        inner_scan: &mut inner,
        cells: &mut cells,
        max_payload_bytes: 165,
        max_pages: 32_768,
    };
    let written = Cell::new(0);
    let cancel = Cell::new(false);
    let child = {
        let mut context = WorldFieldReadContextV1 {
            root: auth,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut value_io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || cancel.get() && written.get() >= 3,
        };
        // Two distinct operations share the caller's finite work/read budgets.
        // A fresh destination ensures the observed puts really publish pages.
        for fail in [true, false] {
            written.set(0);
            cancel.set(!fail);
            let directory = Directory::new();
            let target = IncrementalPageStore::create(
                &directory.0.join("pages"),
                memory.clone(),
                PageCompression::Raw,
            )
            .unwrap();
            let mut target_values = target.value_session(physical());
            let mut sink = FaultSink {
                sink: &mut target_values,
                written: &written,
                fail,
            };
            let result =
                transfer_diplomacy_family_pages_v1(&roots, &mut context, &mut work, &mut sink);
            if fail {
                assert!(matches!(result, Err(ValueTreeError::Io(_))));
            } else {
                assert!(matches!(result, Err(ValueTreeError::Cancelled)));
            }
            assert_eq!(
                written.get(),
                3,
                "three actual successful puts precede failure/cancellation"
            );
            assert!(
                std::fs::read_dir(directory.0.join("pages/recipes"))
                    .unwrap()
                    .next()
                    .is_some(),
                "a physical prefix exists, yet no completed collection escaped"
            );
        }
        cancel.set(false);
        inspect_diplomacy_root_v1(DiplomacyFieldV1::Wars, KEY, &mut context)
            .unwrap()
            .expect("actual authenticated child reference")
    };
    let missing = MissingChild {
        source: &state,
        digest: &child.node_root,
        reached: Cell::new(false),
    };
    let mut context = WorldFieldReadContextV1 {
        root: auth,
        state: &missing,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    assert!(matches!(
        mark_diplomacy_family_pages_v1(&roots, &mut context, &mut work),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(
        missing.reached.get(),
        "the actual typed child edge was traversed"
    );
}

#[test]
fn diplomacy_buffered_pages_two_compressions_finish_and_small_recovery() {
    let original = Directory::new();
    let raw_directory = Directory::new();
    let zstd_directory = Directory::new();
    let cancel_directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &original.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = roots(&build(
        &store,
        &memory,
        &produce(&store, &memory, &source()),
    ));
    let canonical = roots.canonical_bytes().unwrap();
    let business = roots
        .state
        .iter()
        .find(|r| r.namespace == Ns::Business)
        .unwrap();
    let raw = IncrementalPageStore::create(
        &raw_directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let zstd = IncrementalPageStore::create(
        &zstd_directory.0.join("pages"),
        memory.clone(),
        PageCompression::Zstd,
    )
    .unwrap();
    let cancelled = IncrementalPageStore::create(
        &cancel_directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = fixed!(business, &state, &values, &memory, &mut io, &mut keys);
        let mut cells = 1_000_000;
        let mut outer = scan();
        let mut inner = scan();
        let mut work = DiplomacyPagesWorkV1 {
            outer_scan: &mut outer,
            inner_scan: &mut inner,
            cells: &mut cells,
            max_payload_bytes: 165,
            max_pages: 0,
        };
        assert!(matches!(
            mark_diplomacy_family_pages_v1(&roots, &mut ctx, &mut work),
            Err(ValueTreeError::ResourceLimited("diplomacy marked pages"))
        ));
        work.max_pages = 32768;
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let phase = Cell::new(false);
        let baseline = Cell::new(0);
        let mut after_final = || {
            phase.get()
                && std::fs::read_dir(cancelled.root().join("recipes"))
                    .unwrap()
                    .count()
                    > baseline.get()
        };
        let mut raw_io = physical();
        let mut zstd_io = physical();
        let mut cancel_io = physical();
        let mut never1 = || false;
        let mut never2 = || false;
        let mut raw_sink = raw
            .buffered_page_writer(&mut raw_io, &cancel, &mut never1)
            .unwrap();
        let mut zstd_sink = zstd
            .buffered_page_writer(&mut zstd_io, &cancel, &mut never2)
            .unwrap();
        let mut cancel_sink = cancelled
            .buffered_page_writer(&mut cancel_io, &cancel, &mut after_final)
            .unwrap();
        let pages = transfer_diplomacy_family_pages_v1(
            &roots,
            &mut ctx,
            &mut work,
            &mut flush::MirroredPages {
                accepted: &mut flush::MirroredPages {
                    accepted: &mut raw_sink,
                    cancelled: &mut zstd_sink,
                    memory: &memory,
                },
                cancelled: &mut cancel_sink,
                memory: &memory,
            },
        )
        .unwrap();
        assert!(raw_sink.pending_reserved_bytes() > 0);
        assert!(zstd_sink.pending_reserved_bytes() > 0);
        assert!(cancel_sink.pending_reserved_bytes() > 0);
        baseline.set(
            std::fs::read_dir(cancelled.root().join("recipes"))
                .unwrap()
                .count(),
        );
        phase.set(true);
        assert!(matches!(
            cancel_sink.finish().map(|()| pages.qualification()),
            Err(PageStoreError::Cancelled)
        ));
        assert!(
            std::fs::read_dir(cancelled.root().join("recipes"))
                .unwrap()
                .count()
                > baseline.get()
        );
        raw_sink.finish().unwrap();
        zstd_sink.finish().unwrap();
        assert_eq!(pages.qualification().record_counts(), [1; 4]);
        assert!(pages.page_count() > business.leaf_count as usize);
        for target in [&raw, &zstd] {
            let mut io = physical();
            pages
                .visit_pages(|id| {
                    let page = target
                        .read_page(id, 64 << 10, MemoryClass::Ordinary, &mut io, &cancel)
                        .unwrap();
                    assert_eq!(blake3::hash(&page).as_bytes(), id);
                    Ok::<_, ()>(())
                })
                .unwrap();
        }
    }
    drop(store);
    drop(raw);
    drop(zstd);
    drop(cancelled);
    drop(memory);
    for (directory, compression) in [
        (&raw_directory, PageCompression::Raw),
        (&zstd_directory, PageCompression::Zstd),
    ] {
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let cold = IncrementalPageStore::open(
            &directory.0.join("pages"),
            memory.clone(),
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let roots = RootSetV1::from_canonical_bytes(&canonical).unwrap();
        let root = roots
            .state
            .iter()
            .find(|r| r.namespace == Ns::Business)
            .unwrap();
        let state = cold
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = cold.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        assert_eq!(
            qualify_diplomacy_family_v1(&roots, &mut ctx, &mut scan(), &mut scan(), &mut 8192, 165)
                .unwrap()
                .record_counts(),
            [1; 4]
        );
    }
}
