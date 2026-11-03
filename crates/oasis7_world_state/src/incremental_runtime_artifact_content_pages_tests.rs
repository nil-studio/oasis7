use super::*;
use std::cell::Cell;
#[test]
fn typed_page_transfer_lists_separate_raw_edges_and_refuses_page_or_original_quota() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots) = seed(&memory);
    for mode in 0..3 {
        let mut target = Pages::new(&memory);
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut originals = Originals {
            memory: &memory,
            missing: false,
            calls: 0,
        };
        let result = transfer_runtime_artifact_content_state_pages_v1(
            &roots,
            &mut WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || None,
                cancelled: || false,
            },
            &mut originals,
            &mut RuntimeArtifactContentPagesWorkV1 {
                outer_scan: &mut scan(),
                raw: &mut RuntimeArtifactOriginalWorkV1 {
                    objects_remaining: 2,
                    bytes_remaining: 4096,
                    max_object_bytes: 4096,
                },
                max_pages: if mode == 1 { 0 } else { 4096 },
                max_originals: if mode == 2 { 0 } else { 1 },
            },
            &mut target,
        );
        if mode != 0 {
            assert!(matches!(
                result,
                Err(RuntimeArtifactOriginalErrorV1::Value(
                    ValueTreeError::ResourceLimited(_)
                ))
            ));
            assert_eq!(originals.calls, 0);
            continue;
        }
        let marked = result.unwrap();
        marked.qualification().validate_binding(&roots).unwrap();
        assert!(marked.page_count() > 0);
        assert_eq!(marked.original_count(), 1);
        assert_eq!(originals.calls, 2);
        marked
            .visit_pages::<ValueTreeError>(|id| {
                let before = ValueNodeReader::read_node(
                    &pages,
                    id,
                    1 << 20,
                    &memory,
                    MemoryClass::Ordinary,
                )?;
                let after = ValueNodeReader::read_node(
                    &target,
                    id,
                    1 << 20,
                    &memory,
                    MemoryClass::Ordinary,
                )?;
                assert_eq!(before.as_slice(), after.as_slice());
                assert_eq!(blake3::hash(&after).as_bytes(), id);
                Ok(())
            })
            .unwrap();
        marked
            .visit_originals::<ValueTreeError>(|original| {
                assert_eq!(original.id, *blake3::hash(ORIGINAL_BYTES).as_bytes());
                assert_eq!(original.byte_len, ORIGINAL_BYTES.len() as u64);
                assert!(!target.nodes.borrow().contains_key(&original.id));
                Ok(())
            })
            .unwrap();
        // State pages alone cannot stand in for these required raw originals.
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        assert!(matches!(
            qualify_runtime_artifact_content_v1(
                &roots,
                &mut WorldFieldReadContextV1 {
                    root: &roots.state[0],
                    state: &target,
                    values: ValueReadContext {
                        source: &target,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut reads
                    },
                    key_work: &mut keys,
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || None,
                    cancelled: || false
                },
                &mut scan(),
                &mut Originals {
                    memory: &memory,
                    missing: true,
                    calls: 0
                },
                &mut RuntimeArtifactOriginalWorkV1 {
                    objects_remaining: 2,
                    bytes_remaining: 4096,
                    max_object_bytes: 4096
                }
            ),
            Err(RuntimeArtifactOriginalErrorV1::Source(
                "missing required raw original"
            ))
        ));
    }
}
struct Counted<'a> {
    pages: &'a mut Pages,
    count: &'a Cell<usize>,
    fail_after: Option<usize>,
}
impl ValueNodeWriter for Counted<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.pages.put_node(id, bytes)?;
        self.count.set(self.count.get() + 1);
        if self.fail_after == Some(self.count.get()) {
            return Err(ValueTreeError::Io(std::io::Error::other(
                "after actual page put",
            )));
        }
        Ok(())
    }
}
#[test]
fn actual_final_unique_page_put_io_or_cancel_never_returns_a_completed_set() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots) = seed(&memory);
    let limit = {
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        mark_runtime_artifact_content_pages_v1(
            &roots,
            &mut WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
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
            &mut RuntimeArtifactContentPagesWorkV1 {
                outer_scan: &mut scan(),
                raw: &mut RuntimeArtifactOriginalWorkV1 {
                    objects_remaining: 2,
                    bytes_remaining: 4096,
                    max_object_bytes: 4096,
                },
                max_pages: 4096,
                max_originals: 1,
            },
        )
        .unwrap()
        .page_count()
    };
    assert!(limit > 0);
    for mode in 0..2 {
        let mut target = Pages::new(&memory);
        let count = Cell::new(0);
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let result = transfer_runtime_artifact_content_state_pages_v1(
            &roots,
            &mut WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || None,
                cancelled: || mode == 1 && count.get() >= limit,
            },
            &mut Originals {
                memory: &memory,
                missing: false,
                calls: 0,
            },
            &mut RuntimeArtifactContentPagesWorkV1 {
                outer_scan: &mut scan(),
                raw: &mut RuntimeArtifactOriginalWorkV1 {
                    objects_remaining: 2,
                    bytes_remaining: 4096,
                    max_object_bytes: 4096,
                },
                max_pages: 4096,
                max_originals: 1,
            },
            &mut Counted {
                pages: &mut target,
                count: &count,
                fail_after: if mode == 0 { Some(limit) } else { None },
            },
        );
        assert_eq!(count.get(), limit);
        assert_eq!(target.nodes.borrow().len(), limit);
        if mode == 0 {
            assert!(matches!(
                result,
                Err(RuntimeArtifactOriginalErrorV1::Value(ValueTreeError::Io(_)))
            ));
        } else {
            assert!(matches!(
                result,
                Err(RuntimeArtifactOriginalErrorV1::Value(
                    ValueTreeError::Cancelled
                ))
            ));
        }
    }
}
