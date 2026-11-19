use super::*;
use crate::world::incremental_body::tests::fixture;
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateDeltaV1, StatePageWriter, StateWriteV1},
    incremental_value::{ValueIoBudget, ValueNodeReader},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};
struct Store {
    memory: MemoryBudget,
    nodes: RefCell<BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>>,
    _metadata: MemoryReservation,
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl Store {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            memory: memory.clone(),
            nodes: RefCell::new(BTreeMap::new()),
            _metadata: memory.reserve(65536 * 256, MemoryClass::Ordinary).unwrap(),
        }
    }
}
impl ValueNodeWriter for Store {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if !value.uses_budget(&self.memory) {
            return Err(ValueTreeError::Invalid("instance GEN fixture pool"));
        }
        let mut nodes = self.nodes.borrow_mut();
        if !nodes.contains_key(&id) && nodes.len() >= 65536 {
            return Err(ValueTreeError::ResourceLimited(
                "instance GEN fixture pages",
            ));
        }
        nodes.insert(id, Arc::new(value));
        Ok(())
    }
}
impl ValueNodeReader for Store {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Ordinary || !memory.same_pool(&self.memory) {
            return Err(ValueTreeError::Invalid("instance GEN fixture read pool"));
        }
        let value = self
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("instance GEN missing value"))?;
        if value.len() > max {
            return Err(ValueTreeError::Invalid("instance GEN page bound"));
        }
        Ok(value)
    }
}
impl StatePageWriter for Store {}
impl MerkleNodeReader for Store {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        self.nodes
            .borrow()
            .get(&id)
            .map(|v| {
                if v.len() > max {
                    Err("instance GEN page bound".into())
                } else {
                    Ok(Node(v.clone()))
                }
            })
            .transpose()
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 500000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
fn instance(module: &str, active: bool) -> oasis7_world_state::ModuleInstanceState {
    oasis7_world_state::ModuleInstanceState {
        instance_id: "inner-instance-id".into(),
        module_id: module.into(),
        module_version: "original version".into(),
        wasm_hash: "original sha256".into(),
        owner_agent_id: "alice".into(),
        install_target:
            oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
                location_id: " original location ".into(),
            },
        active,
        installed_at: u64::MAX,
    }
}
#[test]
fn streaming_instance_genesis_matches_original_roots_empty_inactive_and_duplicate_modules() {
    for nonempty in [false, true] {
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let _source = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let (mut world, parent, pages) = fixture(&memory);
        world.state.module_instances.clear();
        if nonempty {
            world
                .state
                .module_instances
                .insert("outer-one".into(), instance(" module ", false));
            world
                .state
                .module_instances
                .insert("outer-two".into(), instance(" module ", true));
            world
                .state
                .module_instances
                .insert("outer-three".into(), instance("", false));
        }
        let mut store = Store::new(&memory);
        for (id, bytes) in &pages.0 {
            let id = *blake3::Hash::from_hex(id).unwrap().as_bytes();
            store
                .put_node(
                    id,
                    memory
                        .reserve(bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(bytes.clone()),
                )
                .unwrap();
        }
        let base = parent.roots.state_root(StateNamespaceV1::Business);
        let mut remaining = if nonempty { 3 } else { 0 };
        let original = world
            .write_iws_module_instance_genesis(
                base.clone(),
                &mut BodyMutationContext {
                    sink: &mut store,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                &mut remaining,
                4096,
            )
            .unwrap();
        assert_eq!(remaining, 0);
        let mut writes = Vec::with_capacity(32);
        let mut remaining = if nonempty { 3 } else { 0 };
        let mut count_rows = if nonempty { 2 } else { 0 };
        let total = world
            .write_iws_module_instance_genesis_rows(
                &mut BodyMutationContext {
                    sink: &mut store,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                &mut remaining,
                &mut count_rows,
                4096,
                &mut |namespace, key, value| {
                    assert_eq!(namespace, StateNamespaceV1::Business);
                    assert!(writes.len() < 32);
                    writes.push(StateWriteV1 {
                        key: key.to_vec(),
                        before: None,
                        after: Some(value),
                    });
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(total, if nonempty { 3 } else { 0 });
        assert_eq!(remaining, 0);
        assert_eq!(count_rows, 0);
        assert_eq!(writes.len(), if nonempty { 27 } else { 1 });
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        assert!(writes.windows(2).all(|w| w[0].key != w[1].key));
        let delta = StateDeltaV1::build(base.clone(), writes, &store, 8 << 20).unwrap();
        assert_eq!(&delta.after, &*original);
    }
}
#[test]
fn module_id_hash_identity_is_chunk_cancellable_without_changing_original_key_bytes() {
    let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
    let _source = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let long = "x".repeat(8193);
    for id in ["", " module ", "MODULE", "模块", "a\0b", long.as_str()] {
        assert_eq!(
            count_key_cancelled(id, &mut || false).unwrap(),
            module_instance_count_key(id)
        );
    }
    let polls = Cell::new(0);
    assert!(matches!(
        count_key_cancelled(&"x".repeat(1 << 20), &mut || {
            polls.set(polls.get() + 1);
            polls.get() == 4
        }),
        Err(BodyMutationError::Value(ValueTreeError::Cancelled))
    ));
    assert_eq!(polls.get(), 4);
}
#[test]
fn instance_genesis_count_allowances_and_cancellation_do_not_return_completed_rows() {
    for case in 0..4 {
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let _source = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let (mut world, _, _) = fixture(&memory);
        world.state.module_instances.clear();
        world
            .state
            .module_instances
            .insert("outer-one".into(), instance("a", false));
        world
            .state
            .module_instances
            .insert("outer-two".into(), instance("b", false));
        let mut store = Store::new(&memory);
        let mut remaining = if case == 0 { 1 } else { 2 };
        let mut counts = if case == 1 { 1 } else { 2 };
        let emitted = Cell::new(0);
        let index = Cell::new(false);
        let result = world.write_iws_module_instance_genesis_rows(
            &mut BodyMutationContext {
                sink: &mut store,
                memory: &memory,
                io: &mut io(),
                cancelled: || case == 2,
            },
            &mut remaining,
            &mut counts,
            4096,
            &mut |_, key, _| {
                emitted.set(emitted.get() + 1);
                index.set(key == MODULE_INSTANCE_INDEX_KEY_V1);
                if case == 3 {
                    return Err(BodyMutationError::Invalid("injected instance GEN sink"));
                }
                Ok(())
            },
        );
        assert!(result.is_err(), "case {case}");
        assert!(!index.get());
        if case == 0 || case == 2 {
            assert_eq!(emitted.get(), 0);
        } else {
            assert!(emitted.get() > 0);
        }
    }
}

#[test]
fn instance_genesis_polls_inside_the_actual_long_module_count_hash() {
    for stop in [false, true] {
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let _source = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let (mut world, _, _) = fixture(&memory);
        world.state.module_instances.clear();
        let module = "m".repeat(128 << 10);
        world
            .state
            .module_instances
            .insert("outer-key".into(), instance(&module, false));
        let mut store = Store::new(&memory);
        let count_phase = Cell::new(false);
        let polls = Cell::new(0);
        let rows = Cell::new(0);
        let mut instances = 1;
        let mut count_rows = 1;
        let result=world.write_iws_module_instance_genesis_rows(
            &mut BodyMutationContext{sink:&mut store,memory:&memory,io:&mut io(),cancelled:||{
                if count_phase.get(){polls.set(polls.get()+1);stop&&polls.get()>=4}else{false}
            }},&mut instances,&mut count_rows,1<<20,&mut|_,key,_|{
                rows.set(rows.get()+1);
                if let Ok((_,field))=ModuleInstanceKeyV1::from_bytes(key) {
                    if field==oasis7_world_state::incremental_module_instance_fields::ModuleInstanceFieldV1::InstalledAt {count_phase.set(true);}
                }
                Ok(())
            });
        if stop {
            assert!(matches!(
                result,
                Err(BodyMutationError::Value(ValueTreeError::Cancelled))
            ));
            assert_eq!(rows.get(), 8);
            assert_eq!(polls.get(), 4);
        } else {
            assert_eq!(result.unwrap(), 1);
            assert_eq!(rows.get(), 10);
            assert!(
                polls.get() >= module.len() / 4096,
                "hash was not chunk-polled: {}",
                polls.get()
            );
        }
    }
}
