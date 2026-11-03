use super::*;
use crate::world::{
    incremental_scheduler_index as index, incremental_scheduler_recovery::ModuleScheduleReadContext,
};
use oasis7_formal_protocol::{
    incremental_schedule::{ScheduleIndexV1, ScheduleScanBudget},
    incremental_state::StateScanBudget,
};

struct ChargedState<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
struct ChargedPage(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for ChargedPage {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for ChargedState<'_> {
    type Node = ChargedPage;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let bytes = self.pages.0.get(id).ok_or("state page missing")?;
        if bytes.len() > max {
            return Err("state page bound".into());
        }
        let charge = self
            .memory
            .reserve(bytes.len(), MemoryClass::Recovery)
            .map_err(|e| e.to_string())?;
        Ok(Some(ChargedPage(charge.retain(bytes.clone()))))
    }
}
struct Quotas {
    state: StateScanBudget,
    values: oasis7_formal_protocol::incremental_value::ValueIoBudget,
    index: ScheduleScanBudget,
}
impl Quotas {
    fn new() -> Self {
        Self {
            state: StateScanBudget {
                nodes_remaining: 50_000,
                read_bytes_remaining: 50_000_000,
                leaves_remaining: 1000,
            },
            values: io(),
            index: ScheduleScanBudget {
                nodes_remaining: 50_000,
                read_bytes_remaining: 50_000_000,
                buckets_remaining: 1000,
                instances_remaining: 1000,
            },
        }
    }
    fn spent(&self) -> Self {
        let initial = Self::new();
        Self {
            state: StateScanBudget {
                nodes_remaining: initial.state.nodes_remaining - self.state.nodes_remaining,
                read_bytes_remaining: initial.state.read_bytes_remaining
                    - self.state.read_bytes_remaining,
                leaves_remaining: initial.state.leaves_remaining - self.state.leaves_remaining,
            },
            values: oasis7_formal_protocol::incremental_value::ValueIoBudget {
                nodes_remaining: initial.values.nodes_remaining - self.values.nodes_remaining,
                read_bytes_remaining: initial.values.read_bytes_remaining
                    - self.values.read_bytes_remaining,
                write_bytes_remaining: initial.values.write_bytes_remaining
                    - self.values.write_bytes_remaining,
            },
            index: ScheduleScanBudget {
                nodes_remaining: initial.index.nodes_remaining - self.index.nodes_remaining,
                read_bytes_remaining: initial.index.read_bytes_remaining
                    - self.index.read_bytes_remaining,
                buckets_remaining: initial.index.buckets_remaining - self.index.buckets_remaining,
                instances_remaining: initial.index.instances_remaining
                    - self.index.instances_remaining,
            },
        }
    }
}
fn verify<V: ValueNodeReader>(
    parent: &ExecutionParent,
    pages: &Pages,
    values: &V,
    memory: &MemoryBudget,
    quotas: &mut Quotas,
    cancelled: impl FnMut() -> bool,
) -> Result<
    ReservedValue<crate::world::incremental_scheduler_recovery::VerifiedModuleScheduleContent>,
    BodyMutationError,
