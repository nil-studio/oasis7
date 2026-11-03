use super::*;
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
fn transfer_pair(
    paths: &[std::path::PathBuf; 2],
    total_bytes: usize,
    expect_encoding_limit: bool,
    cancel_final: bool,
) -> Option<RootSetV1> {
    let roots = {
        let memory = MemoryBudget::new(total_bytes, 4 << 20).unwrap();
        // GEN uses a separate larger source pool which is destroyed before
        // the continuous bounded transfer starts.
        let source_path = paths[0].with_extension("source");
        let roots = {
            let producer = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store =
                IncrementalPageStore::create(&source_path, producer.clone(), PageCompression::Raw)
                    .unwrap();
            seed(&store, &producer)
        };
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
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let stopped = AtomicBool::new(false);
        let mut never1 = || false;
        let final_phase = std::cell::Cell::new(false);
        let before_recipes = std::cell::Cell::new(0usize);
        let recipe_path = second.root().join("recipes");
        let recipe_count = || std::fs::read_dir(&recipe_path).unwrap().count();
        let mut never2 = || final_phase.get() && recipe_count() > before_recipes.get();
        let mut first_io = physical();
        let mut second_io = physical();
        let mut first_writer = first
            .buffered_page_writer(&mut first_io, &stopped, &mut never1)
            .unwrap();
        let mut second_writer = second
            .buffered_page_writer(&mut second_io, &stopped, &mut never2)
            .unwrap();
        let limited = mark_fixed_economy_family_pages_v1(
            &roots,
            &mut context,
            &mut FixedEconomyPagesWorkV1 {
                outer_scan: &mut scan(),
                inner_scan: &mut scan(),
                cells: &mut 8192,
                max_pages: 0,
            },
        );
        assert!(matches!(limited, Err(ValueTreeError::ResourceLimited(_))));
        let result = transfer_fixed_economy_family_pages_v1(
            &roots,
            &mut context,
            &mut FixedEconomyPagesWorkV1 {
                outer_scan: &mut scan(),
                inner_scan: &mut scan(),
                cells: &mut 8192,
                max_pages: 8192,
            },
            &mut Mirror {
                first: &mut first_writer,
                second: &mut second_writer,
                memory: &memory,
            },
        );
        if expect_encoding_limit {
            assert!(matches!(result, Err(ValueTreeError::Memory(_))));
            return None;
        }
        let result = result.unwrap();
        result
            .qualification()
            .validate_binding(&roots.digest().unwrap())
            .unwrap();
        assert!(result.page_count() > 0);
        // A completed typed traversal is still provisional while either
        // writer retains pages. Both final flushes are mandatory.
        assert!(first_writer.pending_reserved_bytes() > 0);
        assert!(second_writer.pending_reserved_bytes() > 0);
        first_writer.finish().unwrap();
        before_recipes.set(recipe_count());
        final_phase.set(cancel_final);
        let finished = second_writer.finish();
        if cancel_final {
            assert!(matches!(
                finished,
                Err(oasis7_distfs::incremental_page_store::PageStoreError::Cancelled)
            ));
            assert!(recipe_count() > before_recipes.get());
        } else {
            finished.unwrap();
        }
        let source_pages = source.value_session(physical());
        let first_pages = first.value_session(physical());
        let second_pages = second.value_session(physical());
        result
            .visit_pages(|id| -> Result<(), ValueTreeError> {
                let original = source_pages.read_node(
                    id,
                    VALUE_MAX_NODE_BYTES,
                    &memory,
                    MemoryClass::Ordinary,
                )?;
                assert_eq!(blake3::hash(&original).as_bytes(), id);
                for target in [&first_pages, &second_pages] {
                    let got = target.read_node(
                        id,
                        VALUE_MAX_NODE_BYTES,
                        &memory,
                        MemoryClass::Ordinary,
                    )?;
                    assert_eq!(got.as_slice(), original.as_slice());
                }
                Ok(())
            })
            .unwrap();
        roots
    };
    Some(roots)
}
#[test]
fn typed_fixed_economy_pages_transfer_to_two_buffered_formats_then_cold_recover() {
    let directory = Directory::new();
    let paths = [
        directory.0.join("raw-target"),
        directory.0.join("zstd-target"),
    ];
    let negative_paths = [
        directory.0.join("limited-raw"),
        directory.0.join("limited-zstd"),
    ];
    // Keep the original 8MiB pool as a strict negative: the frozen codec's
    // declared 16MiB encoder workspace must fail admission, never escape as a
    // complete typed transfer. Positive encoding has that existing workspace
    // plus bounded page tracking and mirrored output, and is not a C<T metric.
    assert!(transfer_pair(&negative_paths, 8 << 20, true, false).is_none());
    let roots = transfer_pair(&paths, 32 << 20, false, false).unwrap();
    let cancelled_paths = [
        directory.0.join("cancelled-raw"),
        directory.0.join("cancelled-zstd"),
    ];
    let cancelled_roots = transfer_pair(&cancelled_paths, 32 << 20, false, true).unwrap();
    assert_eq!(roots, cancelled_roots);
    // The cancelled destination has no completion result. A fresh receiver can
    // independently resume by proving its already durable complete root closure.
    // Original World, both transfer writers, source handle and all original
    // pools are gone. Each persisted format receives a new root-only pool.
    for (path, compression) in paths.iter().chain(cancelled_paths.iter()).zip([
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
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let proof = qualify_fixed_economy_family_v1(
            &roots,
            &mut context,
            &mut scan(),
            &mut scan(),
            &mut 8192,
        )
        .unwrap();
        proof.validate_binding(&roots.digest().unwrap()).unwrap();
        context.root = roots
            .state
            .iter()
            .find(|root| root.namespace == StateNamespaceV1::Authorization)
            .unwrap();
        let record = open_qualified_fixed_economy_record_v1(
            FixedEconomyFieldV1::RewardSignatureGovernancePolicy,
            &roots,
            &proof,
            &mut context,
        )
        .unwrap();
        let value = read_qualified_fixed_economy_cell_v1(
            FixedEconomyCellRequestV1 {
                column: FixedEconomyColumnV1::RequireMintsigV2,
                qualification: &record,
            },
            &mut context,
        )
        .unwrap();
        assert_eq!(value, FixedEconomyValueV1::Bool(true));
    }
}

struct AfterPut<'a, W> {
    source: &'a mut W,
    puts: &'a std::cell::Cell<usize>,
    stop: &'a std::cell::Cell<bool>,
    cancel: bool,
    fail: bool,
}
impl<W: ValueNodeWriter> ValueNodeWriter for AfterPut<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.source.put_node(id, bytes)?;
        self.puts.set(self.puts.get() + 1);
        if self.fail && self.puts.get() == 3 {
            if self.cancel {
                self.stop.set(true);
            } else {
                return Err(ValueTreeError::Io(std::io::Error::other(
                    "fixed economy third transferred put",
                )));
            }
        }
        Ok(())
    }
}
struct Missing<'a, R> {
    source: &'a R,
    id: [u8; 32],
    hits: &'a std::cell::Cell<usize>,
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
            return Err(ValueTreeError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "missing typed child",
            )));
        }
        self.source.read_node(id, max, memory, class)
    }
}
struct MissingState<'a, M> {
    source: &'a M,
    digest: Option<&'a str>,
    hits: &'a std::cell::Cell<usize>,
}
impl<M: MerkleNodeReader> MerkleNodeReader for MissingState<'_, M> {
    type Node = M::Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if self.digest == Some(digest) {
            self.hits.set(self.hits.get() + 1);
            return Err("fixed economy missing typed state child".into());
        }
        self.source.read_node(digest, max)
    }
}
#[test]
fn fixed_economy_page_collection_rejects_real_sink_faults_and_missing_typed_children() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let source = IncrementalPageStore::create(
        &directory.0.join("fault-source"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&source, &memory);
    let missing_id = {
        let state = source
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = source.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Business)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let child =
            inspect_fixed_economy_root_v1(FixedEconomyFieldV1::MainTokenSupply, &mut ctx).unwrap();
        let raw = child.node_root.as_bytes();
        let mut id = [0u8; 32];
        for index in 0..32 {
            id[index] = u8::from_str_radix(
                std::str::from_utf8(&raw[index * 2..index * 2 + 2]).unwrap(),
                16,
            )
            .unwrap();
        }
        id
    };
    for mode in 0..3 {
        let target = IncrementalPageStore::create(
            &directory.0.join(format!("fault-target-{mode}")),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let puts = std::cell::Cell::new(0);
        let stop = std::cell::Cell::new(false);
        let hits = std::cell::Cell::new(0);
        let state_backend = source
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let missing_digest = missing_id
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let state = MissingState {
            source: &state_backend,
            digest: if mode == 2 {
                Some(&missing_digest)
            } else {
                None
            },
            hits: &hits,
        };
        let backend = source.value_session(physical());
        let values = Missing {
            source: &backend,
            id: if mode == 2 { missing_id } else { [0; 32] },
            hits: &hits,
        };
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
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
            record_bytes: 4096,
            take_state_error: || {
                if hits.get() > 0 {
                    Some(ValueTreeError::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "missing typed state child",
                    )))
                } else {
                    state_backend.take_read_error().map(state_error)
                }
            },
            cancelled: || stop.get(),
        };
        let mut sink = target.value_session(physical());
        let result = if mode == 2 {
            mark_fixed_economy_family_pages_v1(
                &roots,
                &mut ctx,
                &mut FixedEconomyPagesWorkV1 {
                    outer_scan: &mut scan(),
                    inner_scan: &mut scan(),
                    cells: &mut 8192,
                    max_pages: 8192,
                },
            )
        } else {
            transfer_fixed_economy_family_pages_v1(
                &roots,
                &mut ctx,
                &mut FixedEconomyPagesWorkV1 {
                    outer_scan: &mut scan(),
                    inner_scan: &mut scan(),
                    cells: &mut 8192,
                    max_pages: 8192,
                },
                &mut AfterPut {
                    source: &mut sink,
                    puts: &puts,
                    stop: &stop,
                    cancel: mode == 1,
                    fail: mode < 2,
                },
            )
        };
        if let Err(error) = &result {
            eprintln!(
                "page fault mode={mode} puts={} missing_hits={} error={error:?}",
                puts.get(),
                hits.get()
            );
        }
        if mode == 1 {
            assert!(matches!(result, Err(ValueTreeError::Cancelled)));
        } else {
            assert!(matches!(result, Err(ValueTreeError::Io(_))));
        }
        if mode < 2 {
            assert_eq!(puts.get(), 3);
            assert!(
                std::fs::read_dir(target.root().join("recipes"))
                    .unwrap()
                    .count()
                    > 0
            );
        } else {
            assert_eq!(hits.get(), 1);
        }
    }
}
