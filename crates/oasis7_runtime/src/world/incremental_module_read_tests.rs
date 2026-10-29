use super::*;
use crate::ModuleReadContext;
use oasis7_wasm_abi::ModuleKind;
use oasis7_world_state::incremental_module_instance_fields::{
    ModuleInstallTargetRefV1 as T, ModuleInstanceFieldV1 as F, ModuleInstanceKeyV1,
    ModuleInstanceScalarRefV1 as S,
};

pub(super) fn seed_instances(
    world: &World,
    parent: &mut ExecutionParent,
    pages: &mut Pages,
    values: &mut Values,
    memory: &MemoryBudget,
) {
    let mut writes = Vec::new();
    world
        .write_iws_module_instance_genesis(
            &mut BodyMutationContext {
                sink: values,
                memory,
                io: &mut io(),
                cancelled: || false,
            },
            &mut 100,
            4096,
            |key, id| {
                writes.push(StateWriteV1 {
                    key: key.to_vec(),
                    before: None,
                    after: Some(id),
                });
                Ok(())
            },
        )
        .unwrap();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(parent.roots.state[0].clone(), writes, pages, 4 << 20).unwrap();
    parent.roots.state[0] = delta.after;
    for node in delta.new_nodes {
        pages.0.insert(node.digest, node.bytes);
    }
}
pub(super) fn instance() -> oasis7_world_state::ModuleInstanceState {
    oasis7_world_state::ModuleInstanceState {
        instance_id: "module#stable-2".into(),
        module_id: "module".into(),
        module_version: "2.0".into(),
        wasm_hash: "preserved-sha256".into(),
        owner_agent_id: "alice".into(),
        install_target:
            oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
                location_id: "  place  ".into(),
            },
        active: false,
        installed_at: u64::MAX,
    }
}
fn setup_instances(memory: &MemoryBudget) -> (World, ExecutionParent, Pages, Values) {
    let (mut world, mut parent, mut pages, mut values) = setup(memory);
    world
        .state
        .module_instances
        .insert("map-key".into(), instance());
    seed_instances(&world, &mut parent, &mut pages, &mut values, memory);
    (world, parent, pages, values)
}
#[test]
fn authenticated_instance_fields_survive_cache_changes_and_support_one_field_overlay_reads() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (mut world, parent, pages, mut values) = setup_instances(&memory);
    world.state.module_instances.clear();
    world.state.module_states.clear();
    let source = values.clone();
    source.reads.set(0);
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    let mut quota = io();
    let row = ModuleReadContext {
        values: &source,
        memory: &memory,
        io: &mut quota,
        max_value_bytes: 4096,
        cancelled: || false,
    }
    .read_instance("map-key", &mut txn)
    .unwrap()
    .unwrap();
    assert!(row.uses_budget(&memory));
    assert_eq!(row.field(F::InstanceId), S::Text("module#stable-2"));
    assert_eq!(row.field(F::ModuleId), S::Text("module"));
    assert_eq!(row.field(F::Version), S::Text("2.0"));
    assert_eq!(row.field(F::WasmHash), S::Text("preserved-sha256"));
    assert_eq!(row.field(F::Owner), S::Text("alice"));
    assert_eq!(
        row.field(F::Target),
        S::Target(T::LocationInfrastructure("  place  "))
    );
    assert_eq!(row.field(F::Active), S::Bool(false));
    assert_eq!(row.field(F::InstalledAt), S::U64(u64::MAX));
    assert_eq!(source.reads.get(), 16);
    let active = crate::world::incremental_module_instance_genesis::write_module_instance_scalar(
        F::Active,
        S::Bool(true),
        &mut BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut io(),
            cancelled: || false,
        },
        4096,
    )
    .unwrap();
    let key = ModuleInstanceKeyV1::new("map-key", F::Active).unwrap();
    txn.write(Ns::Business, key.as_bytes(), Some(active))
        .unwrap();
    source.reads.set(0);
    let value = ModuleReadContext {
        values: &source,
        memory: &memory,
        io: &mut quota,
        max_value_bytes: 4096,
        cancelled: || false,
    }
    .read_instance_field("map-key", F::Active, &mut txn)
    .unwrap()
    .unwrap();
    assert_eq!(
        S::from_canonical_bytes(&value, F::Active).unwrap(),
        S::Bool(true)
    );
    assert_eq!(source.reads.get(), 2);
    assert_eq!(row.field(F::Active), S::Bool(false));
    drop(value);
    drop(row);
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.writes.len(), 1);
    assert!(world.state.module_instances.is_empty());
}
#[test]
fn pure_does_not_read_state_while_reducer_distinguishes_absence_empty_and_missing_data() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, values) = setup(&memory);
    let mut bounds = limits();
    bounds.max_reads = 0;
    bounds.max_writes = 0;
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, bounds).unwrap();
    let mut no_io = io();
    no_io.nodes_remaining = 0;
    no_io.read_bytes_remaining = 0;
    no_io.write_bytes_remaining = 0;
    values.reads.set(0);
    assert!(
        ModuleReadContext {
            values: &values,
            memory: &memory,
            io: &mut no_io,
            max_value_bytes: 0,
            cancelled: || false
        }
        .read_state(&"x".repeat(1024), &ModuleKind::Pure, &mut txn)
        .unwrap()
        .is_none()
    );
    assert_eq!(values.reads.get(), 0);
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert!(prepared.record.reads.is_empty());
    drop(prepared);
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let mut quota = io();
    values.reads.set(0);
    let absent = ModuleReadContext {
        values: &values,
        memory: &memory,
        io: &mut quota,
        max_value_bytes: 1000,
        cancelled: || false,
    }
    .read_state("absent", &ModuleKind::Reducer, &mut txn)
    .unwrap()
    .unwrap();
    assert!(absent.is_empty());
    assert!(absent.uses_budget(&memory));
    assert_eq!(values.reads.get(), 0);
    drop(absent);
    let empty = ModuleReadContext {
        values: &values,
        memory: &memory,
        io: &mut quota,
        max_value_bytes: 1000,
        cancelled: || false,
    }
    .read_state("other", &ModuleKind::Reducer, &mut txn)
    .unwrap()
    .unwrap();
    assert!(empty.is_empty());
    assert!(values.reads.get() > 0);
    drop(empty);
    drop(txn);
    let key = ModuleStateKeyV1::new("other").unwrap();
    let id = parent.roots.state[0]
        .get(key.as_bytes(), &pages)
        .unwrap()
        .unwrap();
    let root = values.pages.borrow()[&id].clone();
    let oasis7_formal_protocol::incremental_value::ValuePageLinksV1::Root(child) =
        oasis7_formal_protocol::incremental_value::verified_value_links(&id, &root).unwrap()
    else {
        panic!()
    };
    values.pages.borrow_mut().remove(&child.id);
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    assert!(
        ModuleReadContext {
            values: &values,
            memory: &memory,
            io: &mut io(),
            max_value_bytes: 1000,
            cancelled: || false
        }
        .read_state("other", &ModuleKind::Reducer, &mut txn)
        .is_err()
    );
    let guard = txn.guard();
    assert!(txn.seal(&guard, &live, 0).is_err());
}
#[test]
fn metadata_read_exact_io_and_each_one_less_dimension_fail_closed() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, values) = setup_instances(&memory);
    let mut quota = io();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let before = memory.used();
    let row = ModuleReadContext {
        values: &values,
        memory: &memory,
        io: &mut quota,
        max_value_bytes: 4096,
        cancelled: || false,
    }
    .read_instance("map-key", &mut txn)
    .unwrap()
    .unwrap();
    assert!(memory.used() > before);
    drop(row);
    drop(txn);
    let spent = (
        io().nodes_remaining - quota.nodes_remaining,
        io().read_bytes_remaining - quota.read_bytes_remaining,
        io().write_bytes_remaining - quota.write_bytes_remaining,
    );
    for dimension in 0..4 {
        let mut exact = oasis7_formal_protocol::incremental_value::ValueIoBudget {
            nodes_remaining: spent.0,
            read_bytes_remaining: spent.1,
            write_bytes_remaining: spent.2,
        };
        if dimension > 0 {
            match dimension {
                1 => exact.nodes_remaining -= 1,
                2 => exact.read_bytes_remaining -= 1,
                _ => exact.write_bytes_remaining -= 1,
            }
        }
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits())
                .unwrap();
        let result = ModuleReadContext {
            values: &values,
            memory: &memory,
            io: &mut exact,
            max_value_bytes: 4096,
            cancelled: || false,
        }
        .read_instance("map-key", &mut txn);
        if dimension == 0 {
            drop(result.unwrap().unwrap());
            assert_eq!(
                (
                    exact.nodes_remaining,
                    exact.read_bytes_remaining,
                    exact.write_bytes_remaining
                ),
                (0, 0, 0)
            );
        } else {
            assert!(result.is_err());
            let guard = txn.guard();
            assert!(txn.seal(&guard, &live, 0).is_err());
        }
    }
}
#[test]
fn absent_rows_partial_rows_wrong_types_limits_pool_and_cancel_never_fall_back() {
    for failure in 0..7 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (_, mut parent, mut pages, mut values) = setup_instances(&memory);
        if failure == 0 || failure == 1 {
            let key = ModuleInstanceKeyV1::new("map-key", F::Version).unwrap();
            let before = parent.roots.state[0].get(key.as_bytes(), &pages).unwrap();
            let after = if failure == 0 {
                None
            } else {
                Some(
                    write_field_bytes(
                        b"wrong encoded field",
                        &mut BodyMutationContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut io(),
                            cancelled: || false,
                        },
                    )
                    .unwrap(),
                )
            };
            let delta = StateDeltaV1::build(
                parent.roots.state[0].clone(),
                vec![StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before,
                    after,
                }],
                &pages,
                4 << 20,
            )
            .unwrap();
            parent.roots.state[0] = delta.after;
            for node in delta.new_nodes {
                pages.0.insert(node.digest, node.bytes);
            }
        }
        let alien = MemoryBudget::new(32 << 20, 0).unwrap();
        let live = parent.fence().unwrap();
        let mut lim = limits();
        if failure == 6 {
            lim.max_reads = 7;
        }
        let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, lim).unwrap();
        if failure != 6 {
            txn.write(Ns::Effects, b"prior", Some([1; 32])).unwrap();
        }
        let bad_key = "x".repeat(1024);
        let result = ModuleReadContext {
            values: &values,
            memory: if failure == 3 { &alien } else { &memory },
            io: &mut io(),
            max_value_bytes: if failure == 2 { 1 } else { 4096 },
            cancelled: || failure == 4,
        }
        .read_instance(if failure == 5 { &bad_key } else { "map-key" }, &mut txn);
        assert!(result.is_err(), "failure {failure}");
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
    }
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, values) = setup_instances(&memory);
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    let mut quota = io();
    values.reads.set(0);
    assert!(
        ModuleReadContext {
            values: &values,
            memory: &memory,
            io: &mut quota,
            max_value_bytes: 4096,
            cancelled: || false
        }
        .read_instance("absent", &mut txn)
        .unwrap()
        .is_none()
    );
    assert_eq!(values.reads.get(), 0);
}