> {
    ModuleScheduleReadContext {
        state_root: &parent.roots.state[2],
        state_source: &ChargedState { pages, memory },
        values,
        memory,
        class: MemoryClass::Recovery,
        state_io: &mut quotas.state,
        value_io: &mut quotas.values,
        index_io: &mut quotas.index,
    }
    .verify(cancelled)
}
fn setup(memory: &MemoryBudget) -> (World, ExecutionParent, Pages, Values) {
    let (mut world, mut parent, mut pages) = fixture(memory);
    world.module_tick_schedule.insert("m#1".into(), 0);
    world.module_tick_schedule.insert("m#2".into(), u64::MAX);
    let mut values = Values::default();
    seed_schedules(&world, &mut parent, &mut pages, &mut values, memory);
    write_field_bytes(
        &oasis7_world_state::incremental_agent_body::world_time_bytes_v1(world.state.time),
        &mut BodyMutationContext {
            sink: &mut values,
            memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    (world, parent, pages, values)
}
fn replace(parent: &mut ExecutionParent, pages: &mut Pages, key: Vec<u8>, after: Option<[u8; 32]>) {
    let before = parent.roots.state[2].get(&key, pages).unwrap();
    let delta = StateDeltaV1::build(
        parent.roots.state[2].clone(),
        vec![StateWriteV1 { key, before, after }],
        pages,
        4 << 20,
    )
    .unwrap();
    parent.roots.state[2] = delta.after;
    for node in delta.new_nodes {
        pages.0.insert(node.digest, node.bytes);
    }
}

#[test]
fn recovery_proves_bijection_with_exact_allowances_and_retains_output_charge() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, values) = setup(&memory);
    let baseline = memory.used();
    let mut quotas = Quotas::new();
    let content = verify(&parent, &pages, &values, &memory, &mut quotas, || false).unwrap();
    assert_eq!(content.scalar_entries, 2);
    assert_eq!(content.index.entries, 2);
    assert!(content.uses_budget(&memory));
    assert_eq!(memory.used(), baseline + content.reserved_bytes());
    drop(content);
    assert_eq!(memory.used(), baseline);
    let exact = quotas.spent();
    assert!(exact.state.leaves_remaining >= 6); // Both complete state passes.
    let mut precise = quotas.spent();
    drop(verify(&parent, &pages, &values, &memory, &mut precise, || false).unwrap());
    assert_eq!(
        (
            precise.state.nodes_remaining,
            precise.state.read_bytes_remaining,
            precise.state.leaves_remaining,
            precise.values.nodes_remaining,
            precise.values.read_bytes_remaining,
            precise.index.nodes_remaining,
            precise.index.read_bytes_remaining,
            precise.index.buckets_remaining,
            precise.index.instances_remaining
        ),
        (0, 0, 0, 0, 0, 0, 0, 0, 0)
    );
    for dimension in 0..10 {
        let mut short = quotas.spent();
        match dimension {
            0 => short.state.nodes_remaining -= 1,
            1 => short.state.read_bytes_remaining -= 1,
            2 => short.state.leaves_remaining -= 1,
            3 => short.values.nodes_remaining -= 1,
            4 => short.values.read_bytes_remaining -= 1,
            5 => short.values.write_bytes_remaining -= 1,
            6 => short.index.nodes_remaining -= 1,
            7 => short.index.read_bytes_remaining -= 1,
            8 => short.index.buckets_remaining -= 1,
            _ => short.index.instances_remaining -= 1,
        }
        assert!(
            verify(&parent, &pages, &values, &memory, &mut short, || false).is_err(),
            "dimension {dimension}"
        );
        assert_eq!(memory.used(), baseline);
    }
}

#[test]
fn recovery_rejects_missing_descriptor_scalar_mismatch_extra_and_duplicate_pairs() {
    for mode in 0..5 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (_, mut parent, mut pages, mut values) = setup(&memory);
        let mut quota = io();
        let mut ctx = BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut quota,
            cancelled: || false,
        };
        match mode {
            0 => replace(
                &mut parent,
                &mut pages,
                MODULE_WAKE_INDEX_KEY_V1.to_vec(),
                None,
            ),
            1 => {
                let id = write_field_bytes(&ModuleWakeV1(1).canonical_bytes(), &mut ctx).unwrap();
                replace(
                    &mut parent,
                    &mut pages,
                    ModuleWakeKeyV1::new("m#1").unwrap().as_bytes().to_vec(),
                    Some(id),
                );
            }
            2 => replace(
                &mut parent,
                &mut pages,
                ModuleWakeKeyV1::new("m#1").unwrap().as_bytes().to_vec(),
                None,
            ),
            _ => {
                let mut root = ScheduleIndexV1::empty().unwrap();
                // Adversarial content intentionally violates update's caller
                // precondition in mode 4: one instance at two different times.
                for (id, wake) in if mode == 4 {
                    [("m#1", 0), ("m#1", 1)]
                } else {
                    [("m#1", 0), ("extra", 1)]
                } {
                    let delta = index::build(&root, id, None, Some(wake), &mut ctx).unwrap();
                    index::persist(&delta, &mut ctx).unwrap();
                    root = delta.after.clone();
                }
                let id = write_field_bytes(&root.canonical_bytes().unwrap(), &mut ctx).unwrap();
                replace(
                    &mut parent,
                    &mut pages,
                    MODULE_WAKE_INDEX_KEY_V1.to_vec(),
                    Some(id),
                );
            }
        }
        assert!(
            verify(
                &parent,
                &pages,
                &values,
                &memory,
                &mut Quotas::new(),
                || false
            )
            .is_err(),
            "mode {mode}"
        );
    }
}

#[test]
fn empty_genesis_requires_an_explicit_empty_index_and_late_cancel_never_returns_content() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (world, mut parent, mut pages) = fixture(&memory);
    let mut values = Values::default();
    assert!(
        verify(
            &parent,
            &pages,
            &values,
            &memory,
            &mut Quotas::new(),
            || false
        )
        .is_err()
    );
    seed_schedules(&world, &mut parent, &mut pages, &mut values, &memory);
    let content = verify(
        &parent,
        &pages,
        &values,
        &memory,
        &mut Quotas::new(),
        || false,
    )
    .unwrap();
    assert_eq!(content.scalar_entries, 0);
    drop(content);
    let mut quotas = Quotas::new();
    assert!(verify(&parent, &pages, &values, &memory, &mut quotas, || true).is_err());
    assert_eq!(quotas.state.nodes_remaining, 50_000);
    let mut calls = 0;
    let mut all = Quotas::new();
    drop(
        verify(&parent, &pages, &values, &memory, &mut all, || {
            calls += 1;
            false
        })
        .unwrap(),
    );
    let mut cancelled_calls = 0;
    assert!(matches!(
        verify(
            &parent,
            &pages,
            &values,
            &memory,
            &mut Quotas::new(),
            || {
                cancelled_calls += 1;
                cancelled_calls == calls
            }
        ),
        Err(BodyMutationError::Value(ValueTreeError::Cancelled))
    ));
}

