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
fn real_reopened_instance_family_verifies_with_only_recovery_reserve_and_no_cache() {
    use super::super::super::physical::{Directory, install_fixture_pages, physical_io};
    use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression};
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (mut world, mut parent, pages, values) = setup(&memory);
        for (key, module_id) in [("map-key", "module"), ("different", "module"), ("", "模块")] {
            let mut row = instance();
            row.module_id = module_id.into();
            world.state.module_instances.insert(key.into(), row);
        }
        world
            .module_registry
            .active
            .insert("global".into(), "stored version".into());
        world.state.installed_module_targets.insert(
            "global".into(),
            oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
                location_id: "stored place".into(),
            },
        );
        let expected_rows = world.state.module_instances.len() as u64;
        let expected_modules = world
            .state
            .module_instances
            .values()
            .map(|row| &row.module_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len() as u64;
        let path = directory.0.join("pages");
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        install_fixture_pages(&store, &memory, &pages, &values);
        // Exercise the real bounded genesis writer, rather than importing a
        // complete instance tree produced by the in-memory fixture adapter.
        let mut sink = store.value_session(physical_io());
        let mut entries = expected_rows;
        let root = world
            .write_iws_module_instance_genesis(
                parent.roots.state[0].clone(),
                &mut BodyMutationContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut genesis_io(),
                    cancelled: || false,
                },
                &mut entries,
                4096,
            )
            .unwrap();
        parent.roots.state[0] = (*root).clone();
        drop(root);
        let root = world
            .write_iws_module_selection_genesis(
                parent.roots.state[5].clone(),
                &mut BodyMutationContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut genesis_io(),
                    cancelled: || false,
                },
                &mut 2,
                4096,
            )
            .unwrap();
        parent.roots.state[5] = (*root).clone();
        drop(root);
        drop(sink);
        drop(store);
        drop(values);
        drop(pages);
        world.state.module_instances.clear();
        world.module_registry.active.clear();
        world.state.installed_module_targets.clear();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        // Each state/counter page has one frame. A packed recipe/frame
        // member needs magic + header + <=9 binary-search rows + payload:
        // <=24 physical reads per logical page. The old body fixture's
        // 100k request cap was sized for individual files, not this full scan.
        let mut recovery_physical = physical_io();
        recovery_physical.reads_remaining =
            24 * (state_io().nodes_remaining + scratch_io().reads + scratch_io().writes);
        let state = store
            .state_session(recovery_physical.clone(), MemoryClass::Recovery)
            .unwrap();
        let scratch = store
            .state_session(recovery_physical.clone(), MemoryClass::Recovery)
            .unwrap();
        let source = store.value_session(physical_io());
        let ordinary_full = memory
            .reserve((28 << 20) - memory.used(), MemoryClass::Ordinary)
            .unwrap();
        assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
        let before = memory.used();
        let mut scan = state_io();
        let mut scratch_allowance = scratch_io();
        let content = ModuleInstanceRecoveryContext {
            root: &parent.roots.state[0],
            state: &state,
            values: &source,
            scratch: &scratch,
            memory: &memory,
            class: MemoryClass::Recovery,
            state_io: &mut scan,
            value_io: &mut genesis_io(),
            scratch_io: &mut scratch_allowance,
            max_scalar_bytes: 4096,
        }
        .verify(|| false)
        .unwrap();
        let state_physical = state.remaining_io().unwrap();
        let scratch_physical = scratch.remaining_io().unwrap();
        let used_requests = 2 * recovery_physical.reads_remaining
            - state_physical.reads_remaining
            - scratch_physical.reads_remaining;
        let logical_pages = state_io().nodes_remaining - scan.nodes_remaining + scratch_io().reads
            - scratch_allowance.reads
            + scratch_io().writes
            - scratch_allowance.writes;
        assert!(used_requests <= 24 * logical_pages);
        eprintln!(
            "instance recovery packed physical requests={used_requests}, logical pages={logical_pages}"
        );
        assert_eq!(content.instances, expected_rows);
        assert_eq!(content.modules, expected_modules);
        assert!(content.uses_budget(&memory));
        drop(content);
        assert_eq!(memory.used(), before);
        assert!(scratch.remaining_io().unwrap().writes_remaining < physical_io().writes_remaining);
        drop(ordinary_full);
        let ordinary_state = store
            .state_session(physical_io(), MemoryClass::Ordinary)
            .unwrap();
        let mut txn = ExecutionTransaction::begin(
            parent.clone(),
            [4; 32],
            &ordinary_state,
            &memory,
            limits(),
        )
        .unwrap();
        let mut value_io = genesis_io();
        let mut selection = crate::ModuleReadContext {
            values: &source,
            memory: &memory,
            io: &mut value_io,
            max_value_bytes: 4096,
            cancelled: || false,
        };
        assert!(
            selection
                .select_module_content("map-key", &mut txn)
                .unwrap()
                .is_none()
        );
        let selected = selection
            .select_module_content("global", &mut txn)
            .unwrap()
            .unwrap();
        assert_eq!(selected.module_id(), "global");
        assert_eq!(selected.record_version(), "stored version");
        assert_eq!(selected.target(), T::LocationInfrastructure("stored place"));
        assert!(!selected.is_instance());
    }
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
