use super::incremental_body::write_field_bytes;
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError},
};
use crate::ModuleReadContext;
use crate::world::incremental_body::tests::{Pages, fixture, limits};
use oasis7_formal_protocol::incremental_state::{StateDeltaV1, StateWriteV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionParent, ExecutionTransaction},
    incremental_state::{StateNamespaceV1 as Ns, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::incremental_module_instance_fields::{
    ModuleInstallTargetRefV1 as T, ModuleInstanceFieldV1 as F, ModuleInstanceKeyV1,
    ModuleInstanceScalarRefV1 as S,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};
type ValuePages = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
#[derive(Clone, Default)]
struct Values {
    pages: Rc<RefCell<ValuePages>>,
    reads: Rc<Cell<usize>>,
}
impl StatePageWriter for Values {}
impl ValueNodeWriter for Values {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.pages.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Values {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.reads.set(self.reads.get() + 1);
        let pages = self.pages.borrow();
        let p = pages
            .get(id)
            .ok_or(ValueTreeError::Invalid("instance test missing page"))?;
        if p.len() > max {
            return Err(ValueTreeError::Invalid("instance test page bound"));
        }
        Ok(Arc::new(memory.reserve(p.len(), class)?.retain(p.to_vec())))
    }
}
fn io() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    genesis_io()
}
fn setup(memory: &MemoryBudget) -> (World, ExecutionParent, Pages, Values) {
    let (w, p, s) = fixture(memory);
    (w, p, s, Values::default())
}
fn genesis_io() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    oasis7_formal_protocol::incremental_value::ValueIoBudget {
        nodes_remaining: 50_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}
fn seed_instances(
    world: &World,
    parent: &mut ExecutionParent,
    pages: &mut Pages,
    values: &mut Values,
    memory: &MemoryBudget,
) {
    // State/Value pages share the same immutable CAS in the real backend.
    // Mirror the fixture's already prepared parent state pages into that CAS.
    for (id, bytes) in &pages.0 {
        let id: [u8; 32] = hex::decode(id).unwrap().try_into().unwrap();
        let charge = memory.reserve(bytes.len(), MemoryClass::Ordinary).unwrap();
        values.put_node(id, charge.retain(bytes.clone())).unwrap();
    }
    let mut quota = genesis_io();
    let root = world
        .write_iws_module_instance_genesis(
            parent.roots.state[0].clone(),
            &mut BodyMutationContext {
                sink: values,
                memory,
                io: &mut quota,
                cancelled: || false,
            },
            &mut 100,
            4096,
        )
        .unwrap();
    parent.roots.state[0] = (*root).clone();
    for (id, bytes) in values.pages.borrow().iter() {
        if bytes.starts_with(b"oasis7.iws.state-page.v1\0") {
            pages.0.insert(hex::encode(id), bytes.to_vec());
        }
    }
}
fn instance() -> oasis7_world_state::ModuleInstanceState {
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
fn instance_counts_follow_inactive_creation_module_move_and_last_deletion_in_one_overlay() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (mut world, parent, pages, mut values) = setup_instances(&memory);
    world.state.module_instances.clear();
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut bounds = limits();
    bounds.max_reads = 128;
    bounds.max_writes = 128;
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, bounds).unwrap();
    let mut quota = genesis_io();
    let count = |id: &str, txn: &mut ExecutionTransaction<'_, Pages>, quota: &mut _| {
        ModuleReadContext {
            values: &source,
            memory: &memory,
            io: quota,
            max_value_bytes: 4096,
            cancelled: || false,
        }
        .module_instance_count(id, txn)
        .unwrap()
    };
    assert_eq!(count("module", &mut txn, &mut quota), 1);
    assert_eq!(count("absent", &mut txn, &mut quota), 0);
    let mut second = instance();
    second.instance_id = "different stored identity".into();
    for (key, after, expected_old, expected_new) in [
        ("second", Some(second.clone()), 2, 0),
        (
            "map-key",
            Some({
                let mut moved = instance();
                moved.module_id = "新 module".into();
                moved
            }),
            1,
            1,
        ),
        ("second", None, 0, 1),
        ("map-key", None, 0, 0),
    ] {
        crate::contribute_module_instance_change(
            key,
            after.as_ref(),
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut quota,
                cancelled: || false,
            },
            4096,
        )
        .unwrap();
        assert_eq!(count("module", &mut txn, &mut quota), expected_old);
        assert_eq!(count("新 module", &mut txn, &mut quota), expected_new);
    }
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert!(!prepared.record.writes.is_empty());
    assert!(world.state.module_instances.is_empty());
}

#[test]
fn instance_change_output_failure_poison_includes_prior_outer_write() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (_, parent, pages, mut values) = setup_instances(&memory);
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    txn.write(Ns::Business, b"prior-outer-write", Some([8; 32]))
        .unwrap();
    let mut changed = instance();
    changed.owner_agent_id = "x".repeat(5000);
    let mut quota = genesis_io();
    assert!(
        crate::contribute_module_instance_change(
            "map-key",
            Some(&changed),
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut quota,
                cancelled: || false
            },
            4096,
        )
        .is_err()
    );
    let guard = txn.guard();
    assert!(txn.seal(&guard, &live, 0).is_err());
}

#[path = "incremental_module_instance_recovery_tests.rs"]
mod recovery;
