use super::*;
use oasis7_formal_protocol::incremental_state::{StateNamespaceV1, StateScanBudget};

struct BadValues<'a> {
    source: &'a Values<'a>,
    foreign: &'a MemoryBudget,
    mode: usize,
    reads: Cell<usize>,
}
impl ValueNodeReader for BadValues<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.reads.set(self.reads.get() + 1);
        if self.mode == 2 {
            return Err(ValueTreeError::Invalid("injected missing Agent value page"));
        }
        let original = self.source.read_node(id, max, memory, class)?;
        let allocation = if self.mode == 0 { self.foreign } else { memory };
        // Deliberately return a malformed carrier, while admitting the test copy
        // before allocation. The receiver must reject it before decoding bytes.
        let charge = allocation.reserve(original.len(), class)?;
        let bytes = original.to_vec();
        let copy = if self.mode == 0 {
            charge.retain(bytes)
        } else {
            let bad = allocation.reserve(0, class)?.retain(bytes);
            drop(charge);
            bad
        };
        Ok(Arc::new(copy))
    }
}

#[test]
fn agent_value_source_faults_reject_after_successful_prefix_and_poison_transaction() {
    for mode in 0..3 {
        let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
        let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let extra = RefCell::new(Pages::default());
        let values = Values {
            base: &pages,
            extra: &extra,
        };
        let state = fixture::State::new(&pages, &memory);
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
        let mut reads = io();
        let mut writes = io();
        let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut sink = Sink(&extra);
        let guard = tx.guard();
        assert!(
            change_agent_subtree_scalar_v1(
                AgentSubtreeCellChangeV1 {
                    query: AgentSubtreeQualifiedRequestV1 {
                        cell: request("agent α", AgentFieldV1::PositionX, 0),
                        qualification: &proof,
                    },
                    expected: Some(AgentScalarRefV1::I64(i64::MIN)),
                    after: Some(AgentScalarRefV1::I64(5)),
                    max_scalar_bytes: 1024,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut reads,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut writes,
                    cancelled: || false
                },
                &guard,
                &live,
            )
            .unwrap()
        );
        let bad = BadValues {
            source: &values,
            foreign: &foreign,
            mode,
            reads: Cell::new(0),
        };
        let guard = tx.guard();
        let result = change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: request("agent α", AgentFieldV1::PositionY, 0),
                    qualification: &proof,
                },
                expected: Some(AgentScalarRefV1::I64(0)),
                after: Some(AgentScalarRefV1::I64(7)),
                max_scalar_bytes: 1024,
            },
            &mut overlay!(
                &mut tx,
                &state,
                &bad,
                &memory,
                &mut reads,
                &mut keys,
                || false
            ),
            &mut ValueMutationContextV1 {
                sink: &mut sink,
                memory: &memory,
                io: &mut writes,
                cancelled: || false,
            },
            &guard,
            &live,
        );
        assert!(
            bad.reads.get() > 0,
            "source fault must actually be observed"
        );
        assert!(
            matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "mode={mode} result={result:?}"
        );
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}

