use super::*;
use crate::world::incremental_module_state::{ModuleStateContentBudgetV1, read_iws_module_state};
use crate::world::incremental_module_state_recovery::ModuleStateRecoveryContextV1;
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_wasm_abi::ModuleStateUpdate;
use oasis7_world_state::incremental_event_fields::MAILBOX_KEY_DOMAIN_V1;
use std::io::{self, Write};

struct QualifiedPages<'a>(&'a Pages, &'a MemoryBudget);
impl MerkleNodeReader for QualifiedPages<'_> {
    type Node = Vec<u8>;
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Vec<u8>>, String> {
        self.0.read_node(id, max)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.1)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
}

fn module_event(id: u64, time: u64, module_id: &str, state: Vec<u8>) -> WorldEvent {
    WorldEvent {
        id,
        time,
        caused_by: Some(CausedBy::Effect {
            intent_id: "module-effect-opaque".into(),
        }),
        body: WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
            module_id: module_id.to_owned(),
            trace_id: "trace:module-output".into(),
            state,
        }),
    }
}

fn setup_module_states(
    memory: &MemoryBudget,
    states: &[(&str, &[u8])],
) -> (World, ExecutionParent, Pages, Values) {
    let (mut world, mut parent, mut pages, mut values) = setup(memory);
    for (key, value) in states {
        world
            .state
            .module_states
            .insert((*key).to_owned(), value.to_vec());
    }
    let mut content = ModuleStateContentBudgetV1 {
        states_remaining: states.len() as u64,
        logical_bytes_remaining: states.iter().map(|(_, value)| value.len() as u64).sum(),
        max_state_bytes: 1 << 20,
    };
    world
        .write_iws_module_state_genesis(
            &mut BodyMutationContext {
                sink: &mut values,
                memory,
                io: &mut quota(),
                cancelled: || false,
            },
            &mut content,
            |key, id| {
                add_leaf(&mut parent, &mut pages, Ns::Business, key.to_vec(), id);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(content.states_remaining, 0);
    assert_eq!(content.logical_bytes_remaining, 0);
    (world, parent, pages, values)
}

#[test]
fn module_state_genesis_point_read_and_recovery_preserve_empty_and_absent() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let (world, parent, pages, values) =
        setup_module_states(&memory, &[("instance#1\0雪", &[1, 2, 255]), ("empty", &[])]);
    drop(world);

    let source = values.clone();
    let qualified = qualify_module_states(&parent, &pages, &source, &memory);
    let root = &parent.roots.state[Ns::Business as usize - 1];
    let mut scan = StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 16 << 20,
        leaves_remaining: 1024,
    };
    let mut value_io = quota();
    let mut content = ModuleStateContentBudgetV1 {
        states_remaining: 2,
        logical_bytes_remaining: 64,
        max_state_bytes: 32,
    };
    let verified = ModuleStateRecoveryContextV1 {
        root,
        state: &QualifiedPages(&pages, &memory),
        values: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        state_io: &mut scan,
        value_io: &mut value_io,
        content: &mut content,
    }
    .verify(|| false)
    .unwrap();
    assert_eq!(verified.states, 2);
    assert_eq!(verified.logical_bytes, 3);
    assert_eq!(content.states_remaining, 0);
    assert_eq!(content.logical_bytes_remaining, 61);

    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [0x51; 32], &pages, &memory, txn_limits())
            .unwrap();
    let mut output = Vec::new();
    let mut io = quota();
    let mut cancelled = || false;
    assert_eq!(
        read_iws_module_state(
            &qualified,
            "empty",
            &mut txn,
            &mut oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io
            },
            &mut cancelled,
            32,
            &mut output,
        )
        .unwrap(),
        Some(0)
    );
    assert!(output.is_empty());
    assert_eq!(
        read_iws_module_state(
            &qualified,
            "absent",
            &mut txn,
            &mut oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io
            },
            &mut cancelled,
            32,
            &mut output,
        )
        .unwrap(),
        None
    );
}

