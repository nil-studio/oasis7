use super::*;
use std::cell::Cell;
struct FinalPut<'a, W> {
    inner: W,
    store: &'a IncrementalPageStore,
    memory: &'a MemoryBudget,
    armed: &'a Cell<bool>,
    cancel: &'a Cell<bool>,
    published: &'a Cell<u64>,
    fail: bool,
}
impl<W: ValueNodeWriter> ValueNodeWriter for FinalPut<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let root_ref = self.armed.get()
            && bytes
                .windows(b"oasis7.iws.record-root.v1\0".len())
                .any(|b| b == b"oasis7.iws.record-root.v1\0");
        if !root_ref {
            return self.inner.put_node(id, bytes);
        }
        let before = self.store.read_page(
            &id,
            bytes.len(),
            MemoryClass::Ordinary,
            &mut physical_scope(),
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert!(
            matches!(before, Err(oasis7_distfs::incremental_page_store::PageStoreError::Missing(missing)) if missing == id)
        );
        let copy = self
            .memory
            .reserve(bytes.len(), MemoryClass::Ordinary)?
            .retain(bytes.to_vec());
        self.inner.put_node(id, bytes)?;
        let physical = self
            .store
            .read_page(
                &id,
                copy.len(),
                MemoryClass::Ordinary,
                &mut physical_scope(),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(physical.as_slice(), copy.as_slice());
        self.published.set(self.published.get() + 1);
        if self.fail {
            return Err(ValueTreeError::Io(std::io::Error::other(
                "injected failure after final root publication",
            )));
        }
        self.cancel.set(true);
        Ok(())
    }
}
impl<W: StatePageWriter> StatePageWriter for FinalPut<'_, W> {
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        self.inner
            .put_state_pages_cancelled(delta, memory, cancelled)
    }
}
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
#[test]
fn final_real_root_put_cancel_or_failure_never_seals_successful_prefix() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("overlay"), memory.clone(), compression)
                .unwrap();
        let roots = seed(&store, &memory, &source());
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
            .find(|root| root.namespace == StateNamespaceV1::Authorization)
            .unwrap();
        let key = GovernanceRecordKeyV1 {
            kind: GovernanceRecordKindV1::Proposal,
            id: " outer proposal ",
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
            take_state_error: || state.take_read_error().map(super::pages::state_error),
            cancelled: || false,
        };
        let family = qualify_governance_family_v1(
            &roots,
            &mut context,
            &mut work().scan,
            &mut work().scan,
            &mut 8192,
            165,
        )
        .unwrap();
        let base = open_qualified_governance_record_v1(key, &roots, &family, &mut context)
            .unwrap()
            .unwrap();

        for fail in [false, true] {
            let armed = Cell::new(false);
            let cancelled = Cell::new(false);
            let published = Cell::new(0);
            let mut writer = FinalPut {
                inner: store.value_session(physical_scope()),
                store: &store,
                memory: &memory,
                armed: &armed,
                cancel: &cancelled,
                published: &published,
                fail,
            };
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
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
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
                take_state_error: || state.take_read_error().map(super::pages::state_error),
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
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
                cancelled: || cancelled.get(),
            };
            let cell = GovernanceCellV1::Column(GovernanceColumnV1::WinningWeight);
            let guard = read.transaction.guard();
            let proof = change_qualified_governance_cell_v1(
                GovernanceCellChangeV1 {
                    key,
                    cell,
                    qualification: &base,
                    expected: Some(GovernanceValueRefV1::U64(u64::MAX)),
                    after: Some(GovernanceValueRefV1::U64(18)),
                    max_current_payload_bytes: 8,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut read,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            armed.set(true);
            let guard = read.transaction.guard();
            let result = change_qualified_governance_cell_v1(
                GovernanceCellChangeV1 {
                    key,
                    cell,
                    qualification: &proof,
                    expected: Some(GovernanceValueRefV1::U64(18)),
                    after: Some(GovernanceValueRefV1::U64(if fail { 20 } else { 19 })),
                    max_current_payload_bytes: 8,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut read,
                &mut write,
                &guard,
                &live,
            );
            assert_eq!(published.get(), 1);
            if fail {
                match result {
                    Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                        ValueTreeError::Io(error),
                    ))) => assert_eq!(
                        error.to_string(),
                        "injected failure after final root publication"
                    ),
                    other => panic!("wrong finalput failure: {other:?}"),
                }
            } else {
                assert!(matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                        ValueTreeError::Cancelled
                    ))) | Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                ));
            }
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
            eprintln!(
                "governance finalput codec={compression:?} io={fail}: real fresh publication1, prefixAborted"
            );
        }
    }
}
