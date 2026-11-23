use super::*;
use std::{cell::Cell, sync::Arc};
pub(super) fn qualify_full(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
) -> ReservedValue<LogisticsFamilyQualificationV1> {
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_scope());
    qualify_logistics_family_v1(
        roots,
        &mut LogisticsFamilyReadContextV1 {
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            max_payload_bytes: 1 << 20,
            outer_scan: &mut StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 256 << 20,
                leaves_remaining: 10_000,
            },
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|_| ValueTreeError::Invalid("logistics fixture full qualification"))
            },
            cancelled: || false,
        },
        &mut work(),
    )
    .unwrap()
}
macro_rules! context {
    ($tx:expr,$state:expr,$values:expr,$writer:expr,$memory:expr,$read_cancel:expr,$write_cancel:expr,|$read:ident,$write:ident|$body:block) => {{
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
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || {
                $state
                    .take_read_error()
                    .map(|_| ValueTreeError::Invalid("logistics fixture state"))
            },
            cancelled: $read_cancel,
        };
        let mut $write = MemberRecordWriteContext {
            sink: $writer,
            memory: $memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: $write_cancel,
        };
        $body
    }};
}
macro_rules! prefix {
    ($read:ident,$write:ident,$work:expr,$live:expr) => {{
        let guard = $read.transaction.guard();
        assert!(
            change_logistics_cell_overlay_v1(
                LogisticsCellChangeV1 {
                    key: LogisticsRecordKeyV1 {
                        kind: LogisticsRecordKindV1::Route,
                        id: " outer/路线 "
                    },
                    cell: LogisticsCellV1::Column(LogisticsColumnV1::DistanceKm),
                    expected: Some(LogisticsValueRefV1::Signed(i64::MIN)),
                    after: Some(LogisticsValueRefV1::Signed(17)),
                    max_current_payload_bytes: 8
                },
                &mut $read,
                &mut $write,
                $work,
                &guard,
                $live
            )
            .unwrap()
        );
        assert!($write.io.write_bytes_remaining < logical().write_bytes_remaining);
    }};
}
pub(super) fn noops_cancel_after_public_resolver_and_poison_real_prefix() {
    use LogisticsColumnV1 as C;
    use LogisticsRecordKindV1 as K;
    use LogisticsValueRefV1 as V;
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let dir = Directory::new();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let mut source = source();
    source
        .logistics_routes
        .get_mut(" outer/路线 ")
        .unwrap()
        .owner_agent_id = format!("original/{}", "界".repeat(22_000));
    let equal_text = source.logistics_routes[" outer/路线 "]
        .owner_agent_id
        .clone();
    let roots = seed(&store, &memory, &source);
    let qualification = qualify_full(&store, &memory, &roots);
    let parent = parent(roots);
    let live = parent.fence().unwrap();
    let mut traces = Vec::new();
    for case in 0..4 {
        let mut resolver = 0;
        for phase in 0..3 {
            let state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical_scope());
            let mut writer = store.value_session(physical_scope());
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [0x91; 32],
                &state,
                &memory,
                tx_limits(),
            )
            .unwrap();
            let reads = Cell::new(0usize);
            let writes = Cell::new(0usize);
            let armed = Cell::new(false);
            let threshold = if phase == 0 {
                usize::MAX
            } else if case == 3 {
                resolver
            } else {
                resolver + 1
            };
            let result;
            context!(
                &mut tx,
                state,
                &values,
                &mut writer,
                &memory,
                || {
                    if !armed.get() {
                        return false;
                    }
                    let n = reads.get() + 1;
                    reads.set(n);
                    phase == 1 && n >= threshold
                },
                || {
                    if !armed.get() {
                        return false;
                    }
                    let n = writes.get() + 1;
                    writes.set(n);
                    phase == 2 && n >= threshold
                },
                |read, write| {
                    let mut budget = work();
                    prefix!(read, write, &mut budget, &live);
                    armed.set(true);
                    let (key, cell, expected) = match case {
                        0 => (
                            LogisticsRecordKeyV1 {
                                kind: K::Route,
                                id: " outer/路线 ",
                            },
                            LogisticsCellV1::Column(C::Priority),
                            V::Unsigned(1),
                        ),
                        1 => (
                            LogisticsRecordKeyV1 {
                                kind: K::Route,
                                id: " outer/路线 ",
                            },
                            LogisticsCellV1::Column(C::OwnerAgentId),
                            V::Text(&source.logistics_routes[" outer/路线 "].owner_agent_id),
                        ),
                        _ => (
                            LogisticsRecordKeyV1 {
                                kind: K::Path,
                                id: " outer/path ",
                            },
                            LogisticsCellV1::TextItem(LogisticsCollectionV1::RouteIds, 0),
                            V::Text("dup"),
                        ),
                    };
                    if phase == 0 {
                        if case == 3 {
                            assert_eq!(
                                read_logistics_record_root_overlay_v1(
                                    LogisticsRecordKeyV1 {
                                        kind: K::Route,
                                        id: "missing"
                                    },
                                    &mut read
                                )
                                .unwrap(),
                                None
                            );
                        } else {
                            read_logistics_cell_overlay_v1(
                                key,
                                cell,
                                &mut read,
                                1 << 20,
                                &mut std::io::sink(),
                            )
                            .unwrap();
                        }
                        resolver = reads.get();
                        assert!(resolver > 0);
                        result = Ok(false);
                    } else {
                        let guard = read.transaction.guard();
                        result = if case == 3 {
                            delete_logistics_record_overlay_v1(
                                LogisticsRecordDeleteV1 {
                                    key: LogisticsRecordKeyV1 {
                                        kind: K::Route,
                                        id: "missing",
                                    },
                                    expected_root: None,
                                    qualification: &qualification,
                                },
                                &mut read,
                                &mut write,
                                &mut budget,
                                &guard,
                                &live,
                            )
                        } else {
                            change_logistics_cell_overlay_v1(
                                LogisticsCellChangeV1 {
                                    key,
                                    cell,
                                    expected: Some(expected),
                                    after: Some(if case == 1 {
                                        V::Text(&equal_text)
                                    } else {
                                        expected
                                    }),
                                    max_current_payload_bytes: 1 << 20,
                                },
                                &mut read,
                                &mut write,
                                &mut budget,
                                &guard,
                                &live,
                            )
                        };
                    }
                }
            );
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 1);
            if phase == 0 {
                assert!(!result.unwrap());
                assert!(sealed.is_ok());
            } else {
                assert!(matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                ));
                assert!(matches!(sealed, Err(ExecutionError::Aborted)));
                traces.push((case, phase, resolver, threshold, reads.get(), writes.get()));
            }
        }
    }
    // Delete's public root getter includes its own terminal read check. Its final
    // combined deletion check is that same public boundary N; point setters add
    // a distinct post-comparison N+1 beyond the public cell resolver.
    eprintln!("logistics public no-op boundaries {traces:?}");
    assert_eq!(traces.len(), 8);
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
pub(super) fn state_and_value_carriers_require_the_actual_caller_pool_and_capacity() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let dir = Directory::new();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let roots = seed(&store, &memory, &source());
    let parent = parent(roots.clone());
    let live = parent.fence().unwrap();
    for state_pages in [false, true] {
        for undercharged in [false, true] {
            let state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let good = store.value_session(physical_scope());
            let mut writer = store.value_session(physical_scope());
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [0x92; 32],
                &state,
                &memory,
                tx_limits(),
            )
            .unwrap();
            context!(
                &mut tx,
                state,
                &good,
                &mut writer,
                &memory,
                || false,
                || false,
                |read, write| {
                    prefix!(read, write, &mut work(), &live);
                }
            );
            let fault = Fault {
                source: &good,
                pool: MemoryBudget::new(4 << 20, 0).unwrap(),
                state: state_pages,
                undercharged,
            };
            let result = read_logistics_cell_overlay_v1(
                LogisticsRecordKeyV1 {
                    kind: LogisticsRecordKindV1::Route,
                    id: " outer/路线 ",
                },
                LogisticsCellV1::Column(LogisticsColumnV1::DistanceKm),
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
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("logistics fault state"))
                    },
                    cancelled: || false,
                },
                8,
                &mut std::io::sink(),
            );
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ));
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
            // Exercise the actual typed-transfer entry too, beyond point preparation.
            let transfer = transfer_logistics_family_pages_v1(
                &roots,
                &mut LogisticsFamilyReadContextV1 {
                    state: &state,
                    values: ValueReadContext {
                        source: &fault,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    max_payload_bytes: 4096,
                    outer_scan: &mut StateScanBudget {
                        nodes_remaining: 1_000_000,
                        read_bytes_remaining: 256 << 20,
                        leaves_remaining: 10_000,
                    },
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("logistics fault transfer"))
                    },
                    cancelled: || false,
                },
                &mut work(),
                20_000,
                &mut writer,
            );
            assert!(matches!(transfer, Err(ValueTreeError::Invalid(_))));
            eprintln!(
                "logistics source state={state_pages} undercharged={undercharged} rejected and prefix aborted"
            );
        }
    }
}