#[test]
fn step_dispatch_updates_opaque_state_and_original_without_mailbox() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let key = "instance#stable-2\0雪";
    let (_world, parent, pages, mut values) = setup_module_states(&memory, &[(key, &[1, 2, 3])]);
    let key_bytes = oasis7_world_state::incremental_module_fields::ModuleStateKeyV1::new(key)
        .unwrap()
        .as_bytes()
        .to_vec();
    let old_value = parent.roots.state[Ns::Business as usize - 1]
        .get(&key_bytes, &pages)
        .unwrap()
        .unwrap();
    let source = values.clone();
    let qualified = qualify_module_states(&parent, &pages, &source, &memory);
    let event = module_event(1, 12, key, vec![0, 255, 7, 0]);
    let expected_original = oasis7_wasm_abi::encode_canonical_cbor(&event).unwrap();
    let operation = [0x52; 32];
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), operation, &pages, &memory, txn_limits())
            .unwrap();
    let guard = txn.guard();
    let mut io = quota();
    let mut encoding_work = original_work();
    let mut mutation = BodyMutationContext {
        sink: &mut values,
        memory: &memory,
        io: &mut io,
        cancelled: || false,
    };
    let mut step = IwsStepContext::begin(
        &mut txn,
        11,
        &guard,
        &live,
        &source,
        &mut mutation,
        &mut encoding_work,
    )
    .unwrap();
    assert_eq!(step.time(), 12);
    let mut originals = BTreeMap::new();
    step.dispatch_module_state_event(&qualified, &event, 4096, |original| {
        let id = original.original_id();
        originals.insert(id, collect_original(original, &memory)?);
        Ok(())
    })
    .unwrap();
    assert_eq!(
        step.read_allocator(Alloc::Event).unwrap(),
        Seq { next: 2, era: 0 }
    );

    let mut output = Vec::new();
    let mut read_io = quota();
    assert_eq!(
        read_iws_module_state(
            &qualified,
            key,
            &mut txn,
            &mut oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io
            },
            &mut || false,
            128,
            &mut output,
        )
        .unwrap(),
        Some(4)
    );
    assert_eq!(output, [0, 255, 7, 0]);

    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    TransitionVerifier {
        parent: &parent,
        operation,
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
    let mut receiver_context = ModuleStateRecoveryContextV1 {
        root: prepared.record.after.state_root(Ns::Business),
        state: &pages,
        values: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        state_io: &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 16 << 20,
            leaves_remaining: 1000,
        },
        value_io: &mut quota(),
        content: &mut ModuleStateContentBudgetV1 {
            states_remaining: 1,
            logical_bytes_remaining: 1024,
            max_state_bytes: 128,
        },
    };
    let verifier = TransitionVerifier {
        parent: &parent,
        operation,
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    };
    let accepted =
        crate::world::incremental_module_state_family::accept_module_state_transition_v1(
            &qualified,
            &prepared.record,
            &verifier,
            &mut receiver_context,
            || false,
        )
        .unwrap();
    assert_eq!(accepted.states(), 1);
    assert_eq!(accepted.logical_bytes(), 4);
    assert_eq!(receiver_context.content.states_remaining, 0);
    assert!(
        crate::world::incremental_module_state_family::accept_module_state_transition_v1(
            &qualified,
            &prepared.record,
            &verifier,
            &mut receiver_context,
            || false
        )
        .is_err()
    );
    let state_write = prepared
        .record
        .writes
        .iter()
        .find(|write| write.namespace == Ns::Business && write.mutation.key == key_bytes)
        .unwrap();
    assert_eq!(state_write.mutation.before, Some(old_value));
    assert_ne!(state_write.mutation.after, Some(old_value));
    assert!(prepared.record.writes.iter().all(|write| {
        write.namespace != Ns::Business || !write.mutation.key.starts_with(MAILBOX_KEY_DOMAIN_V1)
    }));
    assert_eq!(prepared.record.facts.len(), 1);
    assert_eq!(prepared.record.facts[0].kind, ExecutionFactKindV1::Event);
    let original = originals.remove(&prepared.record.facts[0].id).unwrap();
    assert_eq!(original.as_slice(), expected_original);
    assert!(originals.is_empty());
}

