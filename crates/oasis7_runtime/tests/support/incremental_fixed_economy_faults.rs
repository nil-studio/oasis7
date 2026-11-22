use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::{cell::Cell, sync::Arc};
struct Writer<'a, W> {
    source: &'a mut W,
    active: &'a Cell<bool>,
    mode: usize,
    puts: &'a Cell<usize>,
    root_puts: &'a Cell<usize>,
    stop: &'a Cell<bool>,
}
impl<W: ValueNodeReader> ValueNodeReader for Writer<'_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.source.read_node(id, max, memory, class)
    }
}
impl<W: ValueNodeWriter> ValueNodeWriter for Writer<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let root = bytes.windows(FIELD_MEMBER_DOMAIN.len() + 3).any(|window| {
            window.starts_with(FIELD_MEMBER_DOMAIN) && window[FIELD_MEMBER_DOMAIN.len() + 2] == 10
        });
        self.source.put_node(id, bytes)?;
        if self.active.get() {
            self.puts.set(self.puts.get() + 1);
            if root {
                self.root_puts.set(self.root_puts.get() + 1);
            }
            if matches!(self.mode, 3 | 4) && self.puts.get() == 1
                || matches!(self.mode, 5 | 6) && root
            {
                if matches!(self.mode, 3 | 5) {
                    return Err(ValueTreeError::Io(std::io::Error::other(
                        "fixed economy after physical put",
                    )));
                }
                self.stop.set(true);
            }
        }
        Ok(())
    }
}
impl<W: StatePageWriter> StatePageWriter for Writer<'_, W> {
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancel: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        self.source.put_state_pages_cancelled(delta, memory, cancel)
    }
}
#[test]
fn getter_independently_bounds_noop_and_real_put_failures_abort_prior_success() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("faults"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&store, &memory);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let column = FixedEconomyColumnV1::TotalSupply;
    let base = {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            &roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family =
            qualify_fixed_economy_family_v1(&roots, &mut ctx, &mut scan(), &mut scan(), &mut 1000)
                .unwrap();
        open_qualified_fixed_economy_record_v1(
            FixedEconomyFieldV1::MainTokenSupply,
            &roots,
            &family,
            &mut ctx,
        )
        .unwrap()
    };
    for mode in 0..12 {
        let active = Cell::new(false);
        let getter = Cell::new(false);
        let reads = Cell::new(0usize);
        let writes = Cell::new(0usize);
        let read_n = Cell::new(usize::MAX);
        let write_n = Cell::new(usize::MAX);
        let stop = Cell::new(false);
        let puts = Cell::new(0usize);
        let root_puts = Cell::new(0usize);
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
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
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
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
            cancelled: || {
                if active.get() {
                    reads.set(reads.get() + 1);
                    if !getter.get() && mode == 1 && reads.get() == read_n.get() + 1 {
                        return true;
                    }
                    if getter.get() {
                        writes.set(writes.get() + 1);
                    }
                }
                stop.get()
            },
        };
        let mut backend = store.value_session(physical());
        let mut writer = Writer {
            source: &mut backend,
            active: &active,
            mode,
            puts: &puts,
            root_puts: &root_puts,
            stop: &stop,
        };
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: limits(),
            cancelled: || {
                if active.get() {
                    writes.set(writes.get() + 1);
                    if mode == 2 && writes.get() == write_n.get() + 1 {
                        return true;
                    }
                }
                stop.get()
            },
        };
        let stale = overlay.transaction.guard();
        let proof = change_qualified_fixed_economy_cell_v1(
            FixedEconomyCellChangeV1 {
                column,
                qualification: &base,
                expected: FixedEconomyValueV1::U64(u64::MAX),
                after: FixedEconomyValueV1::U64(0),
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &stale,
            &live,
        )
        .unwrap()
        .unwrap();
        active.set(true);
        getter.set(true);
        let guard = overlay.transaction.guard();
        assert_eq!(
            read_qualified_fixed_economy_overlay_v1(
                FixedEconomyCellRequestV1 {
                    column,
                    qualification: &proof
                },
                &mut overlay,
                &guard,
                &live
            )
            .unwrap(),
            FixedEconomyValueV1::U64(0)
        );
        read_n.set(reads.get());
        write_n.set(writes.get());
        assert!(read_n.get() > 0 && write_n.get() > 0);
        reads.set(0);
        writes.set(0);
        getter.set(false);
        let recipes = std::fs::read_dir(store.root().join("recipes"))
            .unwrap()
            .count();
        if mode == 7 {
            overlay.values.io.nodes_remaining = 0;
        }
        if mode == 8 {
            overlay.key_work.bytes_remaining = 0;
        }
        let after = if matches!(mode, 3..=6) {
            FixedEconomyValueV1::U64(1000 + mode as u64)
        } else if mode == 9 {
            FixedEconomyValueV1::Bool(true)
        } else {
            FixedEconomyValueV1::U64(0)
        };
        let guard = if mode == 11 {
            stale
        } else {
            overlay.transaction.guard()
        };
        let result = change_qualified_fixed_economy_cell_v1(
            FixedEconomyCellChangeV1 {
                column,
                qualification: &proof,
                expected: if mode == 10 {
                    FixedEconomyValueV1::U64(u64::MAX)
                } else {
                    FixedEconomyValueV1::U64(0)
                },
                after,
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
        );
        if mode == 0 {
            assert!(result.unwrap().is_none());
            assert_eq!(reads.get(), read_n.get() + 1);
            assert_eq!(writes.get(), write_n.get() + 1);
            eprintln!(
                "independent getter N read={} write={}; NoOp N+1 read={} write={}",
                read_n.get(),
                write_n.get(),
                reads.get(),
                writes.get()
            );
            let guard = overlay.transaction.guard();
            tx.seal(&guard, &live, 1).unwrap();
            continue;
        }
        match mode {
            1 | 2 | 4 | 6 => assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled)
                        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                            ValueTreeError::Cancelled
                        )))
                ),
                "mode{mode}: {result:?}"
            ),
            3 | 5 => assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Io(_))
                        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                            ValueTreeError::Io(_)
                        )))
                ),
                "mode{mode}: {result:?}"
            ),
            7 | 8 => assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(
                    ValueTreeError::ResourceLimited(_)
                ))
            )),
            9 | 10 => assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            )),
            11 => assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Execution(
                    ExecutionError::StaleGuard
                ))
            )),
            _ => unreachable!(),
        }
        if mode == 1 {
            assert_eq!(reads.get(), read_n.get() + 1);
        }
        if mode == 2 {
            assert_eq!(writes.get(), write_n.get() + 1);
        }
        if matches!(mode, 3..=6) {
            assert_eq!(puts.get(), if mode <= 4 { 1 } else { 3 });
            assert_eq!(root_puts.get(), usize::from(mode >= 5));
            assert!(
                std::fs::read_dir(store.root().join("recipes"))
                    .unwrap()
                    .count()
                    > recipes
            );
        }
        let guard = overlay.transaction.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
