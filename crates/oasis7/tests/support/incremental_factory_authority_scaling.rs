use super::*;
use std::{cell::Cell, sync::Arc};
struct CountState<'a, R> {
    source: &'a R,
    calls: Cell<u64>,
    bytes: Cell<u64>,
}
impl<R: MerkleNodeReader> MerkleNodeReader for CountState<'_, R> {
    type Node = R::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let result = self.source.read_node(id, max)?;
        if let Some(ref page) = result {
            self.calls.set(self.calls.get() + 1);
            self.bytes
                .set(self.bytes.get() + page.as_ref().len() as u64);
        }
        Ok(result)
    }
}
struct CountValue<'a, R> {
    source: &'a R,
    calls: Cell<u64>,
    bytes: Cell<u64>,
}
impl<R: ValueNodeReader> ValueNodeReader for CountValue<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let page = self.source.read_node(id, max, memory, class)?;
        self.calls.set(self.calls.get() + 1);
        self.bytes.set(self.bytes.get() + page.len() as u64);
        Ok(page)
    }
}

use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
#[test]
fn scalar_receiver_cost_excludes_unchanged_site_text_and_allowlist() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let mut observed = Vec::new();
        // Fixed in advance: the complete generic+typed receiver may grow by
        // at most64 calls and64KiB across all three unchanged-state sizes.
        for (text_bytes, list_len) in [(64, 3), (1 << 20, 32), (32 << 20, 64)] {
            let path = directory.0.join(format!("point-pages-{text_bytes}"));
            let before = {
                let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
                let _fixture_text = memory
                    .reserve(text_bytes + list_len * 64, MemoryClass::Ordinary)
                    .unwrap();
                let mut original = source();
                let site = original
                    .factory_site_authorities
                    .get_mut(" outer-site\0界 ")
                    .unwrap();
                site.site_id = "x".repeat(text_bytes);
                site.authorized_agent_ids = (0..list_len)
                    .map(|i| format!("unchanged agent {i}"))
                    .collect();
                let store =
                    IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
                let top = seed_large(&original, &store, &memory);
                let mut roots = RootSetV1::empty("factory-scaling".into(), [0x85; 32]).unwrap();
                roots.state[0] = top;
                roots
            };
            let memory = MemoryBudget::new(24 << 20, 4 << 20).unwrap();
            let _caller = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let parent = ExecutionParent {
                roots: before.clone(),
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
            let top = before
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Business)
                .unwrap();
            let mut ctx = WorldFieldReadContextV1 {
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
                record_bytes: 64 << 20,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let family = qualify_factory_authority_family_v1(
                &before,
                &mut ctx,
                &mut scan(),
                &mut scan(),
                &mut 8192,
                64 << 20,
            )
            .unwrap();
            let key = FactoryAuthorityRecordKeyV1 {
                kind: FactoryAuthorityRecordKindV1::Site,
                id: " outer-site\0界 ",
            };
            let proof = open_qualified_factory_authority_record_v1(key, &before, &family, &mut ctx)
                .unwrap()
                .unwrap();
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [6; 32],
                &state,
                &memory,
                TransactionLimits {
                    max_reads: 160,
                    max_writes: 32,
                    max_facts: 4,
                    max_pending_node_bytes: 4 << 20,
                },
            )
            .unwrap();
            let mut overlay_io = logical();
            let mut overlay_keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut overlay_io,
                },
                key_work: &mut overlay_keys,
                semantic_bytes: 1024,
                record_bytes: 64 << 20,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let mut writer = store.value_session(physical_scope());
            let mut write_io = logical();
            let mut write_keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 64 << 20,
                },
                cancelled: || false,
            };
            let guard = overlay.transaction.guard();
            let proof = change_qualified_factory_authority_cell_v1(
                FactoryAuthorityCellChangeV1 {
                    key,
                    cell: FactoryAuthorityCellV1::AuthorityRevision,
                    qualification: &proof,
                    expected: Some(FactoryAuthorityCellValueRefV1::U64(u64::MAX)),
                    after: Some(FactoryAuthorityCellValueRefV1::U64(18)),
                    max_current_payload_bytes: 8,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let _proof = proof;
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            let state2 = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let counted_state = CountState {
                source: &state2,
                calls: std::cell::Cell::new(0),
                bytes: std::cell::Cell::new(0),
            };
            let values2 = store.value_session(physical_scope());
            let counted_values = CountValue {
                source: &values2,
                calls: std::cell::Cell::new(0),
                bytes: std::cell::Cell::new(0),
            };
            let verifier = TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &counted_state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            };
            let mut receiver = WorldFieldReadContextV1 {
                root: &sealed.record.after.state[0],
                state: &counted_state,
                values: ValueReadContext {
                    source: &counted_values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 64 << 20,
                take_state_error: || state2.take_read_error().map(state_error),
                cancelled: || false,
            };
            let accepted = accept_factory_authority_transition_v1(
                &family,
                &verifier,
                &sealed.record,
                &mut receiver,
                &mut scan(),
                &mut 8192,
                8,
            )
            .unwrap();
            assert_eq!(accepted.record_counts(), [1, 3]);
            let calls = counted_state.calls.get() + counted_values.calls.get();
            let bytes = counted_state.bytes.get() + counted_values.bytes.get();
            eprintln!(
                "factory scalar codec={compression:?} unchanged_text={text_bytes} expanded_members={} state={}/{} value={}/{} total={calls}/{bytes}",
                list_len,
                counted_state.calls.get(),
                counted_state.bytes.get(),
                counted_values.calls.get(),
                counted_values.bytes.get()
            );
            observed.push((calls, bytes));
        }
        let small = observed[0];
        for large in &observed[1..] {
            assert!(
                large.0 <= small.0 + 64,
                "receiver scanned unchanged paths: {observed:?}"
            );
            assert!(
                large.1 <= small.1 + (64 << 10),
                "receiver read unchanged text: {observed:?}"
            );
        }
    }
}

fn seed_large(
    source: &WorldState,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> StateRootV1 {
    let mut gen_scope = physical_scope();
    gen_scope.writes_remaining = 512_000; // finite full-GEN only, at most250 cells *2048
    let mut values = store.value_session(gen_scope);
    let _row_admission = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(8);
    let count = prepare_factory_authority_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 64 << 20,
                record_bytes: 64 << 20,
            },
            cancelled: || false,
        },
        &mut FactoryAuthorityWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 2 << 20,
        },
        &mut |row| {
            assert!(rows.len() < 8);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count, 4);
    assert_eq!(rows.len(), 8);
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let mut writes: Vec<_> = rows
        .iter()
        .map(|row| StateWriteV1 {
            key: row.mutation.key.clone(),
            before: None,
            after: row.mutation.after,
        })
        .collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            writes,
            &state,
            2 << 20,
        )
        .unwrap(),
    );
    values.put_state_pages(&delta, memory).unwrap();
    delta.after.clone()
}