#[test]
fn wrong_clock_id_and_variant_abort_a_preexisting_step_prefix() {
    for mode in 0..3 {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let key = "instance#stable-2";
        let (world, parent, pages, mut values) = setup_module_states(&memory, &[(key, &[1])]);
        let source = values.clone();
        let qualified = qualify_module_states(&parent, &pages, &source, &memory);
        let operation = [0x53 + mode as u8; 32];
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), operation, &pages, &memory, txn_limits())
                .unwrap();
        let guard = txn.guard();
        let mut io = quota();
        let mut encoding_work = original_work();
        let mut mutation = BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut io,
            cancelled: || false,
        };
        let mut step = IwsStepContext::begin(
            &mut txn,
            11,
            &guard,
            &live,
            &source,
            &mut mutation,
            &mut encoding_work,
        )
        .unwrap();
        assert_eq!(
            step.allocate(Alloc::Event, Seq { next: 1, era: 0 })
                .unwrap()
                .0,
            1
        );
        let event = match mode {
            0 => module_event(2, 13, key, vec![9]),
            1 => module_event(3, 12, key, vec![9]),
            _ => {
                let mut event = super::super::event(&world, 2);
                event.time = 12;
                event
            }
        };
        let called = Cell::new(false);
        assert!(
            step.dispatch_module_state_event(&qualified, &event, 4096, |_| {
                called.set(true);
                Ok(())
            })
            .is_err()
        );
        assert!(!called.get());
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn original_skip_late_cancel_and_partial_io_failure_abort_the_whole_step() {
    for mode in 0..3 {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let key = "instance#stable-2";
        let (world, parent, pages, mut values) = setup_module_states(&memory, &[(key, &[1])]);
        let source = values.clone();
        let qualified = qualify_module_states(&parent, &pages, &source, &memory);
        let event = module_event(1, 12, key, vec![2]);
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [0x56; 32], &pages, &memory, txn_limits())
                .unwrap();
        let guard = txn.guard();
        let stopped = Rc::new(Cell::new(false));
        let cancelled = stopped.clone();
        let mut io = quota();
        let mut encoding_work = original_work();
        let mut mutation = BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut io,
            cancelled: move || cancelled.get(),
        };
        let mut step = IwsStepContext::begin(
            &mut txn,
            11,
            &guard,
            &live,
            &source,
            &mut mutation,
            &mut encoding_work,
        )
        .unwrap();
        let prefix_bytes = Cell::new(0);
        let result =
            step.dispatch_module_state_event(&qualified, &event, 4096, |original| match mode {
                0 => Ok(()), // Callback success cannot stand in for stream+finish.
                1 => original.persist_to(
                    &mut |stream: &mut dyn AgentEventOriginalStreamV1,
                          _cancelled: &mut dyn FnMut() -> bool| {
                        let result = stream.write_to(&mut PrefixThenFail {
                            written: &prefix_bytes,
                        });
                        result?;
                        Ok(())
                    },
                ),
                _ => {
                    collect_original(original, &memory)?;
                    stopped.set(true);
                    Ok(())
                }
            });
        match mode {
            0 => assert!(matches!(
                result,
                Err(BodyMutationError::Invalid(
                    "agent original persistence incomplete"
                ))
            )),
            1 => {
                assert!(prefix_bytes.get() > 0);
                assert!(result.is_err());
            }
            _ => assert!(matches!(
                result,
                Err(BodyMutationError::Value(ValueTreeError::Cancelled))
            )),
        }
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
        drop(world);
    }
}

