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
pub(super) fn final_real_root_put_cancel_or_failure_never_seals_the_successful_prefix() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let dir = Directory::new();
        let store = IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), compression)
            .unwrap();
        let roots = seed(&store, &memory, &source());
        let parent = parent(roots);
        let live = parent.fence().unwrap();
        for fail in [false, true] {
            let state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical_scope());
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
                [0x95; 32],
                &state,
                &memory,
                tx_limits(),
            )
            .unwrap();
            let result;
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("logistics final put state"))
                    },
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192,
                    },
                    cancelled: || cancelled.get(),
                };
                let mut budget = work();
                let key = LogisticsRecordKeyV1 {
                    kind: LogisticsRecordKindV1::Route,
                    id: " outer/路线 ",
                };
                let cell = LogisticsCellV1::Column(LogisticsColumnV1::DistanceKm);
                let guard = read.transaction.guard();
                assert!(
                    change_logistics_cell_overlay_v1(
                        LogisticsCellChangeV1 {
                            key,
                            cell,
                            expected: Some(LogisticsValueRefV1::Signed(i64::MIN)),
                            after: Some(LogisticsValueRefV1::Signed(17)),
                            max_current_payload_bytes: 8
                        },
                        &mut read,
                        &mut write,
                        &mut budget,
                        &guard,
                        &live
                    )
                    .unwrap()
                );
                assert!(write.io.write_bytes_remaining < logical().write_bytes_remaining);
                armed.set(true);
                let guard = read.transaction.guard();
                result = change_logistics_cell_overlay_v1(
                    LogisticsCellChangeV1 {
                        key,
                        cell,
                        expected: Some(LogisticsValueRefV1::Signed(17)),
                        after: Some(LogisticsValueRefV1::Signed(if fail { 19 } else { 18 })),
                        max_current_payload_bytes: 8,
                    },
                    &mut read,
                    &mut write,
                    &mut budget,
                    &guard,
                    &live,
                );
            }
            assert_eq!(
                published.get(),
                1,
                "the fresh final RootRef page was actually written"
            );
            if fail {
                match result {
                    Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                        ValueTreeError::Io(error),
                    ))) => assert_eq!(
                        error.to_string(),
                        "injected failure after final root publication"
                    ),
                    other => panic!("wrong final-put failure: {other:?}"),
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
                "logistics final root put codec={compression:?} fail={fail}: published=1 aborted"
            );
        }
    }
}
