use super::*;
use crate::{InstanceScratchBudget, ModuleInstanceRecoveryContext};
use oasis7_formal_protocol::incremental_scratch_counter::{
    CounterScratchStore, ScratchCounterDelta,
};
use oasis7_formal_protocol::incremental_state::{MerkleNodeReader, StateScanBudget};
use oasis7_world_state::incremental_module_instance_index::*;
use std::cell::RefCell;

struct Bytes(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct Store<'a> {
    memory: &'a MemoryBudget,
    class: MemoryClass,
    pages: RefCell<BTreeMap<String, Vec<u8>>>,
    fail_write: bool,
}
impl MerkleNodeReader for Store<'_> {
    type Node = Bytes;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Bytes>, String> {
        let pages = self.pages.borrow();
        let Some(bytes) = pages.get(id) else {
            return Ok(None);
        };
        if bytes.len() > max {
            return Err("test state bound".into());
        }
        let charge = self
            .memory
            .reserve(bytes.len(), self.class)
            .map_err(|e| e.to_string())?;
        Ok(Some(Bytes(charge.retain(bytes.clone()))))
    }
}
impl CounterScratchStore for Store<'_> {
    fn put_scratch_pages(&self, delta: &ReservedValue<ScratchCounterDelta>) -> Result<(), String> {
        if self.fail_write {
            return Err("injected scratch failure".into());
        }
        assert!(delta.uses_budget(self.memory));
        delta.verify_pages()?;
        for page in &delta.new_nodes {
            self.pages
                .borrow_mut()
                .insert(page.digest.clone(), page.bytes.clone());
        }
        Ok(())
    }
}
fn state_io() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 200_000,
        read_bytes_remaining: 128 << 20,
        leaves_remaining: 10_000,
    }
}
fn scratch_io() -> InstanceScratchBudget {
    InstanceScratchBudget {
        reads: 200_000,
        read_bytes: 128 << 20,
        writes: 200_000,
        write_bytes: 128 << 20,
    }
}
pub(super) fn qualify(
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    pages: &Pages,
    values: &Values,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> ReservedValue<crate::ModuleInstanceFamilyQualificationV1> {
    let state = Store {
        memory,
        class,
        pages: RefCell::new(pages.0.clone()),
        fail_write: false,
    };
    let scratch = Store {
        memory,
        class,
        pages: RefCell::new(BTreeMap::new()),
        fail_write: false,
    };
    ModuleInstanceRecoveryContext {
        root: roots.state_root(Ns::Business),
        state: &state,
        values,
        scratch: &scratch,
        memory,
        class,
        state_io: &mut state_io(),
        value_io: &mut genesis_io(),
        scratch_io: &mut scratch_io(),
        max_scalar_bytes: 4096,
    }
    .qualify_family(roots, || false)
    .unwrap()
}
fn verify<S: CounterScratchStore>(
    root: &oasis7_formal_protocol::incremental_state::StateRootV1,
    state: &S,
    values: &Values,
    memory: &MemoryBudget,
    class: MemoryClass,
    scratch: &S,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<crate::VerifiedModuleInstanceContent>, BodyMutationError> {
    ModuleInstanceRecoveryContext {
        root,
        state,
        values,
        scratch,
        memory,
        class,
        state_io: &mut state_io(),
        value_io: &mut genesis_io(),
        scratch_io: &mut scratch_io(),
        max_scalar_bytes: 4096,
    }
    .verify(cancelled)
}
fn change(parent: &mut ExecutionParent, pages: &mut Pages, key: Vec<u8>, after: Option<[u8; 32]>) {
    let before = parent.roots.state[0].get(&key, pages).unwrap();
    let delta = StateDeltaV1::build(
        parent.roots.state[0].clone(),
        vec![StateWriteV1 { key, before, after }],
        pages,
        1 << 20,
    )
    .unwrap();
    for node in &delta.new_nodes {
        pages.0.insert(node.digest.clone(), node.bytes.clone());
    }
    parent.roots.state[0] = delta.after;
}
#[test]
fn recovery_rebuilds_exact_counts_and_rejects_orphan_partial_wrong_and_missing_families() {
    for case in 0..10 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (_, mut parent, mut pages, mut values) = setup_instances(&memory);
        match case {
            0 | 8 | 9 => {}
            1 => change(
                &mut parent,
                &mut pages,
                ModuleInstanceKeyV1::new("map-key", F::Owner)
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
                None,
            ),
            2 => {
                let key = ModuleInstanceKeyV1::new("orphan", F::Active).unwrap();
                let value = parent.roots.state[0]
                    .get(
                        ModuleInstanceKeyV1::new("map-key", F::Active)
                            .unwrap()
                            .as_bytes(),
                        &pages,
                    )
                    .unwrap();
                change(&mut parent, &mut pages, key.as_bytes().to_vec(), value);
            }
            3 | 4 => {
                let bytes = module_instance_count_bytes(2).unwrap();
                let value = write_field_bytes(
                    &bytes,
                    &mut BodyMutationContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut genesis_io(),
                        cancelled: || false,
                    },
                )
                .unwrap();
                let key = module_instance_count_key(if case == 3 { "module" } else { "extra" });
                change(&mut parent, &mut pages, key.to_vec(), Some(value));
            }
            5 => change(
                &mut parent,
                &mut pages,
                MODULE_INSTANCE_INDEX_KEY_V1.to_vec(),
                None,
            ),
            6 => {
                let mut key = ModuleInstanceKeyV1::new("map-key", F::Owner)
                    .unwrap()
                    .as_bytes()
                    .to_vec();
                *key.last_mut().unwrap() = 99;
                change(&mut parent, &mut pages, key, Some([0; 32]));
            }
            7 => change(
                &mut parent,
                &mut pages,
                module_instance_count_key("module").to_vec(),
                None,
            ),
            _ => unreachable!(),
        }
        let state = Store {
            memory: &memory,
            class: MemoryClass::Ordinary,
            pages: RefCell::new(pages.0),
            fail_write: false,
        };
        let scratch = Store {
            memory: &memory,
            class: MemoryClass::Ordinary,
            pages: RefCell::new(BTreeMap::new()),
            fail_write: case == 8,
        };
        let before = memory.used();
        let result = verify(
            &parent.roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &scratch,
            || case == 9,
        );
        if case == 0 {
            let output = result.unwrap();
            assert_eq!(output.instances, 1);
            assert_eq!(output.modules, 1);
            assert!(output.uses_budget(&memory));
        } else {
            assert!(result.is_err(), "case {case}");
        }
        assert_eq!(memory.used(), before);
    }
}
#[test]
fn recovery_budget_shortage_and_foreign_scratch_never_return_partial_content() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, values) = setup_instances(&memory);
    for dimension in 0..8 {
        let state = Store {
            memory: &memory,
            class: MemoryClass::Ordinary,
            pages: RefCell::new(pages.0.clone()),
            fail_write: false,
        };
        let mut scan = state_io();
        let mut value = genesis_io();
        let mut scratch = scratch_io();
        match dimension {
            0 => scan.nodes_remaining = 0,
            1 => scan.read_bytes_remaining = 0,
            2 => scan.leaves_remaining = 0,
            3 => value.nodes_remaining = 0,
            4 => scratch.reads = 0,
            5 => scratch.read_bytes = 0,
            6 => scratch.writes = 0,
            7 => scratch.write_bytes = 0,
            _ => unreachable!(),
        }
        assert!(
            ModuleInstanceRecoveryContext {
                root: &parent.roots.state[0],
                state: &state,
                values: &values,
                scratch: &state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                state_io: &mut scan,
                value_io: &mut value,
                scratch_io: &mut scratch,
                max_scalar_bytes: 4096,
            }
            .verify(|| false)
            .is_err(),
            "dimension {dimension}"
        );
    }
    let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let state = Store {
        memory: &memory,
        class: MemoryClass::Ordinary,
        pages: RefCell::new(pages.0),
        fail_write: false,
    };
    let scratch = Store {
        memory: &foreign,
        class: MemoryClass::Ordinary,
        pages: RefCell::new(BTreeMap::new()),
        fail_write: false,
    };
    assert!(
        verify(
            &parent.roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &scratch,
            || false
        )
        .is_err()
    );
}