struct InterruptingValues<'a> {
    values: &'a Values,
    calls: Cell<usize>,
    panic_at: Option<usize>,
    cancel_at: Option<usize>,
    cancelled: &'a Cell<bool>,
}
impl ValueNodeReader for InterruptingValues<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.calls.set(self.calls.get() + 1);
        assert_ne!(
            Some(self.calls.get()),
            self.panic_at,
            "injected module read unwind"
        );
        let value = self.values.read_node(id, max, memory, class)?;
        if Some(self.calls.get()) == self.cancel_at {
            self.cancelled.set(true);
        }
        Ok(value)
    }
}
#[test]
fn reader_unwind_and_last_field_cancel_release_all_partial_output_and_spend_io() {
    for panic in [false, true] {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (_, parent, pages, values) = setup_instances(&memory);
        let baseline = memory.used();
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        let cancelled = Cell::new(false);
        let source = InterruptingValues {
            values: &values,
            calls: Cell::new(0),
            panic_at: panic.then_some(3),
            cancel_at: (!panic).then_some(16),
            cancelled: &cancelled,
        };
        let mut quota = io();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ModuleReadContext {
                values: &source,
                memory: &memory,
                io: &mut quota,
                max_value_bytes: 4096,
                cancelled: || cancelled.get(),
            }
            .read_instance("map-key", &mut txn)
        }));
        if panic {
            assert!(result.is_err());
            assert_eq!(quota.nodes_remaining, io().nodes_remaining - 3);
        } else {
            assert!(matches!(
                result.unwrap(),
                Err(BodyMutationError::Value(ValueTreeError::Cancelled))
            ));
            assert_eq!(source.calls.get(), 16);
        }
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
        drop(guard);
        assert_eq!(memory.used(), baseline);
    }
}