struct PanicValues<'a> {
    source: &'a Values,
    calls: Cell<u64>,
}
impl ValueNodeReader for PanicValues<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.calls.set(self.calls.get() + 1);
        assert_ne!(self.calls.get(), 3, "injected index read panic");
        self.source.read_node(id, max, memory, class)
    }
}
#[test]
fn recovery_source_unwind_keeps_work_spent_and_releases_workspace() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, values) = setup(&memory);
    let baseline = memory.used();
    let mut quotas = Quotas::new();
    let panicking = PanicValues {
        source: &values,
        calls: Cell::new(0),
    };
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| verify(
            &parent,
            &pages,
            &panicking,
            &memory,
            &mut quotas,
            || false
        )))
        .is_err()
    );
    assert!(quotas.state.nodes_remaining < 50_000);
    assert_eq!(quotas.index.nodes_remaining, 49_999);
    assert_eq!(memory.used(), baseline);
}

struct FailingSink {
    values: Values,
    failure: u8,
    cancelled: Rc<Cell<bool>>,
}
impl ValueNodeReader for FailingSink {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.values.read_node(id, max, memory, class)
    }
}
impl ValueNodeWriter for FailingSink {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.values.put_node(id, bytes)
    }
}
impl oasis7_formal_protocol::incremental_schedule::SchedulePageWriter for FailingSink {
    fn put_schedule_pages(
        &mut self,
        delta: &ReservedValue<oasis7_formal_protocol::incremental_schedule::ScheduleDeltaV1>,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        if matches!(self.failure, 4 | 5) {
            let first = &delta.new_nodes[0];
            let charge = memory.reserve(first.bytes.len(), MemoryClass::Ordinary)?;
            self.values.put_node(
                *blake3::hash(&first.bytes).as_bytes(),
                charge.retain(first.bytes.clone()),
            )?;
            assert_ne!(self.failure, 5, "injected schedule persistence unwind");
            return Err(ValueTreeError::Invalid(
                "injected partial index persistence",
            ));
        }
        self.values.put_schedule_pages(delta, memory)?;
        if self.failure == 6 {
            self.cancelled.set(true);
        }
        Ok(())
    }
}
#[test]
fn index_failures_and_partial_storage_poison_all_prior_transaction_work() {
    for failure in 0..7 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (world, mut parent, mut pages, mut values) = setup(&memory);
        if failure == 0 {
            replace(
                &mut parent,
                &mut pages,
                MODULE_WAKE_INDEX_KEY_V1.to_vec(),
                None,
            );
        }
        if failure == 1 {
            let id = write_field_bytes(
                &[0; oasis7_formal_protocol::incremental_schedule::SCHEDULE_ROOT_BYTES],
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
            )
            .unwrap();
            replace(
                &mut parent,
                &mut pages,
                MODULE_WAKE_INDEX_KEY_V1.to_vec(),
                Some(id),
            );
        }
        if failure == 2 {
            let id = parent.roots.state[2]
                .get(MODULE_WAKE_INDEX_KEY_V1, &pages)
                .unwrap()
                .unwrap();
            let index = decoded_index(id, &values, &memory);
            let id: [u8; 32] = hex::decode(index.node_root).unwrap().try_into().unwrap();
            values.pages.borrow_mut().remove(&id);
        }
        let source = values.clone();
        let cancelled = Rc::new(Cell::new(false));
        let mut sink = FailingSink {
            values,
            failure,
            cancelled: cancelled.clone(),
        };
        let mut quota = io();
        if failure == 3 {
            quota.write_bytes_remaining = 0;
        }
        let mut context = BodyMutationContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut quota,
            cancelled: || cancelled.get(),
        };
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        txn.write(Ns::Business, b"prior-work", Some([7; 32]))
            .unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.contribute_iws_module_wake("m#1", Some(1), &mut txn, &source, &mut context)
        }));
        if failure == 5 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err(), "failure {failure}");
        }
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
        assert_eq!(world.module_tick_schedule["m#1"], 0);
        assert_eq!(world.module_tick_schedule["m#2"], u64::MAX);
    }
}