#[test]
fn late_cancellation_and_scratch_unwind_release_workspace_without_partial_content() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, values) = setup_instances(&memory);
    let state = Store {
        memory: &memory,
        class: MemoryClass::Ordinary,
        pages: RefCell::new(pages.0),
        fail_write: false,
    };
    let scratch = Store {
        memory: &memory,
        class: MemoryClass::Ordinary,
        pages: RefCell::new(BTreeMap::new()),
        fail_write: false,
    };
    let before = memory.used();
    assert!(
        verify(
            &parent.roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &scratch,
            || !scratch.pages.borrow().is_empty()
        )
        .is_err()
    );
    assert!(!scratch.pages.borrow().is_empty());
    assert_eq!(memory.used(), before);
    struct Panics<'a, 'b>(&'a Store<'b>);
    impl MerkleNodeReader for Panics<'_, '_> {
        type Node = Bytes;
        fn allocation_budget(&self) -> Option<&MemoryBudget> {
            self.0.allocation_budget()
        }
        fn allocation_class(&self) -> Option<MemoryClass> {
            self.0.allocation_class()
        }
        fn read_node(&self, id: &str, max: usize) -> Result<Option<Bytes>, String> {
            self.0.read_node(id, max)
        }
    }
    impl CounterScratchStore for Panics<'_, '_> {
        fn put_scratch_pages(&self, _: &ReservedValue<ScratchCounterDelta>) -> Result<(), String> {
            panic!("scratch writer unwind")
        }
    }
    let mut scan = state_io();
    let mut value = genesis_io();
    let mut allowance = scratch_io();
    let before_writes = allowance.writes;
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ModuleInstanceRecoveryContext {
                root: &parent.roots.state[0],
                state: &state,
                values: &values,
                scratch: &Panics(&scratch),
                memory: &memory,
                class: MemoryClass::Ordinary,
                state_io: &mut scan,
                value_io: &mut value,
                scratch_io: &mut allowance,
                max_scalar_bytes: 4096,
            }
            .verify(|| false)
        }))
        .is_err()
    );
    assert!(allowance.writes < before_writes);
    assert_eq!(memory.used(), before);
}