struct CancelAfterPut<'a> {
    sink: Sink<'a>,
    published: &'a Cell<usize>,
}
impl ValueNodeWriter for CancelAfterPut<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.sink.put_node(id, bytes)?;
        self.published.set(self.published.get() + 1);
        Ok(())
    }
}
#[test]
fn agent_last_real_value_put_cancels_both_channels_and_aborts_successful_prefix() {
    for read_channel in [false, true] {
        let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let extra = RefCell::new(Pages::default());
        let values = Values {
            base: &pages,
            extra: &extra,
        };
        let state = fixture::State::new(&pages, &memory);
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
        let mut reads = io();
        let mut writes = io();
        let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut sink = Sink(&extra);
        let guard = tx.guard();
        assert!(
            change_agent_subtree_scalar_v1(
                AgentSubtreeCellChangeV1 {
                    query: AgentSubtreeQualifiedRequestV1 {
                        cell: request("agent α", AgentFieldV1::PositionX, 0),
                        qualification: &proof
                    },
                    expected: Some(AgentScalarRefV1::I64(i64::MIN)),
                    after: Some(AgentScalarRefV1::I64(5)),
                    max_scalar_bytes: 1024,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut reads,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut writes,
                    cancelled: || false
                },
                &guard,
                &live,
            )
            .unwrap()
        );
        let before = extra.borrow().0.len();
        let published = Cell::new(0);
        let mut sink = CancelAfterPut {
            sink: Sink(&extra),
            published: &published,
        };
        let guard = tx.guard();
        let result = change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: request("agent α", AgentFieldV1::PositionY, 0),
                    qualification: &proof,
                },
                expected: Some(AgentScalarRefV1::I64(0)),
                after: Some(AgentScalarRefV1::I64(7)),
                max_scalar_bytes: 1024,
            },
            &mut overlay!(
                &mut tx,
                &state,
                &values,
                &memory,
                &mut reads,
                &mut keys,
                || read_channel && published.get() > 0
            ),
            &mut ValueMutationContextV1 {
                sink: &mut sink,
                memory: &memory,
                io: &mut writes,
                cancelled: || !read_channel && published.get() > 0,
            },
            &guard,
            &live,
        );
        assert!(
            published.get() > 0 && extra.borrow().0.len() > before,
            "must cancel after a real new page publication"
        );
        assert!(
            matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
            ),
            "read_channel={read_channel} result={result:?}"
        );
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn actual_missing_agent_state_or_value_page_never_returns_structure_or_page_set() {
    for missing_state in [false, true] {
        let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let (mut pages, roots) = fixture::seed(&memory);
        let valid = fixture::qualify(&roots, &pages, &memory).unwrap();
        valid.validate_binding(&roots).unwrap();
        drop(valid);
        let business = roots.state_root(StateNamespaceV1::Business);
        let missing = if missing_state {
            *blake3::Hash::from_hex(&business.node_root)
                .unwrap()
                .as_bytes()
        } else {
            let source = fixture::State::new(&pages, &memory);
            let body = crate::incremental_agent_body::AgentBodyKeyV1::new("agent α").unwrap();
            business.get(body.as_bytes(), &source).unwrap().unwrap()
        };
        assert!(
            pages.0.remove(&missing).is_some(),
            "remove an actual stored page"
        );
        assert!(matches!(
            fixture::qualify(&roots, &pages, &memory),
            Err(ValueTreeError::Invalid(_))
        ));
        let state = fixture::State::new(&pages, &memory);
        let result = mark_agent_subtree_pages_v1(
            &roots,
            &mut crate::incremental_field_read::WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 1 << 20,
                take_state_error: || state.take(),
                cancelled: || false,
            },
            &mut AgentSubtreePagesWorkV1 {
                scan: &mut StateScanBudget {
                    nodes_remaining: 100_000,
                    read_bytes_remaining: 64 << 20,
                    leaves_remaining: 1000,
                },
                structure: &mut AgentSubtreeScanWorkV1 {
                    max_agents: 2,
                    cells_remaining: 1000,
                    max_scalar_bytes: 1 << 20,
                },
                max_pages: 20_000,
                max_originals: 0,
            },
        );
        assert!(
            matches!(result, Err(ValueTreeError::Invalid(_))),
            "missing_state={missing_state}"
        );
    }
}

pub(super) fn assert_only_live_external_dependencies(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
) {
    for max_originals in [0, 1] {
        let state = fixture::State::new(pages, memory);
        let result = mark_agent_subtree_pages_v1(
            roots,
            &mut crate::incremental_field_read::WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: pages,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 1 << 20,
                take_state_error: || state.take(),
                cancelled: || false,
            },
            &mut AgentSubtreePagesWorkV1 {
                scan: &mut StateScanBudget {
                    nodes_remaining: 100_000,
                    read_bytes_remaining: 64 << 20,
                    leaves_remaining: 1000,
                },
                structure: &mut AgentSubtreeScanWorkV1 {
                    max_agents: 2,
                    cells_remaining: 1000,
                    max_scalar_bytes: 1 << 20,
                },
                max_pages: 40_000,
                max_originals,
            },
        );
        eprintln!(
            "agent external original max={max_originals} result={:?}",
            result.as_ref().map(|_| ())
        );
        if max_originals == 0 {
            assert!(matches!(
                result,
                Err(ValueTreeError::ResourceLimited(
                    "agent external original limit"
                ))
            ));
        } else {
            let marked = result.unwrap();
            eprintln!(
                "agent external dependency marked pages={}",
                marked.page_count()
            );
            assert_eq!(marked.original_count(), 1);
            assert_eq!(marked.structure().mailbox_reference_count(), 1);
            let _charge = memory.reserve(32, MemoryClass::Ordinary).unwrap();
            let mut ids = Vec::with_capacity(1);
            marked
                .visit_external_originals::<()>(|id| {
                    ids.push(*id);
                    Ok(())
                })
                .unwrap();
            // References7/8/9 were consumed. Only live ordinal3's opaque ID is
            // reported; even that ID has no accepted-history/Domain assertion.
            assert_eq!(ids, [[10; 32]]);
        }
    }
}
