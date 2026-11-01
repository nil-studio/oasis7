use super::*;
use std::cell::Cell;
macro_rules! context {
    ($tx:expr,$state:expr,$values:expr,$writer:expr,$memory:expr,$cancel:expr,$write_cancel:expr,|$read:ident,$write:ident|$body:block) => {{
        let mut $read = WorldFieldOverlayContextV1 {
            transaction: $tx,
            values: ValueReadContext {
                source: $values,
                memory: $memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || $state.take_read_error().map(typed),
            cancelled: $cancel,
        };
        let mut $write = MemberRecordWriteContext {
            sink: $writer,
            memory: $memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            limits: limits(),
            cancelled: $write_cancel,
        };
        $body
    }};
}
fn parent(roots: RootSetV1) -> ExecutionParent {
    ExecutionParent {
        commit: [4; 32],
        roots,
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    }
}
fn tx_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 512,
        max_writes: 512,
        max_facts: 8,
        max_pending_node_bytes: 4 << 20,
    }
}
fn persist(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
    sealed: &PreparedTransition,
) {
    let verifier = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
    let verified = TransitionVerifier {
        parent,
        operation: [6; 32],
        source: &verifier,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&sealed.record)
    .unwrap();
    assert_eq!(verified.record.after, sealed.record.after.clone());
    let mut values = store.value_session(quota());
    for node in &verified.new_nodes {
        values
            .put_node(
                *blake3::hash(&node.bytes).as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
}
fn cell_change<'a>(
    key: FactoryRecordKeyV1<'a>,
    cell: FactoryCellV1<'a>,
    expected: Option<FactoryValueRefV1<'a>>,
    after: Option<FactoryValueRefV1<'a>>,
) -> FactoryCellChangeV1<'a> {
    FactoryCellChangeV1 {
        key,
        cell,
        expected,
        after,
        max_current_payload_bytes: 1 << 20,
    }
}
#[test]
fn precise_factory_points_coalesce_and_cold_continue_without_large_siblings() {
    let mut work = factory_work();
    use FactoryColumnV1 as C;
    use FactoryValueRefV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (old, after) = {
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let _caller = memory.reserve(3 << 20, MemoryClass::Ordinary).unwrap();
            let (mut source, _, _) = source();
            source
                .factories
                .get_mut("outer factory")
                .unwrap()
                .spec
                .display_name = "q".repeat(1 << 20);
            source
                .factories
                .get_mut("outer factory")
                .unwrap()
                .production
                .current_blocker_detail = Some("b".repeat(1 << 20));
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let parent = parent(seed(&store, &memory, &source, None));
            drop(source);
            let live = parent.fence().unwrap();
            let state = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
            let values = store.value_session(quota());
            let mut writer = store.value_session(quota());
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [6; 32],
                &state,
                &memory,
                TransactionLimits {
                    max_reads: 8,
                    max_writes: 4,
                    ..tx_limits()
                },
            )
            .unwrap();
            context!(
                &mut tx,
                state,
                &values,
                &mut writer,
                &memory,
                || false,
                || false,
                |read, write| {
                    // The requested scalar cannot materialize either 1MiB sibling: its
                    // entire record/read limits and IO quota are deliberately smaller.
                    read.record_bytes = 1024;
                    read.semantic_bytes = 1024;
                    read.values.io.read_bytes_remaining = 512 << 10;
                    read.values.io.nodes_remaining = 6_000;
                    write.limits.payload_bytes =
                        oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES as u64;
                    write.limits.record_bytes = 1024;
                    for (key, cell, before, after) in [
                        (
                            FactoryRecordKeyV1::Factory("outer factory"),
                            FactoryCellV1::Column(C::FactoryDurabilityPpm),
                            V::Signed(i64::MIN),
                            V::Signed(7),
                        ),
                        (
                            FactoryRecordKeyV1::Factory("outer factory"),
                            FactoryCellV1::Column(C::FactoryDurabilityPpm),
                            V::Signed(7),
                            V::Signed(13),
                        ),
                        (
                            FactoryRecordKeyV1::Build(u64::MAX),
                            FactoryCellV1::Column(C::BuildReadyAt),
                            V::Unsigned(u64::MAX),
                            V::Unsigned(42),
                        ),
                    ] {
                        let guard = read.transaction.guard();
                        let mut change = cell_change(key, cell, Some(before), Some(after));
                        change.max_current_payload_bytes = 8;
                        assert!(
                            change_factory_cell_overlay_v1(
                                change, &mut read, &mut write, &mut work, &guard, &live
                            )
                            .unwrap()
                        );
                    }
                    eprintln!(
                        "factory precise 3changes inner_nodes={} inner_read_bytes={}",
                        6000 - read.values.io.nodes_remaining,
                        (512 << 10) - read.values.io.read_bytes_remaining
                    );
                }
            );
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.reads.len(), 6);
            assert_eq!(sealed.record.writes.len(), 2);
            persist(&store, &memory, &parent, &sealed);
            (parent.roots, sealed.record.after.clone())
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let state = store.state_session(physical(), class).unwrap();
            let values = store.value_session(physical());
            for (roots, expected) in [(&old, i64::MIN), (&after, 13)] {
                let mut context = WorldFieldReadContextV1 {
                    root: &roots.state[0],
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 1024,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                assert_eq!(
                    read_factory_cell_v1(
                        FactoryRecordKeyV1::Factory("outer factory"),
                        FactoryCellV1::Column(C::FactoryDurabilityPpm),
                        &mut context,
                        8,
                        &mut io::sink()
                    )
                    .unwrap(),
                    FactoryCellReadV1::Value(FactoryValueV1::Signed(expected))
                );
            }
        }
    }
}
#[test]
fn explicit_factory_replacement_preserves_optional_shapes_and_vector_members() {
    let mut work = factory_work();
    use FactoryCollectionV1 as L;
    use FactoryColumnV1 as C;
    use FactoryValueRefV1 as V;
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let dir = Directory::new();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (source, _, _) = source();
    let parent = parent(seed(&store, &memory, &source, None));
    let live = parent.fence().unwrap();
    // Four complete before-record proofs and four explicit after-record builds
    // share continuous state/value sessions. Each channel admits the eight
    // explicit scopes at the original 256MiB per-scope bound; neither channel
    // resets per record. Point/cold/production limits are unchanged.
    let mut complete_scope_io = quota();
    complete_scope_io.read_bytes_remaining = 8 * (256 << 20);
    complete_scope_io.reads_remaining = 8 * 1_000_000;
    let complete_before = complete_scope_io.clone();
    let state = store
        .state_session(complete_scope_io.clone(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(complete_scope_io);
    let mut writer_io = quota();
    writer_io.reads_remaining = complete_before.reads_remaining;
    let writer_before = writer_io.clone();
    let mut writer = store.value_session(writer_io);
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, tx_limits()).unwrap();
    let mut factory = source.factories["outer factory"].clone();
    let mut job = source.pending_factory_builds[&u64::MAX].clone();
    let fk = FactoryRecordKeyV1::Factory("outer factory");
    let jk = FactoryRecordKeyV1::Build(u64::MAX);
    context!(
        &mut tx,
        state,
        &values,
        &mut writer,
        &memory,
        || false,
        || false,
        |read, write| {
            // Eight explicit complete scopes share these allowances; do not
            // reset per record or broaden any point/cold byte allowance.
            read.values.io.nodes_remaining = 8 * 100_000;
            write.io.nodes_remaining = 8 * 100_000;
            for (key, cell, before, after) in [
                (
                    fk,
                    FactoryCellV1::TextItem(L::SpecTags, 1),
                    V::Text("dup"),
                    V::Text("changed"),
                ),
                (
                    fk,
                    FactoryCellV1::StackItem(L::SpecBuildCost, 1, FactoryStackColumnV1::Amount),
                    V::Signed(0),
                    V::Signed(-9),
                ),
                (
                    jk,
                    FactoryCellV1::Balance(L::ObligationBalancesBefore, "ore"),
                    V::Signed(i64::MIN),
                    V::Signed(7),
                ),
            ] {
                let guard = read.transaction.guard();
                assert!(
                    change_factory_cell_overlay_v1(
                        cell_change(key, cell, Some(before), Some(after)),
                        &mut read,
                        &mut write,
                        &mut work,
                        &guard,
                        &live
                    )
                    .unwrap()
                );
            }
            factory.spec.tags[1] = "changed".into();
            factory.spec.build_cost[1].amount = -9;
            job.construction_power_obligation
                .as_mut()
                .unwrap()
                .material_balances_before
                .as_mut()
                .unwrap()
                .insert("ore".into(), 7);
            let mut no_snapshot = factory.clone();
            no_snapshot.production.last_completed_canonical_snapshot = None;
            let guard = read.transaction.guard();
            assert!(
                replace_factory_record_overlay_v1(
                    FactoryRecordChangeV1 {
                        key: fk,
                        expected: Some(FactoryRecordRefV1::Factory(&factory)),
                        after: Some(FactoryRecordRefV1::Factory(&no_snapshot)),
                        max_current_payload_bytes: 1 << 20
                    },
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live
                )
                .inspect_err(|error| {
                    eprintln!(
                        "complete replacement error={error:?} state={:?} value={:?} writer={:?} logical_read={:?} logical_write={:?}",
                        state.remaining_io(),
                        values.remaining_io(),
                        write.sink.remaining_io(),
                        read.values.io,
                        write.io
                    );
                })
                .unwrap()
            );
            assert_eq!(
                read_factory_cell_overlay_v1(
                    fk,
                    FactoryCellV1::Collection(L::SnapshotProduce),
                    &mut read,
                    8,
                    &mut io::sink()
                )
                .unwrap(),
                FactoryCellReadV1::Value(FactoryValueV1::Unit)
            );
            let guard = read.transaction.guard();
            assert!(
                replace_factory_record_overlay_v1(
                    FactoryRecordChangeV1 {
                        key: fk,
                        expected: Some(FactoryRecordRefV1::Factory(&no_snapshot)),
                        after: Some(FactoryRecordRefV1::Factory(&factory)),
                        max_current_payload_bytes: 1 << 20
                    },
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live
                )
                .inspect_err(|error| {
                    eprintln!(
                        "complete replacement error={error:?} state={:?} value={:?} writer={:?} logical_read={:?} logical_write={:?}",
                        state.remaining_io(),
                        values.remaining_io(),
                        write.sink.remaining_io(),
                        read.values.io,
                        write.io
                    );
                })
                .unwrap()
            );
            assert_eq!(
                read_factory_cell_overlay_v1(
                    fk,
                    FactoryCellV1::Collection(L::SnapshotProduce),
                    &mut read,
                    8,
                    &mut io::sink()
                )
                .unwrap(),
                FactoryCellReadV1::Value(FactoryValueV1::Unsigned(0))
            );
            let mut none_map = job.clone();
            none_map
                .construction_power_obligation
                .as_mut()
                .unwrap()
                .material_balances_after = None;
            let guard = read.transaction.guard();
            assert!(
                replace_factory_record_overlay_v1(
                    FactoryRecordChangeV1 {
                        key: jk,
                        expected: Some(FactoryRecordRefV1::Build(&job)),
                        after: Some(FactoryRecordRefV1::Build(&none_map)),
                        max_current_payload_bytes: 1 << 20
                    },
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live
                )
                .inspect_err(|error| {
                    eprintln!(
                        "complete replacement error={error:?} state={:?} value={:?} writer={:?} logical_read={:?} logical_write={:?}",
                        state.remaining_io(),
                        values.remaining_io(),
                        write.sink.remaining_io(),
                        read.values.io,
                        write.io
                    );
                })
                .unwrap()
            );
            assert_eq!(
                read_factory_cell_overlay_v1(
                    jk,
                    FactoryCellV1::Collection(L::ObligationBalancesAfter),
                    &mut read,
                    8,
                    &mut io::sink()
                )
                .unwrap(),
                FactoryCellReadV1::Value(FactoryValueV1::Unit)
            );
            let guard = read.transaction.guard();
            assert!(
                replace_factory_record_overlay_v1(
                    FactoryRecordChangeV1 {
                        key: jk,
                        expected: Some(FactoryRecordRefV1::Build(&none_map)),
                        after: Some(FactoryRecordRefV1::Build(&job)),
                        max_current_payload_bytes: 1 << 20
                    },
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live
                )
                .inspect_err(|error| {
                    eprintln!(
                        "complete replacement error={error:?} state={:?} value={:?} writer={:?} logical_read={:?} logical_write={:?}",
                        state.remaining_io(),
                        values.remaining_io(),
                        write.sink.remaining_io(),
                        read.values.io,
                        write.io
                    );
                })
                .unwrap()
            );
            assert_eq!(
                read_factory_cell_overlay_v1(
                    jk,
                    FactoryCellV1::Collection(L::ObligationBalancesAfter),
                    &mut read,
                    8,
                    &mut io::sink()
                )
                .unwrap(),
                FactoryCellReadV1::Value(FactoryValueV1::Unsigned(0))
            );
            let guard = read.transaction.guard();
            assert!(
                !change_factory_cell_overlay_v1(
                    cell_change(
                        jk,
                        FactoryCellV1::Column(C::BuildJobId),
                        Some(V::Unsigned(17)),
                        Some(V::Unsigned(17))
                    ),
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live
                )
                .unwrap()
            );
            eprintln!(
                "four whole replacements logical_read_nodes={} logical_read_bytes={} logical_write_nodes={} logical_write_read_bytes={}",
                800_000 - read.values.io.nodes_remaining,
                logical().read_bytes_remaining - read.values.io.read_bytes_remaining,
                800_000 - write.io.nodes_remaining,
                logical().read_bytes_remaining - write.io.read_bytes_remaining
            );
        }
    );
    for (channel, remaining) in [
        ("state", state.remaining_io().unwrap()),
        ("value", values.remaining_io().unwrap()),
    ] {
        eprintln!(
            "four whole replacements channel={channel} physical_reads={} physical_bytes={}",
            complete_before.reads_remaining - remaining.reads_remaining,
            complete_before.read_bytes_remaining - remaining.read_bytes_remaining
        );
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(sealed.record.writes.len(), 2);
    persist(&store, &memory, &parent, &sealed);
    let writer_remaining = writer.remaining_io().unwrap();
    eprintln!(
        "four whole replacements channel=writer physical_reads={} physical_bytes={}",
        writer_before.reads_remaining - writer_remaining.reads_remaining,
        writer_before.read_bytes_remaining - writer_remaining.read_bytes_remaining
    );
}

#[test]
fn factory_noops_check_final_read_and_write_cancellation_after_public_resolvers() {
    let mut work = factory_work();
    use FactoryColumnV1 as C;
    use FactoryValueRefV1 as V;
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let dir = Directory::new();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (source, _, _) = source();
    let parent = parent(seed(&store, &memory, &source, None));
    let live = parent.fence().unwrap();
    let mut current_factory = source.factories["outer factory"].clone();
    current_factory.durability_ppm = 7;
    let fk = FactoryRecordKeyV1::Factory("outer factory");
    let jk = FactoryRecordKeyV1::Build(u64::MAX);
    let mut traces = Vec::with_capacity(12);
    for case in 0..6 {
        let mut resolver = 0;
        for phase in 0..3 {
            let state = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
            let values = store.value_session(quota());
            let mut writer = store.value_session(quota());
            let mut tx =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, tx_limits())
                    .unwrap();
            let read_polls = Cell::new(0usize);
            let write_polls = Cell::new(0usize);
            let armed = Cell::new(false);
            let threshold = if phase == 0 { usize::MAX } else { resolver + 1 };
            let result;
            context!(
                &mut tx,
                state,
                &values,
                &mut writer,
                &memory,
                || {
                    if armed.get() {
                        let n = read_polls.get() + 1;
                        read_polls.set(n);
                        phase == 1 && n >= threshold
                    } else {
                        false
                    }
                },
                || {
                    if armed.get() {
                        let n = write_polls.get() + 1;
                        write_polls.set(n);
                        phase == 2 && n >= threshold
                    } else {
                        false
                    }
                },
                |read, write| {
                    let guard = read.transaction.guard();
                    assert!(
                        change_factory_cell_overlay_v1(
                            cell_change(
                                fk,
                                FactoryCellV1::Column(C::FactoryDurabilityPpm),
                                Some(V::Signed(i64::MIN)),
                                Some(V::Signed(7))
                            ),
                            &mut read,
                            &mut write,
                            &mut work,
                            &guard,
                            &live
                        )
                        .unwrap()
                    );
                    assert!(write.io.write_bytes_remaining < logical().write_bytes_remaining);
                    armed.set(true);
                    let (key, cell, expected) = match case {
                        0 => (
                            fk,
                            FactoryCellV1::Column(C::ProductionStatus),
                            V::Unsigned(3),
                        ),
                        1 => (
                            fk,
                            FactoryCellV1::Column(C::SpecDisplayName),
                            V::Text(source.factories["outer factory"].spec.display_name.as_str()),
                        ),
                        2 => (
                            jk,
                            FactoryCellV1::Balance(
                                FactoryCollectionV1::ObligationBalancesBefore,
                                "ore",
                            ),
                            V::Signed(i64::MIN),
                        ),
                        _ => (
                            jk,
                            FactoryCellV1::Column(C::BuildReadyAt),
                            V::Unsigned(u64::MAX),
                        ),
                    };
                    if phase == 0 {
                        if case >= 4 {
                            let (key, value) = if case == 4 {
                                (fk, FactoryRecordRefV1::Factory(&current_factory))
                            } else {
                                (
                                    jk,
                                    FactoryRecordRefV1::Build(
                                        &source.pending_factory_builds[&u64::MAX],
                                    ),
                                )
                            };
                            let guard = read.transaction.guard();
                            verify_factory_record_overlay_v1(
                                key,
                                value,
                                &mut read,
                                &mut work,
                                1 << 20,
                                &guard,
                                &live,
                            )
                            .unwrap();
                        } else {
                            read_factory_cell_overlay_v1(
                                key,
                                cell,
                                &mut read,
                                1 << 20,
                                &mut io::sink(),
                            )
                            .unwrap();
                        }
                        resolver = read_polls.get();
                        assert!(resolver > 0);
                        result = Ok(false);
                    } else {
                        let guard = read.transaction.guard();
                        result = if case >= 4 {
                            let (key, value) = if case == 4 {
                                (fk, FactoryRecordRefV1::Factory(&current_factory))
                            } else {
                                (
                                    jk,
                                    FactoryRecordRefV1::Build(
                                        &source.pending_factory_builds[&u64::MAX],
                                    ),
                                )
                            };
                            replace_factory_record_overlay_v1(
                                FactoryRecordChangeV1 {
                                    key,
                                    expected: Some(value),
                                    after: Some(value),
                                    max_current_payload_bytes: 1 << 20,
                                },
                                &mut read,
                                &mut write,
                                &mut work,
                                &guard,
                                &live,
                            )
                        } else {
                            change_factory_cell_overlay_v1(
                                cell_change(key, cell, Some(expected), Some(expected)),
                                &mut read,
                                &mut write,
                                &mut work,
                                &guard,
                                &live,
                            )
                        };
                    }
                }
            );
            let guard = tx.guard();
            let seal = tx.seal(&guard, &live, 1);
            if phase == 0 {
                assert_eq!(seal.unwrap().record.writes.len(), 1);
            } else {
                let cancelled = matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                );
                let aborted = matches!(seal, Err(ExecutionError::Aborted));
                let polls = if phase == 1 {
                    read_polls.get()
                } else {
                    write_polls.get()
                };
                eprintln!(
                    "factory noop case={case} write_channel={} public_resolver={resolver} selected_polls={polls} cancelled={cancelled} aborted={aborted}",
                    phase == 2
                );
                traces.push((case, phase, polls == resolver + 1, cancelled, aborted));
            }
        }
    }
    assert!(
        traces
            .iter()
            .all(|(_, _, boundary, cancelled, aborted)| *boundary && *cancelled && *aborted),
        "{traces:?}"
    );
}
struct CancelAfterPut<'a, W> {
    inner: &'a mut W,
    armed: &'a Cell<bool>,
    cancelled: &'a Cell<bool>,
    put: &'a Cell<bool>,
}
impl<W: ValueNodeWriter> StatePageWriter for CancelAfterPut<'_, W> {}
impl<W: ValueNodeReader> ValueNodeReader for CancelAfterPut<'_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.inner.read_node(id, max, memory, class)
    }
}
impl<W: ValueNodeWriter> ValueNodeWriter for CancelAfterPut<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        if self.armed.get() {
            self.put.set(true);
            self.cancelled.set(true);
        }
        Ok(())
    }
}
#[test]
fn factory_failures_after_successful_prefix_poison_guard_pool_type_quota_and_real_put() {
    let mut work = factory_work();
    use FactoryColumnV1 as C;
    use FactoryValueRefV1 as V;
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let alien = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let dir = Directory::new();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (source, _, _) = source();
    let parent = parent(seed(&store, &memory, &source, None));
    let live = parent.fence().unwrap();
    let fk = FactoryRecordKeyV1::Factory("outer factory");
    for case in 0..13 {
        let state = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
        let values = store.value_session(quota());
        let mut output = store.value_session(quota());
        let armed = Cell::new(false);
        let cancelled = Cell::new(false);
        let put = Cell::new(false);
        let mut writer = CancelAfterPut {
            inner: &mut output,
            armed: &armed,
            cancelled: &cancelled,
            put: &put,
        };
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, tx_limits())
                .unwrap();
        let stale = tx.guard();
        let mut wrong_live = live;
        if case == 2 {
            wrong_live.authority.epoch += 1;
        }
        context!(
            &mut tx,
            state,
            &values,
            &mut writer,
            &memory,
            || cancelled.get(),
            || cancelled.get(),
            |read, write| {
                let guard = read.transaction.guard();
                assert!(
                    change_factory_cell_overlay_v1(
                        cell_change(
                            fk,
                            FactoryCellV1::Column(C::FactoryDurabilityPpm),
                            Some(V::Signed(i64::MIN)),
                            Some(V::Signed(7))
                        ),
                        &mut read,
                        &mut write,
                        &mut work,
                        &guard,
                        &live
                    )
                    .unwrap()
                );
                if case == 3 {
                    read.values.class = MemoryClass::Recovery;
                }
                if case == 4 {
                    write.memory = &alien;
                }
                if case == 5 {
                    read.values.io.nodes_remaining = 0;
                }
                if case == 6 {
                    write.limits.payload_bytes = 0;
                }
                if case == 7 {
                    read.key_work.bytes_remaining = 0;
                }
                if case == 8 {
                    cancelled.set(true);
                }
                if case == 9 {
                    armed.set(true);
                }
                let guard = if case == 1 {
                    stale
                } else {
                    read.transaction.guard()
                };
                let (cell, before, after) = match case {
                    10 => (
                        FactoryCellV1::Column(C::BuildReadyAt),
                        V::Unsigned(0),
                        V::Unsigned(1),
                    ),
                    11 => (
                        FactoryCellV1::Column(C::ProductionStatus),
                        V::Unsigned(3),
                        V::Unsigned(4),
                    ),
                    12 => (
                        FactoryCellV1::Column(C::SpecTier),
                        V::Unsigned(255),
                        V::Unsigned(256),
                    ),
                    _ => (
                        FactoryCellV1::Column(C::ProductionCompletedJobs),
                        V::Unsigned(if case == 0 { 1 } else { u64::MAX }),
                        V::Unsigned(1),
                    ),
                };
                let result = change_factory_cell_overlay_v1(
                    cell_change(fk, cell, Some(before), Some(after)),
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &wrong_live,
                );
                assert!(result.is_err(), "case {case}");
                if case == 9 {
                    assert!(put.get(), "actual provisional put");
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                            | Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                                ValueTreeError::Cancelled
                            )))
                    ));
                }
            }
        );
        let guard = tx.guard();
        assert!(
            matches!(tx.seal(&guard, &live, 1), Err(ExecutionError::Aborted)),
            "case {case}"
        );
    }
}