#[test]
fn instance_genesis_entry_scalar_and_emitter_limits_leave_only_provisional_outputs() {
    let memory = MemoryBudget::new(8 << 20, 2 << 20).unwrap();
    let mut world = World::new();
    world
        .state
        .module_instances
        .insert("one".into(), instance());
    let mut values = Values::default();
    let mut emitted = 0;
    let count = world
        .write_iws_module_instance_genesis(
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
            &mut 1,
            4096,
            |key, _| {
                ModuleInstanceKeyV1::from_bytes(key).unwrap();
                emitted += 1;
                Ok(())
            },
        )
        .unwrap();
    assert_eq!((count, emitted), (1, 8));
    for failure in 0..4 {
        let mut emitted = 0;
        let mut remaining = if failure == 0 { 0 } else { 1 };
        let result = world.write_iws_module_instance_genesis(
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || failure == 2,
            },
            &mut remaining,
            if failure == 1 { 1 } else { 4096 },
            |_, _| {
                emitted += 1;
                if failure == 3 {
                    Err(BodyMutationError::Invalid("emitter failed"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(emitted, usize::from(failure == 3));
    }
    assert_eq!(
        world.state.module_instances["one"].instance_id,
        "module#stable-2"
    );
}

struct CancellingState<'a> {
    pages: &'a Pages,
    cancelled: &'a Cell<bool>,
}
impl MerkleNodeReader for CancellingState<'_> {
    type Node = Vec<u8>;
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let result = self.pages.read_node(id, max);
        self.cancelled.set(true);
        result
    }
}
#[test]
fn cancellation_during_authenticated_absence_never_returns_a_successful_missing_row() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, values) = setup_instances(&memory);
    for whole_row in [false, true] {
        let cancelled = Cell::new(false);
        let state = CancellingState {
            pages: &pages,
            cancelled: &cancelled,
        };
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [4; 32], &state, &memory, limits())
                .unwrap();
        let mut quota = io();
        let mut context = ModuleReadContext {
            values: &values,
            memory: &memory,
            io: &mut quota,
            max_value_bytes: 4096,
            cancelled: || cancelled.get(),
        };
        if whole_row {
            assert!(matches!(
                context.read_instance("absent", &mut txn),
                Err(BodyMutationError::Value(ValueTreeError::Cancelled))
            ));
        } else {
            assert!(matches!(
                context.read_instance_field("absent", F::Active, &mut txn),
                Err(BodyMutationError::Value(ValueTreeError::Cancelled))
            ));
        }
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
    }
}
