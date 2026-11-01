use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use std::sync::atomic::AtomicBool;
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
pub(super) fn roundtrip(source: &IncrementalPageStore, memory: &MemoryBudget, roots: &RootSetV1) {
    let directory = Directory::new();
    let paths = [directory.0.join("raw"), directory.0.join("zstd")];
    {
        let targets = [
            IncrementalPageStore::create(&paths[0], memory.clone(), PageCompression::Raw).unwrap(),
            IncrementalPageStore::create(&paths[1], memory.clone(), PageCompression::Zstd).unwrap(),
        ];
        let state = source
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = source.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Effects),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || false,
        };
        let mut scan = StateScanBudget {
            nodes_remaining: 100000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        };
        let mut records = RuntimeEffectJsonWorkV1 {
            cells_remaining: 69,
            json_nodes_remaining: 6,
            max_depth: 16,
            max_payload_bytes: 165,
        };
        let denied = mark_runtime_effect_family_pages_v1(
            roots,
            &mut ctx,
            &mut RuntimeEffectPagesWorkV1 {
                outer_scan: &mut scan,
                records: &mut records,
                max_pages: 0,
            },
        );
        assert!(matches!(denied, Err(ValueTreeError::ResourceLimited(_))));
        // Exactly 69 independently declared cells. Each sparse leaf contributes
        // at most 257 State pages; every small nonempty Member value has at
        // most its Value root plus one data leaf. Empty namespaces need no page.
        const MAX_PAGES: usize = 69 * (257 + 2);
        let stopped = AtomicBool::new(false);
        let mut first_io = physical();
        let mut second_io = physical();
        let mut never1 = || false;
        let mut never2 = || false;
        let mut first = targets[0]
            .buffered_page_writer(&mut first_io, &stopped, &mut never1)
            .unwrap();
        let mut second = targets[1]
            .buffered_page_writer(&mut second_io, &stopped, &mut never2)
            .unwrap();
        // The quota-zero call failed before traversal; this is a new complete
        // public transfer operation with continuous budgets through all pages.
        let mut scan = StateScanBudget {
            nodes_remaining: 100000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        };
        let mut records = RuntimeEffectJsonWorkV1 {
            cells_remaining: 69,
            json_nodes_remaining: 6,
            max_depth: 16,
            max_payload_bytes: 165,
        };
        let result = transfer_runtime_effect_family_pages_v1(
            roots,
            &mut ctx,
            &mut RuntimeEffectPagesWorkV1 {
                outer_scan: &mut scan,
                records: &mut records,
                max_pages: MAX_PAGES,
            },
            &mut Mirror {
                first: &mut first,
                second: &mut second,
                memory,
            },
        )
        .unwrap();
        assert_eq!(records.cells_remaining, 0);
        assert_eq!(records.json_nodes_remaining, 0);
        assert_eq!(result.qualification().record_counts(), [1, 1]);
        assert!(result.page_count() > 0);
        assert!(result.page_count() <= MAX_PAGES);
        // A traversal result is provisional while either buffered sink has an
        // unpublished tail. The consumer must finish both before cold recovery.
        assert!(first.pending_reserved_bytes() > 0);
        assert!(second.pending_reserved_bytes() > 0);
        first.finish().unwrap();
        second.finish().unwrap();
        let source_pages = source.value_session(physical());
        let raw_pages = targets[0].value_session(physical());
        let zstd_pages = targets[1].value_session(physical());
        result
            .visit_pages(|id| -> Result<(), ValueTreeError> {
                let original = source_pages.read_node(
                    id,
                    VALUE_MAX_NODE_BYTES,
                    memory,
                    MemoryClass::Ordinary,
                )?;
                assert_eq!(blake3::hash(&original).as_bytes(), id);
                for target in [&raw_pages, &zstd_pages] {
                    let copy = target.read_node(
                        id,
                        VALUE_MAX_NODE_BYTES,
                        memory,
                        MemoryClass::Ordinary,
                    )?;
                    assert_eq!(copy.as_slice(), original.as_slice());
                }
                Ok(())
            })
            .unwrap();
        eprintln!(
            "Effects typed pages={} structural_upper={MAX_PAGES}",
            result.page_count()
        );
    }
    // Targets, all writer tails and warm readers above have been dropped.
    // Each independent recovery has a new small pool and only copied pages.
    for (path, compression) in paths
        .iter()
        .zip([PageCompression::Raw, PageCompression::Zstd])
    {
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let target =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let state = target
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = target.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Effects),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || false,
        };
        let mut work = RuntimeEffectJsonWorkV1 {
            cells_remaining: 69,
            json_nodes_remaining: 6,
            max_depth: 16,
            max_payload_bytes: 165,
        };
        let family = qualify_runtime_effect_records_v1(
            roots,
            &mut ctx,
            &mut StateScanBudget {
                nodes_remaining: 100000,
                leaves_remaining: 1000,
                read_bytes_remaining: 64 << 20,
            },
            &mut work,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [1, 1]);
        assert_eq!(work.cells_remaining, 0);
        assert_eq!(work.json_nodes_remaining, 0);
        let proof = open_runtime_effect_record_v1(
            RuntimeEffectRecordKeyV1::Pending(0),
            roots,
            &family,
            &mut ctx,
        )
        .unwrap()
        .unwrap();
        ctx.root = proof.root();
        let mut bytes = Vec::new();
        assert_eq!(
            read_qualified_runtime_effect_field_v1(
                &proof,
                RuntimeEffectFieldV1::IntentId,
                &mut ctx,
                165,
                &mut bytes
            )
            .unwrap(),
            RuntimeEffectFieldValueV1::Text { utf8_bytes: 18 }
        );
        assert_eq!(bytes, b" embedded pending ");
    }
}
struct StopAfter<'a, W> {
    inner: W,
    puts: usize,
    flag: &'a std::cell::Cell<bool>,
    io: bool,
    first: Option<[u8; 32]>,
}
impl<W: ValueNodeWriter> ValueNodeWriter for StopAfter<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        data: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, data)?;
        self.first.get_or_insert(id);
        self.puts += 1;
        if self.puts == 3 {
            if self.io {
                return Err(ValueTreeError::Io(std::io::Error::other(
                    "actual third effect page copied",
                )));
            }
            self.flag.set(true);
        }
        Ok(())
    }
}
struct Missing<'a, R> {
    source: &'a R,
    id: [u8; 32],
    hits: std::cell::Cell<usize>,
}
impl<R: ValueNodeReader> ValueNodeReader for Missing<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if id == &self.id {
            self.hits.set(self.hits.get() + 1);
            return Err(ValueTreeError::Invalid(
                "missing required effect JSON state page",
            ));
        }
        self.source.read_node(id, max, memory, class)
    }
}
#[test]
fn missing_child_real_copy_failure_and_final_flush_cancel_return_no_complete_transfer() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let source = IncrementalPageStore::create(
        &directory.0.join("source"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = super::scaling::seed(
        &source,
        &memory,
        EffectIntent {
            birth_era: None,
            intent_id: "id".into(),
            kind: "kind".into(),
            params: serde_json::Value::Bool(true),
            cap_ref: "cap".into(),
            origin: EffectOrigin::System,
        },
        165,
    );
    for io_failure in [false, true] {
        let path = directory.0.join(if io_failure { "io" } else { "cancel" });
        let target =
            IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
        let flag = std::cell::Cell::new(false);
        let mut sink = StopAfter {
            inner: target.value_session(physical()),
            puts: 0,
            flag: &flag,
            io: io_failure,
            first: None,
        };
        let state = source
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = source.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Effects),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || flag.get(),
        };
        let result = transfer_runtime_effect_family_pages_v1(
            &roots,
            &mut ctx,
            &mut RuntimeEffectPagesWorkV1 {
                outer_scan: &mut StateScanBudget {
                    nodes_remaining: 100000,
                    leaves_remaining: 1000,
                    read_bytes_remaining: 64 << 20,
                },
                records: &mut RuntimeEffectJsonWorkV1 {
                    cells_remaining: 29,
                    json_nodes_remaining: 1,
                    max_depth: 0,
                    max_payload_bytes: 165,
                },
                max_pages: 29 * (257 + 2),
            },
            &mut sink,
        );
        let error = match result {
            Err(e) => e,
            Ok(_) => panic!("completed transfer after real copy failure"),
        };
        if io_failure {
            assert!(matches!(error, ValueTreeError::Io(_)), "{error:?}");
        } else {
            assert!(matches!(error, ValueTreeError::Cancelled), "{error:?}");
        }
        assert_eq!(sink.puts, 3);
        assert!(std::fs::read_dir(path.join("recipes")).unwrap().count() > 0);
        let id = sink.first.unwrap();
        let target_reader = target.value_session(physical());
        let data = target_reader
            .read_node(&id, VALUE_MAX_NODE_BYTES, &memory, MemoryClass::Ordinary)
            .unwrap();
        assert_eq!(blake3::hash(&data).as_bytes(), &id);
    }
    // A real, previously qualified nested JSON root is selectively unavailable.
    let state = source
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = source.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Effects),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || {
            state
                .take_read_error()
                .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
        },
        cancelled: || false,
    };
    let family = qualify_runtime_effect_records_v1(
        &roots,
        &mut ctx,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        },
        &mut RuntimeEffectJsonWorkV1 {
            cells_remaining: 29,
            json_nodes_remaining: 1,
            max_depth: 0,
            max_payload_bytes: 165,
        },
    )
    .unwrap();
    let proof = open_runtime_effect_record_v1(
        RuntimeEffectRecordKeyV1::Pending(0),
        &roots,
        &family,
        &mut ctx,
    )
    .unwrap()
    .unwrap();
    let json = {
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: proof.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || false,
        };
        read_qualified_runtime_effect_params_root_v1(&proof, &mut ctx).unwrap()
    };
    let id = *blake3::Hash::from_hex(&json.node_root).unwrap().as_bytes();
    assert!(
        values
            .read_node(&id, VALUE_MAX_NODE_BYTES, &memory, MemoryClass::Ordinary)
            .is_ok()
    );
    let missing = Missing {
        source: &values,
        id,
        hits: std::cell::Cell::new(0),
    };
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Effects),
        state: &state,
        values: ValueReadContext {
            source: &missing,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || {
            state
                .take_read_error()
                .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
        },
        cancelled: || false,
    };
    let result = mark_runtime_effect_family_pages_v1(
        &roots,
        &mut ctx,
        &mut RuntimeEffectPagesWorkV1 {
            outer_scan: &mut StateScanBudget {
                nodes_remaining: 100000,
                leaves_remaining: 1000,
                read_bytes_remaining: 64 << 20,
            },
            records: &mut RuntimeEffectJsonWorkV1 {
                cells_remaining: 29,
                json_nodes_remaining: 1,
                max_depth: 0,
                max_payload_bytes: 165,
            },
            max_pages: 29 * (257 + 2),
        },
    );
    assert!(matches!(
        result,
        Err(ValueTreeError::Invalid(
            "missing required effect JSON state page"
        ))
    ));
    assert!(missing.hits.get() > 0);
    // Finish cancellation is armed only after complete traversal, and only once
    // the real final recipe is published; a local traversal result stays local.
    let target = IncrementalPageStore::create(
        &directory.0.join("final-flush"),
        memory.clone(),
        PageCompression::Zstd,
    )
    .unwrap();
    let state = source
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = source.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Effects),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || {
            state
                .take_read_error()
                .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
        },
        cancelled: || false,
    };
    let phase = std::cell::Cell::new(false);
    let before_finish = std::cell::Cell::new(0);
    let recipe_count = || {
        std::fs::read_dir(target.root().join("recipes"))
            .unwrap()
            .count()
    };
    let stopped = AtomicBool::new(false);
    let mut cancel = || phase.get() && recipe_count() > before_finish.get();
    let mut sink_io = physical();
    let mut sink = target
        .buffered_page_writer(&mut sink_io, &stopped, &mut cancel)
        .unwrap();
    let result = transfer_runtime_effect_family_pages_v1(
        &roots,
        &mut ctx,
        &mut RuntimeEffectPagesWorkV1 {
            outer_scan: &mut StateScanBudget {
                nodes_remaining: 100000,
                leaves_remaining: 1000,
                read_bytes_remaining: 64 << 20,
            },
            records: &mut RuntimeEffectJsonWorkV1 {
                cells_remaining: 29,
                json_nodes_remaining: 1,
                max_depth: 0,
                max_payload_bytes: 165,
            },
            max_pages: 29 * (257 + 2),
        },
        &mut sink,
    )
    .unwrap();
    assert_eq!(result.qualification().record_counts(), [1, 0]);
    assert!(sink.pending_reserved_bytes() > 0);
    before_finish.set(recipe_count());
    phase.set(true);
    let finished = sink.finish();
    assert!(matches!(finished, Err(PageStoreError::Cancelled)));
    assert!(recipe_count() > before_finish.get());
    drop(result);
}