#[test]
fn qualified_instance_point_and_changed_receiver_match_full_raw_oracle() {
    use oasis7_formal_protocol::incremental_execution::TransitionVerifier;
    for case in 0..9 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (_, parent, mut pages, mut values) = setup_instances(&memory);
        let proof = qualify(
            &parent.roots,
            &pages,
            &values,
            &memory,
            MemoryClass::Ordinary,
        );
        let source = values.clone();
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits())
                .unwrap();
        let mut quota = genesis_io();
        if case == 0 || case >= 7 {
            assert!(
                crate::change_module_instance_field_v1(
                    crate::ModuleInstanceFieldChangeV1 {
                        qualification: &proof,
                        map_key: "map-key",
                        field: F::Active,
                        expected: S::Bool(false),
                        after: S::Bool(true),
                        max_scalar_bytes: 4096,
                    },
                    &mut tx,
                    &source,
                    &mut BodyMutationContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut quota,
                        cancelled: || false,
                    },
                )
                .unwrap()
            );
            let value = ModuleReadContext {
                qualification: &proof,
                values: &source,
                memory: &memory,
                io: &mut quota,
                max_value_bytes: 4096,
                cancelled: || false,
            }
            .read_instance_field("map-key", F::Active, &mut tx)
            .unwrap()
            .unwrap();
            assert_eq!(
                S::from_canonical_bytes(&value, F::Active).unwrap(),
                S::Bool(true)
            );
        } else {
            let (namespace, key, after) = match case {
                1 => (
                    Ns::Business,
                    ModuleInstanceKeyV1::new("map-key", F::Active)
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                    None,
                ),
                2 => {
                    let id = write_field_bytes(
                        &module_instance_count_bytes(2).unwrap(),
                        &mut BodyMutationContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut quota,
                            cancelled: || false,
                        },
                    )
                    .unwrap();
                    (
                        Ns::Business,
                        module_instance_count_key("module").to_vec(),
                        Some(id),
                    )
                }
                3 => {
                    let id = write_field_bytes(
                        &module_instance_index_bytes(2),
                        &mut BodyMutationContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut quota,
                            cancelled: || false,
                        },
                    )
                    .unwrap();
                    (
                        Ns::Business,
                        MODULE_INSTANCE_INDEX_KEY_V1.to_vec(),
                        Some(id),
                    )
                }
                4 => (
                    Ns::Authorization,
                    ModuleInstanceKeyV1::new("map-key", F::Active)
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                    Some([7; 32]),
                ),
                5 => {
                    let mut key = MODULE_INSTANCE_INDEX_KEY_V1.to_vec();
                    key.push(0);
                    let id = parent
                        .roots
                        .state_root(Ns::Business)
                        .get(MODULE_INSTANCE_INDEX_KEY_V1, &pages)
                        .unwrap()
                        .unwrap();
                    (Ns::Business, key, Some(id))
                }
                6 => {
                    let mut bad = Vec::new();
                    S::Bool(true).write_to(F::Active, &mut bad).unwrap();
                    *bad.last_mut().unwrap() = 2;
                    assert!(S::from_canonical_bytes(&bad, F::Active).is_err());
                    let id = write_field_bytes(
                        &bad,
                        &mut BodyMutationContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut quota,
                            cancelled: || false,
                        },
                    )
                    .unwrap();
                    (
                        Ns::Business,
                        ModuleInstanceKeyV1::new("map-key", F::Active)
                            .unwrap()
                            .as_bytes()
                            .to_vec(),
                        Some(id),
                    )
                }
                _ => unreachable!(),
            };
            tx.write(namespace, &key, after).unwrap();
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 0).unwrap();
        for node in &sealed.new_nodes {
            pages.0.insert(node.digest.clone(), node.bytes.clone());
        }
        let state = Store {
            memory: &memory,
            class: MemoryClass::Ordinary,
            pages: RefCell::new(pages.0.clone()),
            fail_write: false,
        };
        let scratch = Store {
            memory: &memory,
            class: MemoryClass::Ordinary,
            pages: RefCell::new(BTreeMap::new()),
            fail_write: false,
        };
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [4; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 1 << 20,
        };
        // Every negative really passes independent generic replay first.
        drop(verifier.verify(&sealed.record).unwrap());
        // Generic replay and owner validation deliberately use different readers.
        // Identical authenticated bytes do not transfer admission between pools/classes.
        let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let typed_state = Store {
            memory: if case == 8 { &foreign } else { &memory },
            class: if case == 7 {
                MemoryClass::Recovery
            } else {
                MemoryClass::Ordinary
            },
            pages: RefCell::new(pages.0.clone()),
            fail_write: false,
        };
        let mut ctx = ModuleInstanceRecoveryContext {
            root: sealed.record.after.state_root(Ns::Business),
            state: &typed_state,
            values: &values,
            scratch: &scratch,
            memory: &memory,
            class: MemoryClass::Ordinary,
            state_io: &mut state_io(),
            value_io: &mut genesis_io(),
            scratch_io: &mut scratch_io(),
            max_scalar_bytes: 4096,
        };
        let result = crate::accept_module_instance_transition_v1(
            &proof,
            &sealed.record,
            &verifier,
            &mut ctx,
            &mut 10_000,
            || false,
        );
        if case == 0 {
            let after = result.unwrap();
            after.validate_binding(&sealed.record.after).unwrap();
            let raw = ctx.qualify_family(&sealed.record.after, || false).unwrap();
            assert_eq!(after.instance_count(), raw.instance_count());
            assert_eq!(after.module_count(), raw.module_count());
        } else {
            assert!(
                matches!(result, Err(BodyMutationError::Invalid(_))),
                "case {case}"
            );
            assert!(
                ctx.qualify_family(&sealed.record.after, || false).is_err(),
                "raw case {case}"
            );
        }
    }
}