struct PrefixThenFail<'a> {
    written: &'a Cell<usize>,
}
impl Write for PrefixThenFail<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.written.get() != 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "injected prefix failure",
            ));
        }
        let count = bytes.len().min(11);
        self.written.set(count);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn recovery_value_class_cannot_read_through_ordinary_step_prefix() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let (_world, parent, pages, mut values) = setup_module_states(&memory, &[("key", &[1])]);
    let source = values.clone();
    let qualified = qualify_module_states(&parent, &pages, &source, &memory);
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent, [0x99; 32], &pages, &memory, txn_limits()).unwrap();
    let guard = txn.guard();
    let mut io = quota();
    let mut work = original_work();
    let mut mutation = BodyMutationContext {
        sink: &mut values,
        memory: &memory,
        io: &mut io,
        cancelled: || false,
    };
    let mut step = IwsStepContext::begin(
        &mut txn,
        11,
        &guard,
        &live,
        &source,
        &mut mutation,
        &mut work,
    )
    .unwrap();
    assert_eq!(
        step.allocate(Alloc::Event, Seq { next: 1, era: 0 })
            .unwrap()
            .0,
        1
    );
    let mut read_io = quota();
    let result = read_iws_module_state(
        &qualified,
        "key",
        &mut txn,
        &mut oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut read_io,
        },
        &mut || false,
        32,
        &mut Vec::new(),
    );
    assert!(matches!(result, Err(BodyMutationError::Invalid(_))));
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));
}

fn qualify_module_states(
    parent: &ExecutionParent,
    pages: &Pages,
    values: &Values,
    memory: &MemoryBudget,
) -> oasis7_proto::memory_budget::ReservedValue<
    crate::world::incremental_module_state_family::ModuleStateFamilyQualificationV1,
> {
    ModuleStateRecoveryContextV1 {
        root: parent.roots.state_root(Ns::Business),
        state: &QualifiedPages(pages, memory),
        values,
        memory,
        class: MemoryClass::Ordinary,
        state_io: &mut StateScanBudget {
            nodes_remaining: 1000000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 10000,
        },
        value_io: &mut quota(),
        content: &mut ModuleStateContentBudgetV1 {
            states_remaining: 1000,
            logical_bytes_remaining: 16 << 20,
            max_state_bytes: 16 << 20,
        },
    }
    .qualify_family(&parent.roots, || false)
    .unwrap()
}

#[test]
fn generic_valid_misplaced_and_malformed_state_writes_fail_owner_receiver() {
    for misplaced in [false, true] {
        let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let (_world, parent, pages, values) = setup_module_states(&memory, &[("key", &[1])]);
        let qualified = qualify_module_states(&parent, &pages, &values, &memory);
        let valid_key =
            oasis7_world_state::incremental_module_fields::ModuleStateKeyV1::new("key").unwrap();
        let id = parent
            .roots
            .state_root(Ns::Business)
            .get(valid_key.as_bytes(), &pages)
            .unwrap()
            .unwrap();
        let mut key = valid_key.as_bytes().to_vec();
        if !misplaced {
            key.push(0);
        }
        let namespace = if misplaced {
            Ns::Allocator
        } else {
            Ns::Business
        };
        let live = parent.fence().unwrap();
        let operation = [0x88; 32];
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), operation, &pages, &memory, txn_limits())
                .unwrap();
        txn.write(namespace, &key, Some(id)).unwrap();
        let guard = txn.guard();
        let prepared = txn.seal(&guard, &live, 0).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation,
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        verifier.verify(&prepared.record).unwrap();
        let mut context = ModuleStateRecoveryContextV1 {
            root: prepared.record.after.state_root(Ns::Business),
            state: &pages,
            values: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            state_io: &mut StateScanBudget {
                nodes_remaining: 100000,
                read_bytes_remaining: 16 << 20,
                leaves_remaining: 1000,
            },
            value_io: &mut quota(),
            content: &mut ModuleStateContentBudgetV1 {
                states_remaining: 10,
                logical_bytes_remaining: 1024,
                max_state_bytes: 128,
            },
        };
        assert!(matches!(
            crate::world::incremental_module_state_family::accept_module_state_transition_v1(
                &qualified,
                &prepared.record,
                &verifier,
                &mut context,
                || false
            ),
            Err(BodyMutationError::Invalid(_))
        ));
    }
}

