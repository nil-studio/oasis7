//! Existing point API only; no business authorization/changed-receiver claim.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionError, ExecutionParent, ExecutionTransaction, TransactionLimits,
};
use std::{cell::Cell, io};
struct Guarded<'a> {
    pages: &'a Pages,
    forbidden: [u8; 32],
}
impl ValueNodeReader for Guarded<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        assert_ne!(
            id, &self.forbidden,
            "ordinary point read unrelated 1MiB grant name"
        );
        self.pages.read_node(id, max, memory, class)
    }
}
struct Put<'a> {
    pages: &'a mut Pages,
    puts: usize,
    fail: bool,
    flip: Option<Rc<Cell<bool>>>,
}
impl ValueNodeWriter for Put<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.puts += 1;
        if self.fail {
            return Err(ValueTreeError::Io(io::ErrorKind::ConnectionReset.into()));
        }
        self.pages.put_node(id, value)?;
        if let Some(stop) = &self.flip {
            stop.set(true);
        }
        Ok(())
    }
}
impl StatePageWriter for Put<'_> {}
fn read_work() -> ModuleAuthorizationReadWorkV1 {
    ModuleAuthorizationReadWorkV1 {
        cells_remaining: 10000,
        comparison_bytes_remaining: 4 << 20,
        max_payload_bytes: 128,
    }
}
fn value_error(error: &WorldFieldOverlayErrorV1) -> Option<&ValueTreeError> {
    match error {
        WorldFieldOverlayErrorV1::Value(e)
        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(e)) => Some(e),
        _ => None,
    }
}
#[test]
fn point_getter_noop_and_real_put_failures_preserve_one_transaction_and_skip_unrelated_name() {
    let backing = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = backing.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages(Rc::new(RefCell::new(BTreeMap::new())));
    let grants = BTreeMap::from([(
        "cap".into(),
        CapabilityGrant {
            name: "x".repeat(1 << 20),
            effect_kinds: vec!["existing".into()],
            expiry: Some(5),
        },
    )]);
    let policies = crate::policy::PolicySet::allow_all();
    let mut writes = Vec::new();
    prepare_module_authorization_genesis_v1(
        &grants,
        &policies,
        &mut writer(&mut pages, &backing, &mut io(), &mut keys()),
        &mut work(),
        &mut |row: ReservedValue<ExecutionWriteV1>| {
            writes.push(row.mutation.clone());
            Ok(())
        },
    )
    .unwrap();
    drop(grants);
    drop(policies);
    let mut roots = RootSetV1::empty("module-point-test".into(), [2; 32]).unwrap();
    roots.state[3] = persist(roots.state[3].clone(), writes, &mut pages, &backing);
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let family = bootstrap(&roots, &pages, &memory, MemoryClass::Ordinary).unwrap();
    let source = pages.clone();
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut never = || false;
    let cancel = RefCell::new(&mut never);
    let error = RefCell::new(None);
    let state = tree::Source {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &state_io,
        cancelled: &cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut q = io();
    let mut k = keys();
    let mut fixed = WorldFieldReadContextV1 {
        root: &roots.state[3],
        state: &state,
        values: ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut q,
        },
        key_work: &mut k,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || error.borrow_mut().take(),
        cancelled: || false,
    };
    let body = open_qualified_module_authorization_body_v1(
        ModuleAuthorizationRecordKeyV1::Grant("cap"),
        &roots,
        &family,
        &mut fixed,
    )
    .unwrap()
    .unwrap();
    let mut body_ctx = WorldFieldReadContextV1 {
        root: body.root(),
        state: &state,
        values: ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut q,
        },
        key_work: &mut k,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || error.borrow_mut().take(),
        cancelled: || false,
    };
    let path = read::key(
        ModuleAuthorizationFieldV1::Capabilities,
        ModuleAuthorizationAddressV1::fixed("name"),
        &mut body_ctx.records(),
    )
    .unwrap();
    let forbidden = body_ctx.lookup(&path.0).unwrap().unwrap();
    let values = Guarded {
        pages: &source,
        forbidden,
    };
    let parent = ExecutionParent {
        commit: [1; 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 0,
        },
    };
    let live = parent.fence().unwrap();
    let mut baseline = 0;
    for mode in 0..10 {
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [4; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 512 << 10,
            },
        )
        .unwrap();
        let stale = tx.guard();
        let mut read_io = io();
        let mut read_keys = keys();
        let mut work = read_work();
        let mut prefix = Put {
            pages: &mut pages,
            puts: 0,
            fail: false,
            flip: None,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        let after = change_module_authorization_cell_v1(
            ModuleAuthorizationCellChangeV1 {
                request: ModuleAuthorizationCellRequestV1 {
                    key: ModuleAuthorizationRecordKeyV1::Grant("cap"),
                    address: ModuleAuthorizationAddressV1::fixed("expiry"),
                    family: &family,
                    body: &body,
                    max_payload_bytes: 8,
                },
                expected: ModuleAuthorizationScalarRefV1::Unsigned(5),
                after: ModuleAuthorizationScalarRefV1::Unsigned(6),
                max_pending_node_bytes: 1 << 20,
            },
            &mut overlay,
            &mut MemberRecordWriteContext {
                sink: &mut prefix,
                memory: &memory,
                io: &mut io(),
                key_work: &mut keys(),
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 256,
                    record_bytes: 3 << 20,
                },
                cancelled: || false,
            },
            &mut work,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        assert!(prefix.puts > 0);
        let read_stop = Rc::new(Cell::new(false));
        let write_stop = Rc::new(Cell::new(false));
        let read_calls = Cell::new(0usize);
        let write_calls = Cell::new(0usize);
        let limit = baseline + 1;
        let mut poll_read = || {
            let n = read_calls.get() + 1;
            read_calls.set(n);
            read_stop.get() || (mode == 1 && n == limit)
        };
        let mut poll_write = || {
            let n = write_calls.get() + 1;
            write_calls.set(n);
            write_stop.get() || (mode == 2 && n == limit)
        };
        let guard = if mode == 9 {
            stale
        } else {
            overlay.transaction.guard()
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut *overlay.transaction,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *overlay.values.io,
            },
            key_work: &mut *overlay.key_work,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: &mut poll_read,
        };
        let request = ModuleAuthorizationCellRequestV1 {
            key: ModuleAuthorizationRecordKeyV1::Grant("cap"),
            address: ModuleAuthorizationAddressV1::fixed("expiry"),
            family: &family,
            body: &after,
            max_payload_bytes: 8,
        };
        if mode == 0 {
            let value = read_module_authorization_overlay_v1(
                request,
                &mut overlay,
                &mut work,
                &guard,
                &live,
                &mut io::sink(),
            )
            .unwrap();
            assert_eq!(*value, ModuleAuthorizationValueV1::Unsigned(6));
            baseline = read_calls.get();
            assert!(baseline > 0);
            continue;
        }
        if mode == 7 {
            work.comparison_bytes_remaining = 0;
        }
        let mut sink = Put {
            pages: &mut pages,
            puts: 0,
            fail: mode == 3,
            flip: if mode == 4 {
                Some(read_stop.clone())
            } else if mode == 5 {
                Some(write_stop.clone())
            } else {
                None
            },
        };
        let expected = if mode == 6 { 5 } else { 6 };
        let next = if mode <= 2 { 6 } else { 7 };
        let result = change_module_authorization_cell_v1(
            ModuleAuthorizationCellChangeV1 {
                request,
                expected: ModuleAuthorizationScalarRefV1::Unsigned(expected),
                after: ModuleAuthorizationScalarRefV1::Unsigned(next),
                max_pending_node_bytes: 1 << 20,
            },
            &mut overlay,
            &mut MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut io(),
                key_work: &mut keys(),
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 256,
                    record_bytes: 3 << 20,
                },
                cancelled: &mut poll_write,
            },
            &mut work,
            &guard,
            &live,
        );
        if mode == 8 {
            let changed = result.unwrap().unwrap();
            assert_eq!(changed.allocation_class(), MemoryClass::Ordinary);
            let guard = overlay.transaction.guard();
            let member = change_module_authorization_cell_v1(
                ModuleAuthorizationCellChangeV1 {
                    request: ModuleAuthorizationCellRequestV1 {
                        key: ModuleAuthorizationRecordKeyV1::Grant("cap"),
                        address: ModuleAuthorizationAddressV1 {
                            column: "effect_kinds",
                            index: Some(0),
                        },
                        family: &family,
                        body: &changed,
                        max_payload_bytes: 128,
                    },
                    expected: ModuleAuthorizationScalarRefV1::Text("existing"),
                    after: ModuleAuthorizationScalarRefV1::Text("next.*"),
                    max_pending_node_bytes: 1 << 20,
                },
                &mut overlay,
                &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
                &mut work,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let _output = memory.reserve(128, MemoryClass::Ordinary).unwrap();
            let mut output = Vec::with_capacity(128);
            let guard = overlay.transaction.guard();
            read_module_authorization_overlay_v1(
                ModuleAuthorizationCellRequestV1 {
                    key: ModuleAuthorizationRecordKeyV1::Grant("cap"),
                    address: ModuleAuthorizationAddressV1 {
                        column: "effect_kinds",
                        index: Some(0),
                    },
                    family: &family,
                    body: &member,
                    max_payload_bytes: 128,
                },
                &mut overlay,
                &mut work,
                &guard,
                &live,
                &mut output,
            )
            .unwrap();
            assert_eq!(output, b"next.*");
            continue;
        }
        let failed = result.err().expect("negative operation must fail");
        match mode {
            1 | 2 => {
                assert!(matches!(
                    value_error(&failed),
                    Some(ValueTreeError::Cancelled)
                ));
                assert_eq!(sink.puts, 0);
                assert_eq!(
                    if mode == 1 {
                        read_calls.get()
                    } else {
                        write_calls.get()
                    },
                    limit
                );
            }
            3 => {
                assert!(
                    matches!(value_error(&failed),Some(ValueTreeError::Io(e)) if e.kind()==io::ErrorKind::ConnectionReset)
                );
                assert!(sink.puts > 0);
            }
            4 | 5 => {
                assert!(matches!(
                    value_error(&failed),
                    Some(ValueTreeError::Cancelled)
                ));
                assert!(sink.puts > 0);
            }
            6 => {
                assert!(matches!(
                    value_error(&failed),
                    Some(ValueTreeError::Invalid(_))
                ));
                assert_eq!(sink.puts, 0);
            }
            7 => {
                assert!(matches!(
                    value_error(&failed),
                    Some(ValueTreeError::ResourceLimited(
                        "module authorization comparison"
                    ))
                ));
                assert_eq!(sink.puts, 0);
            }
            9 => assert!(matches!(
                failed,
                WorldFieldOverlayErrorV1::Execution(ExecutionError::StaleGuard)
            )),
            _ => unreachable!(),
        }
        let guard = overlay.transaction.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
    // Exercise the existing nullable Text and closed-label paths as well as
    // U64: None->Unicode and None->Module preserve the other rule columns.
    {
        let mut q = io();
        let mut k = keys();
        let mut fixed = WorldFieldReadContextV1 {
            root: &roots.state[3],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut q,
            },
            key_work: &mut k,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        };
        let mut body = open_qualified_module_authorization_body_v1(
            ModuleAuthorizationRecordKeyV1::Policy(0),
            &roots,
            &family,
            &mut fixed,
        )
        .unwrap()
        .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [5; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 512 << 10,
            },
        )
        .unwrap();
        let mut q = io();
        let mut k = keys();
        let mut work = read_work();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut q,
            },
            key_work: &mut k,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        };
        for (column, text) in [("when.cap_name", "许可🙂"), ("when.origin_kind", "module")] {
            let guard = overlay.transaction.guard();
            body = change_module_authorization_cell_v1(
                ModuleAuthorizationCellChangeV1 {
                    request: ModuleAuthorizationCellRequestV1 {
                        key: ModuleAuthorizationRecordKeyV1::Policy(0),
                        address: ModuleAuthorizationAddressV1::fixed(column),
                        family: &family,
                        body: &body,
                        max_payload_bytes: 128,
                    },
                    expected: ModuleAuthorizationScalarRefV1::Unit,
                    after: ModuleAuthorizationScalarRefV1::Text(text),
                    max_pending_node_bytes: 1 << 20,
                },
                &mut overlay,
                &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
                &mut work,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let _output = memory.reserve(128, MemoryClass::Ordinary).unwrap();
            let mut output = Vec::with_capacity(128);
            let guard = overlay.transaction.guard();
            let value = read_module_authorization_overlay_v1(
                ModuleAuthorizationCellRequestV1 {
                    key: ModuleAuthorizationRecordKeyV1::Policy(0),
                    address: ModuleAuthorizationAddressV1::fixed(column),
                    family: &family,
                    body: &body,
                    max_payload_bytes: 128,
                },
                &mut overlay,
                &mut work,
                &guard,
                &live,
                &mut output,
            )
            .unwrap();
            if column == "when.cap_name" {
                assert_eq!(output, text.as_bytes());
                assert!(matches!(*value, ModuleAuthorizationValueV1::Text { .. }));
            } else {
                assert_eq!(*value, ModuleAuthorizationValueV1::Label("module"));
            }
        }
    }
    eprintln!(
        "module authorization independent getter callbacks={baseline}, NoOp cancellation boundary={}",
        baseline + 1
    );
}

#[path = "incremental_runtime_module_authorization_receiver_tests.rs"]
mod receiver_tests;
