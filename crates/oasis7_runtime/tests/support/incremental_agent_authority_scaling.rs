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
fn state_error(e: oasis7_distfs::incremental_page_store::PageStoreError) -> ValueTreeError {
    use oasis7_distfs::incremental_page_store::PageStoreError as E;
    match e {
        E::Memory(error) => ValueTreeError::Memory(error),
        E::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        E::Cancelled => ValueTreeError::Cancelled,
        E::Io(error) => ValueTreeError::Io(error),
        E::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "agent authority physical page missing",
        )),
        E::Corrupt(reason) => ValueTreeError::Invalid(reason),
        E::UnsupportedPlatform => ValueTreeError::Invalid("agent authority physical platform"),
    }
}
#[test]
fn scalar_receiver_cost_excludes_unchanged_intent_text() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("point-pages"),
            memory.clone(),
            compression,
        )
        .unwrap();
        let mut observed = Vec::new();
        for text_bytes in [8_000, 420_000] {
            let mut original = source();
            original
                .agent_intent_ledger
                .get_mut(" outer intent ")
                .unwrap()
                .summary = "x".repeat(text_bytes);
            let before = seed(&store, &memory, &original);
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
                record_bytes: 2 << 20,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let family = qualify_agent_authority_family_v1(
                &before,
                &mut ctx,
                &mut work().scan,
                &mut work().scan,
                &mut 8192,
                1 << 20,
            )
            .unwrap();
            let mut proofs = Vec::new();
            for key in [
                AgentAuthorityRecordKeyV1 {
                    kind: AgentAuthorityRecordKindV1::IntentLedger,
                    id: " outer intent ",
                },
                AgentAuthorityRecordKeyV1 {
                    kind: AgentAuthorityRecordKindV1::AgentLocation,
                    id: " outer agent ",
                },
                AgentAuthorityRecordKeyV1 {
                    kind: AgentAuthorityRecordKindV1::LocationAnchor,
                    id: "",
                },
            ] {
                ctx.root = before
                    .state
                    .iter()
                    .find(|r| r.namespace == key.kind.namespace())
                    .unwrap();
                proofs.push(
                    open_qualified_agent_authority_record_v1(key, &before, &family, &mut ctx)
                        .unwrap()
                        .unwrap(),
                );
            }
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
                record_bytes: 2 << 20,
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
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            };
            let key = AgentAuthorityRecordKeyV1 {
                kind: AgentAuthorityRecordKindV1::IntentLedger,
                id: " outer intent ",
            };
            let guard = overlay.transaction.guard();
            let proof = change_qualified_agent_authority_cell_v1(
                AgentAuthorityCellChangeV1 {
                    key,
                    cell: AgentAuthorityCellV1::Column(AgentAuthorityColumnV1::Intent(
                        IntentFieldV1::EventSeq,
                    )),
                    qualification: &proofs[0],
                    expected: AgentAuthorityValueRefV1::U64(17),
                    after: AgentAuthorityValueRefV1::U64(18),
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
                record_bytes: 2 << 20,
                take_state_error: || state2.take_read_error().map(state_error),
                cancelled: || false,
            };
            let accepted = accept_agent_authority_transition_v1(
                &family,
                &verifier,
                &sealed.record,
                &mut receiver,
                &mut work().scan,
                &mut 8192,
                8,
            )
            .unwrap();
            assert_eq!(accepted.record_counts(), [1, 1, 1]);
            let calls = counted_state.calls.get() + counted_values.calls.get();
            let bytes = counted_state.bytes.get() + counted_values.bytes.get();
            eprintln!(
                "agent scalar codec={compression:?} unchanged_text={text_bytes} state={}/{} value={}/{} total={calls}/{bytes}",
                counted_state.calls.get(),
                counted_state.bytes.get(),
                counted_values.calls.get(),
                counted_values.bytes.get()
            );
            observed.push((calls, bytes));
        }
        let [small, large] = observed.as_slice() else {
            unreachable!()
        };
        assert!(
            large.0 <= small.0 + 64,
            "receiver scanned unchanged text paths: {observed:?}"
        );
        assert!(
            large.1 <= small.1 + (64 << 10),
            "receiver read unchanged text: {observed:?}"
        );
    }
}