struct RecoveryPages<'a>(&'a Pages, &'a MemoryBudget);
impl MerkleNodeReader for RecoveryPages<'_> {
    type Node = Vec<u8>;
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Vec<u8>>, String> {
        self.0.read_node(id, max)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.1)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
}
#[test]
fn recovery_family_requires_explicit_ordinary_readmission_after_real_prefix() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (_world, parent, pages, mut values) = setup_module_states(&memory, &[("key", &[1])]);
    let source = values.clone();
    let qualified = ModuleStateRecoveryContextV1 {
        root: parent.roots.state_root(Ns::Business),
        state: &RecoveryPages(&pages, &memory),
        values: &source,
        memory: &memory,
        class: MemoryClass::Recovery,
        state_io: &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 16 << 20,
            leaves_remaining: 1000,
        },
        value_io: &mut quota(),
        content: &mut ModuleStateContentBudgetV1 {
            states_remaining: 10,
            logical_bytes_remaining: 1024,
            max_state_bytes: 128,
        },
    }
    .qualify_family(&parent.roots, || false)
    .unwrap();
    let ordinary =
        crate::world::incremental_module_state_family::readmit_module_state_family_ordinary_v1(
            &qualified,
            &parent.roots,
            &memory,
            || false,
        )
        .unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [0x91; 32], &pages, &memory, txn_limits())
            .unwrap();
    let mut read_io = quota();
    assert_eq!(
        read_iws_module_state(
            &ordinary,
            "key",
            &mut txn,
            &mut oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io
            },
            &mut || false,
            128,
            &mut Vec::new()
        )
        .unwrap(),
        Some(1)
    );
    let guard = txn.guard();
    let mut io = quota();
    let mut work = original_work();
    let mut mutation = BodyMutationContext {
        sink: &mut values,
        memory: &memory,
        io: &mut io,
        cancelled: || false,
    };
    let mut step = IwsStepContext::begin(
        &mut txn,
        11,
        &guard,
        &live,
        &source,
        &mut mutation,
        &mut work,
    )
    .unwrap();
    assert_eq!(
        step.allocate(Alloc::Event, Seq { next: 1, era: 0 })
            .unwrap()
            .0,
        1
    );
    assert!(matches!(
        step.dispatch_module_state_event(
            &qualified,
            &module_event(2, 12, "key", vec![2]),
            4096,
            |_| Ok(())
        ),
        Err(BodyMutationError::Invalid(_))
    ));
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));
}

#[test]
fn full_raw_qualification_rejects_misplaced_and_malformed_dedicated_keys() {
    for namespace in [Ns::Allocator, Ns::Business] {
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let (_world, mut parent, mut pages, values) =
            setup_module_states(&memory, &[("key", &[1])]);
        let valid =
            oasis7_world_state::incremental_module_fields::ModuleStateKeyV1::new("key").unwrap();
        let id = parent
            .roots
            .state_root(Ns::Business)
            .get(valid.as_bytes(), &pages)
            .unwrap()
            .unwrap();
        let mut key = valid.as_bytes().to_vec();
        if namespace == Ns::Business {
            key.push(0);
        }
        add_leaf(&mut parent, &mut pages, namespace, key, id);
        let result = ModuleStateRecoveryContextV1 {
            root: parent.roots.state_root(Ns::Business),
            state: &QualifiedPages(&pages, &memory),
            values: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            state_io: &mut StateScanBudget {
                nodes_remaining: 100000,
                read_bytes_remaining: 16 << 20,
                leaves_remaining: 1000,
            },
            value_io: &mut quota(),
            content: &mut ModuleStateContentBudgetV1 {
                states_remaining: 10,
                logical_bytes_remaining: 1024,
                max_state_bytes: 128,
            },
        }
        .qualify_family(&parent.roots, || false);
        assert!(matches!(result, Err(BodyMutationError::Invalid(_))));
    }
}