#[test]
fn subset_expected_map_cannot_clear_or_delete_a_valid_build_record() {
    let mut work = factory_work();
    use FactoryColumnV1 as C;
    use FactoryValueRefV1 as V;
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let dir = Directory::new();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (source, _, _) = source();
    let parent = parent(seed(&store, &memory, &source, None));
    let live = parent.fence().unwrap();
    let original = &source.pending_factory_builds[&u64::MAX];
    assert_eq!(
        original
            .construction_power_obligation
            .as_ref()
            .unwrap()
            .material_balances_before
            .as_ref()
            .unwrap()
            .len(),
        2
    );
    let mut subset = original.clone();
    subset
        .construction_power_obligation
        .as_mut()
        .unwrap()
        .material_balances_before
        .as_mut()
        .unwrap()
        .remove("");
    let mut cleared = subset.clone();
    cleared
        .construction_power_obligation
        .as_mut()
        .unwrap()
        .material_balances_before = Some(BTreeMap::new());
    for delete_record in [false, true] {
        let state = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
        let values = store.value_session(quota());
        let mut writer = store.value_session(quota());
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, tx_limits())
                .unwrap();
        context!(
            &mut tx,
            state,
            &values,
            &mut writer,
            &memory,
            || false,
            || false,
            |read, write| {
                let guard = read.transaction.guard();
                assert!(
                    change_factory_cell_overlay_v1(
                        cell_change(
                            FactoryRecordKeyV1::Factory("outer factory"),
                            FactoryCellV1::Column(C::FactoryDurabilityPpm),
                            Some(V::Signed(i64::MIN)),
                            Some(V::Signed(7))
                        ),
                        &mut read,
                        &mut write,
                        &mut work,
                        &guard,
                        &live
                    )
                    .unwrap()
                );
                let before = write.io.write_bytes_remaining;
                let guard = read.transaction.guard();
                let result = replace_factory_record_overlay_v1(
                    FactoryRecordChangeV1 {
                        key: FactoryRecordKeyV1::Build(u64::MAX),
                        expected: Some(FactoryRecordRefV1::Build(&subset)),
                        after: if delete_record {
                            None
                        } else {
                            Some(FactoryRecordRefV1::Build(&cleared))
                        },
                        max_current_payload_bytes: 1 << 20,
                    },
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live,
                );
                assert!(matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
                        "factory expected record"
                    )))
                ));
                assert_eq!(
                    write.io.write_bytes_remaining, before,
                    "the inconsistent complete expectation fails before any replacement output"
                );
            }
        );
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
#[test]
fn factory_genesis_prefixes_never_report_success_after_quota_sink_or_put_cancellation() {
    let mut work = factory_work();
    let (source, _, _) = source();
    for case in 0..6 {
        let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
        let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let dir = Directory::new();
        let store = IncrementalPageStore::create(
            &dir.0.join("pages"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let mut output = store.value_session(quota());
        let cancelled = Cell::new(case == 4);
        let armed = Cell::new(case == 5);
        let put = Cell::new(false);
        let mut writer = CancelAfterPut {
            inner: &mut output,
            armed: &armed,
            cancelled: &cancelled,
            put: &put,
        };
        let emitted = Cell::new(0usize);
        let result = prepare_factory_records_genesis_v1(
            &source,
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                },
                limits: limits(),
                cancelled: || cancelled.get(),
            },
            &mut if case <= 1 { case as u64 } else { 256 },
            &mut work,
            &mut |_| {
                emitted.set(emitted.get() + 1);
                if case == 2 {
                    return Err(ValueTreeError::Invalid("injected factory sink"));
                }
                if case == 3 {
                    cancelled.set(true);
                }
                Ok(())
            },
        );
        assert!(result.is_err(), "case {case}");
        assert!(emitted.get() <= 1);
        if case == 5 {
            assert!(put.get());
        }
        if case == 0 || case == 4 || case == 5 {
            assert_eq!(emitted.get(), 0);
        }
    }
}

#[path = "incremental_factory_review_regressions.rs"]
mod review_regressions;
