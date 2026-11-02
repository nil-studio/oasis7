use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::sync::Arc;
#[test]
fn source_carriers_require_caller_pool_and_full_capacity() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("overlay"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let top = seed(&source(), &store, &memory);
    let mut roots = RootSetV1::empty("factory-carrier".into(), [0x86; 32]).unwrap();
    roots.state[0] = top;
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_scope());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let top = roots
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Business)
        .unwrap();
    let key = FactoryAuthorityRecordKeyV1 {
        kind: FactoryAuthorityRecordKindV1::Site,
        id: " outer-site\0界 ",
    };
    let mut context = WorldFieldReadContextV1 {
        root: top,
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
    let family = qualify_factory_authority_family_v1(
        &roots,
        &mut context,
        &mut scan(),
        &mut scan(),
        &mut 8192,
        165,
    )
    .unwrap();
    let base = open_qualified_factory_authority_record_v1(key, &roots, &family, &mut context)
        .unwrap()
        .unwrap();
    for state_pages in [false, true] {
        for undercharged in [false, true] {
            let state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let good = store.value_session(physical_scope());
            let mut backend = store.value_session(physical_scope());
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [6; 32],
                &state,
                &memory,
                TransactionLimits {
                    max_reads: 32,
                    max_writes: 8,
                    max_facts: 4,
                    max_pending_node_bytes: 4 << 20,
                },
            )
            .unwrap();
            let guard = tx.guard();
            let proof = change_qualified_factory_authority_cell_v1(
                FactoryAuthorityCellChangeV1 {
                    key,
                    cell: FactoryAuthorityCellV1::AuthorityRevision,
                    qualification: &base,
                    expected: Some(FactoryAuthorityCellValueRefV1::U64(u64::MAX)),
                    after: Some(FactoryAuthorityCellValueRefV1::U64(18)),
                    max_current_payload_bytes: 165,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &good,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(state_error),
                    cancelled: || false,
                },
                &mut MemberRecordWriteContext {
                    sink: &mut backend,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 165,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let fault = Fault {
                source: &good,
                pool: MemoryBudget::new(4 << 20, 0).unwrap(),
                state: state_pages,
                undercharged,
            };
            let guard = tx.guard();
            let result = read_qualified_factory_authority_cell_overlay_v1(
                FactoryAuthorityCellRequestV1 {
                    key,
                    cell: FactoryAuthorityCellV1::AuthorityRevision,
                    qualification: &proof,
                },
                &mut WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &fault,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(state_error),
                    cancelled: || false,
                },
                165,
                &mut std::io::sink(),
                &guard,
                &live,
            );
            assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
                ),
                "point state={state_pages} undercharged={undercharged}: {result:?}"
            );
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
            // Typed transfer's Merkle reader retains its explicit allocator
            // contract. The Value reader exposes carrier credit directly.
            if !state_pages {
                let transfer = transfer_factory_authority_family_pages_v1(
                    &roots,
                    &mut WorldFieldReadContextV1 {
                        root: &roots.state[0],
                        state: &state,
                        values: ValueReadContext {
                            source: &fault,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut logical(),
                        },
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 1 << 20,
                        },
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || None,
                        cancelled: || false,
                    },
                    &mut FactoryAuthorityPagesWorkV1 {
                        outer_scan: &mut scan(),
                        inner_scan: &mut scan(),
                        cells: &mut 100_000,
                        max_payload_bytes: 165,
                        max_pages: 16_384,
                    },
                    &mut backend,
                );
                assert!(
                    matches!(transfer, Err(ValueTreeError::Invalid(_))),
                    "transfer state={state_pages} undercharged={undercharged}"
                );
            }
            eprintln!(
                "factory source state={state_pages} undercharged={undercharged}: point refused and prefix aborted; value transfer checked separately"
            );
        }
    }
}
struct Fault<'a, R> {
    source: &'a R,
    pool: MemoryBudget,
    state: bool,
    undercharged: bool,
}
impl<R: ValueNodeReader> ValueNodeReader for Fault<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let page = self.source.read_node(id, max, memory, class)?;
        if page.starts_with(b"oasis7.iws.state-page.v1\0") == self.state {
            let charge = if self.undercharged {
                memory.reserve(1, class)?
            } else {
                self.pool.reserve(page.len(), MemoryClass::Ordinary)?
            };
            return Ok(Arc::new(charge.retain(page.to_vec())));
        }
        Ok(page)
    }
}

struct AllocationContract<'a, M> {
    source: &'a M,
    pool: &'a MemoryBudget,
    class: MemoryClass,
}
impl<M: MerkleNodeReader> MerkleNodeReader for AllocationContract<'_, M> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.pool)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        self.source.read_node(id, max)
    }
}
#[test]
fn typed_transfer_rejects_state_reader_foreign_pool_and_wrong_class() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("state-contract"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let top = seed(&source(), &store, &memory);
    let mut roots = RootSetV1::empty("factory-carrier".into(), [0x86; 32]).unwrap();
    roots.state[0] = top;
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_scope());
    let foreign = MemoryBudget::new(4 << 20, 0).unwrap();
    for wrong_class in [false, true] {
        let source = AllocationContract {
            source: &state,
            pool: if wrong_class { &memory } else { &foreign },
            class: if wrong_class {
                MemoryClass::Recovery
            } else {
                MemoryClass::Ordinary
            },
        };
        let mut sink = store.value_session(physical_scope());
        let result = transfer_factory_authority_family_pages_v1(
            &roots,
            &mut WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &source,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut FactoryAuthorityPagesWorkV1 {
                outer_scan: &mut scan(),
                inner_scan: &mut scan(),
                cells: &mut 100_000,
                max_payload_bytes: 165,
                max_pages: 16_384,
            },
            &mut sink,
        );
        assert!(
            matches!(result, Err(ValueTreeError::Invalid(_))),
            "state contract wrong_class={wrong_class}"
        );
    }
}