#[test]
fn recovery_instance_family_requires_explicit_ordinary_readmission_and_poison() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, mut values) = setup_instances(&memory);
    let proof = qualify(
        &parent.roots,
        &pages,
        &values,
        &memory,
        MemoryClass::Recovery,
    );
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let ordinary =
        crate::readmit_module_instance_family_ordinary_v1(&proof, &parent.roots, &memory, || false)
            .unwrap();
    let source = values.clone();
    assert!(
        crate::change_module_instance_field_v1(
            crate::ModuleInstanceFieldChangeV1 {
                qualification: &ordinary,
                map_key: "map-key",
                field: F::Active,
                expected: S::Bool(false),
                after: S::Bool(true),
                max_scalar_bytes: 4096,
            },
            &mut tx,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut genesis_io(),
                cancelled: || false,
            }
        )
        .unwrap()
    );
    values.reads.set(0);
    let result = ModuleReadContext {
        qualification: &proof,
        values: &values,
        memory: &memory,
        io: &mut genesis_io(),
        max_value_bytes: 4096,
        cancelled: || false,
    }
    .read_instance_field("map-key", F::Active, &mut tx);
    assert!(matches!(result, Err(BodyMutationError::Invalid(_))));
    assert_eq!(values.reads.get(), 0);
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 0),
        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
    ));
    let mut wrong = parent.roots.clone();
    wrong.height += 1;
    assert!(
        crate::readmit_module_instance_family_ordinary_v1(&proof, &wrong, &memory, || false)
            .is_err()
    );
    let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    assert!(
        crate::readmit_module_instance_family_ordinary_v1(&proof, &parent.roots, &foreign, || {
            false
        })
        .is_err()
    );
    let used = memory.used();
    let polls = Cell::new(0);
    assert!(matches!(
        crate::readmit_module_instance_family_ordinary_v1(&proof, &parent.roots, &memory, || {
            polls.set(polls.get() + 1);
            polls.get() == 2
        }),
        Err(BodyMutationError::Value(ValueTreeError::Cancelled))
    ));
    assert_eq!(memory.used(), used);
    let hold = memory
        .reserve((28 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(
        crate::readmit_module_instance_family_ordinary_v1(&proof, &parent.roots, &memory, || false)
            .is_err()
    );
    drop(hold);
    let admitted =
        crate::readmit_module_instance_family_ordinary_v1(&proof, &parent.roots, &memory, || false)
            .unwrap();
    assert_eq!(values.reads.get(), 0);
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    drop(proof);
    let mut tx = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    assert!(
        ModuleReadContext {
            qualification: &admitted,
            values: &values,
            memory: &memory,
            io: &mut genesis_io(),
            max_value_bytes: 4096,
            cancelled: || false,
        }
        .read_instance_field("map-key", F::Active, &mut tx)
        .unwrap()
        .is_some()
    );
}

#[test]
fn instance_noop_independent_getter_boundary_and_real_io_failures_abort_prefix() {
    struct BadReader(Cell<usize>);
    impl ValueNodeReader for BadReader {
        fn read_node(
            &self,
            _: &[u8; 32],
            _: usize,
            _: &MemoryBudget,
            _: MemoryClass,
        ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
            self.0.set(self.0.get() + 1);
            Err(ValueTreeError::Io(std::io::Error::other(
                "actual source failure",
            )))
        }
    }
    struct BadWriter(Cell<usize>);
    impl ValueNodeWriter for BadWriter {
        fn put_node(
            &mut self,
            _: [u8; 32],
            _: ReservedValue<Vec<u8>>,
        ) -> Result<(), ValueTreeError> {
            self.0.set(self.0.get() + 1);
            Err(ValueTreeError::Io(std::io::Error::other(
                "actual put failure",
            )))
        }
    }
    for mode in 0..3 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (_, parent, pages, mut values) = setup_instances(&memory);
        let proof = qualify(
            &parent.roots,
            &pages,
            &values,
            &memory,
            MemoryClass::Ordinary,
        );
        let source = values.clone();
        let polls = Cell::new(0);
        let mut getter =
            ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits())
                .unwrap();
        ModuleReadContext {
            qualification: &proof,
            values: &source,
            memory: &memory,
            io: &mut genesis_io(),
            max_value_bytes: 165,
            cancelled: || {
                polls.set(polls.get() + 1);
                false
            },
        }
        .read_instance_field("map-key", F::InstalledAt, &mut getter)
        .unwrap()
        .unwrap();
        let n = polls.get();
        drop(getter);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        let mut quota = genesis_io();
        assert!(
            crate::change_module_instance_field_v1(
                crate::ModuleInstanceFieldChangeV1 {
                    qualification: &proof,
                    map_key: "map-key",
                    field: F::Active,
                    expected: S::Bool(false),
                    after: S::Bool(true),
                    max_scalar_bytes: 165,
                },
                &mut tx,
                &source,
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut quota,
                    cancelled: || false,
                }
            )
            .unwrap()
        );
        let failed_source = BadReader(Cell::new(0));
        let mut failed_writer = BadWriter(Cell::new(0));
        polls.set(0);
        let result = if mode == 0 {
            ModuleReadContext {
                qualification: &proof,
                values: &failed_source,
                memory: &memory,
                io: &mut quota,
                max_value_bytes: 165,
                cancelled: || false,
            }
            .read_instance_field("map-key", F::InstalledAt, &mut tx)
            .map(|_| false)
        } else {
            crate::change_module_instance_field_v1(
                crate::ModuleInstanceFieldChangeV1 {
                    qualification: &proof,
                    map_key: "map-key",
                    field: F::InstalledAt,
                    expected: S::U64(u64::MAX),
                    after: S::U64(if mode == 1 { 0 } else { u64::MAX }),
                    max_scalar_bytes: 165,
                },
                &mut tx,
                &source,
                &mut BodyMutationContext {
                    sink: &mut failed_writer,
                    memory: &memory,
                    io: &mut quota,
                    cancelled: || {
                        polls.set(polls.get() + 1);
                        mode == 2 && polls.get() == n + 1
                    },
                },
            )
        };
        if mode == 2 {
            assert!(matches!(
                result,
                Err(BodyMutationError::Value(ValueTreeError::Cancelled))
            ));
            assert_eq!(polls.get(), n + 1);
            assert_eq!(failed_writer.0.get(), 0);
            eprintln!(
                "instance independent getter N={n}, NoOp cancelled at {}",
                n + 1
            );
        } else {
            assert!(matches!(
                result,
                Err(BodyMutationError::Value(ValueTreeError::Io(_)))
            ));
            assert!(
                if mode == 0 {
                    failed_source.0.get()
                } else {
                    failed_writer.0.get()
                } > 0
            );
        }
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 0),
            Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
        ));
    }
}

#[test]
fn instance_create_move_delete_each_advance_qualified_parent_and_raw_counts() {
    use oasis7_formal_protocol::incremental_execution::TransitionVerifier;
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, mut parent, mut pages, mut values) = setup_instances(&memory);
    let mut proof = qualify(
        &parent.roots,
        &pages,
        &values,
        &memory,
        MemoryClass::Ordinary,
    );
    let mut second = instance();
    second.instance_id = "different embedded ID".into();
    let mut moved = instance();
    moved.module_id = "different module\0 ".into();
    for (ordinal, (key, after, total, modules)) in [
        ("second", Some(second), 2, 1),
        ("map-key", Some(moved), 2, 2),
        ("second", None, 1, 1),
        ("map-key", None, 0, 0),
    ]
    .into_iter()
    .enumerate()
    {
        let source = values.clone();
        let live = parent.fence().unwrap();
        let operation = [ordinal as u8 + 20; 32];
        let mut bounds = limits();
        bounds.max_reads = 128;
        bounds.max_writes = 128;
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), operation, &pages, &memory, bounds)
                .unwrap();
        crate::contribute_module_instance_change(
            &proof,
            key,
            after.as_ref(),
            &mut tx,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut genesis_io(),
                cancelled: || false,
            },
            4096,
        )
        .unwrap();
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 0).unwrap();
        for node in &sealed.new_nodes {
            pages.0.insert(node.digest.clone(), node.bytes.clone());
        }
        let state = Store {
            memory: &memory,
            class: MemoryClass::Ordinary,
            pages: RefCell::new(pages.0.clone()),
            fail_write: false,
        };
        let scratch = Store {
            memory: &memory,
            class: MemoryClass::Ordinary,
            pages: RefCell::new(BTreeMap::new()),
            fail_write: false,
        };
        let verifier = TransitionVerifier {
            parent: &parent,
            operation,
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 1 << 20,
        };
        let mut ctx = ModuleInstanceRecoveryContext {
            root: sealed.record.after.state_root(Ns::Business),
            state: &state,
            values: &values,
            scratch: &scratch,
            memory: &memory,
            class: MemoryClass::Ordinary,
            state_io: &mut state_io(),
            value_io: &mut genesis_io(),
            scratch_io: &mut scratch_io(),
            max_scalar_bytes: 4096,
        };
        let next = crate::accept_module_instance_transition_v1(
            &proof,
            &sealed.record,
            &verifier,
            &mut ctx,
            &mut 10_000,
            || false,
        )
        .unwrap();
        let raw = ctx.qualify_family(&sealed.record.after, || false).unwrap();
        assert_eq!(
            (next.instance_count(), next.module_count()),
            (total, modules)
        );
        assert_eq!((raw.instance_count(), raw.module_count()), (total, modules));
        next.validate_binding(&sealed.record.after).unwrap();
        parent.roots = sealed.record.after.clone();
        parent.commit = operation;
        proof = next;
    }
}
