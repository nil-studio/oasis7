//! Component scope: backing setup is excluded; cold operation drops World/old pool.
use super::*;
use oasis7_distfs::incremental_page_store::*;
use oasis7_formal_protocol::{
    incremental_execution::TransitionVerifier,
    incremental_scratch_counter::{CounterScratchStore, ScratchCounterDelta},
    incremental_state::{MerkleNodeReader, StateRootV1, StateScanBudget},
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-instance-cold-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Node(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct Scratch<'a> {
    memory: &'a MemoryBudget,
    class: MemoryClass,
    pages: RefCell<BTreeMap<String, ReservedValue<Vec<u8>>>>,
}
impl MerkleNodeReader for Scratch<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let pages = self.pages.borrow();
        let Some(bytes) = pages.get(id) else {
            return Ok(None);
        };
        if bytes.len() > max {
            return Err("scratch page bound".into());
        }
        let charge = self
            .memory
            .reserve(bytes.len() + 128, self.class)
            .map_err(|e| e.to_string())?;
        Ok(Some(Node(charge.retain(bytes.to_vec()))))
    }
}
impl CounterScratchStore for Scratch<'_> {
    fn put_scratch_pages(&self, delta: &ReservedValue<ScratchCounterDelta>) -> Result<(), String> {
        if !delta.uses_budget(self.memory) {
            return Err("foreign scratch delta".into());
        }
        delta.verify_pages()?;
        let mut pages = self.pages.borrow_mut();
        for node in &delta.new_nodes {
            if !pages.contains_key(&node.digest) {
                // Includes owned key and conservative per-BTree-entry metadata.
                let charge = self
                    .memory
                    .reserve(node.bytes.len() + node.digest.len() + 512, self.class)
                    .map_err(|e| e.to_string())?;
                pages.insert(node.digest.clone(), charge.retain(node.bytes.clone()));
            }
        }
        Ok(())
    }
}
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 2_000_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 10_000,
    }
}
fn scratch_io() -> crate::InstanceScratchBudget {
    crate::InstanceScratchBudget {
        reads: 100_000,
        read_bytes: 64 << 20,
        writes: 100_000,
        write_bytes: 64 << 20,
    }
}
fn logical() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    oasis7_formal_protocol::incremental_value::ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
struct Forbid<'a, V> {
    source: &'a V,
    forbidden: [u8; 32],
}
impl<V: ValueNodeReader> ValueNodeReader for Forbid<'_, V> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if id == &self.forbidden {
            return Err(ValueTreeError::Invalid("unchanged instance version read"));
        }
        self.source.read_node(id, max, memory, class)
    }
}

#[test]
fn physical_instance_cold_recovery_and_fixed_delta_skip_unchanged_32m_version() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let mut baseline = None;
        for length in [64, 1 << 20, 32 << 20] {
            let directory = Directory::new();
            let path = directory.0.join("pages");
            let roots = {
                let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
                let store = IncrementalPageStore::create(&path, memory.clone(), compression)
                    .unwrap();
                let (mut world, mut parent, _, _) = setup(&memory);
                world.state.module_instances.clear();
                let mut value = instance();
                value.module_version = "v".repeat(length);
                world.state.module_instances.insert("map-key".into(), value);
                for root in &mut parent.roots.state {
                    *root = StateRootV1::empty(root.namespace).unwrap();
                }
                let mut writer = store.value_session(physical());
                let root = world
                    .write_iws_module_instance_genesis(
                        parent.roots.state_root(Ns::Business).clone(),
                        &mut BodyMutationContext {
                            sink: &mut writer,
                            memory: &memory,
                            io: &mut logical(),
                            cancelled: || false,
                        },
                        &mut 10,
                        (length + 4096) as u64,
                    )
                    .unwrap();
                *parent
                    .roots
                    .state
                    .iter_mut()
                    .find(|r| r.namespace == Ns::Business)
                    .unwrap() = (*root).clone();
                parent.roots
            };
            // No World, original DTO, backing pool or retained source pages survive.
            let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                cold.clone(),
                compression,
                MemoryClass::Recovery,
            )
            .unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Recovery)
                .unwrap();
            let values = store.value_session(physical());
            let scratch = Scratch {
                memory: &cold,
                class: MemoryClass::Recovery,
                pages: RefCell::new(BTreeMap::new()),
            };
            let recovered = crate::ModuleInstanceRecoveryContext {
                root: roots.state_root(Ns::Business),
                state: &state,
                values: &values,
                scratch: &scratch,
                memory: &cold,
                class: MemoryClass::Recovery,
                state_io: &mut scan(),
                value_io: &mut logical(),
                scratch_io: &mut scratch_io(),
                max_scalar_bytes: (length + 4096) as u64,
            }
            .qualify_family(&roots, || false)
            .unwrap();
            assert_eq!(recovered.instance_count(), 1);
            eprintln!(
                "instance bootstrap {compression:?} text={length} state={:?} values={:?}",
                state.remaining_io().unwrap(),
                values.remaining_io().unwrap()
            );
            drop((recovered, scratch, values, state));
            drop(store);
            drop(cold);

            // Zstd preserves its existing 16MiB write-workspace admission policy.
            // Raw ordinary uses 8MiB; Zstd ordinary uses the compliant 32MiB envelope.
            let memory = MemoryBudget::new(
                if matches!(compression, PageCompression::Raw) {
                    8 << 20
                } else {
                    32 << 20
                },
                4 << 20,
            )
            .unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Recovery,
            )
            .unwrap();
            let recovery = store
                .state_session(physical(), MemoryClass::Recovery)
                .unwrap();
            let bootstrap_values = store.value_session(physical());
            let scratch = Scratch {
                memory: &memory,
                class: MemoryClass::Recovery,
                pages: RefCell::new(BTreeMap::new()),
            };
            let recovered = crate::ModuleInstanceRecoveryContext {
                root: roots.state_root(Ns::Business),
                state: &recovery,
                values: &bootstrap_values,
                scratch: &scratch,
                memory: &memory,
                class: MemoryClass::Recovery,
                state_io: &mut scan(),
                value_io: &mut logical(),
                scratch_io: &mut scratch_io(),
                max_scalar_bytes: (length + 4096) as u64,
            }
            .qualify_family(&roots, || false)
            .unwrap();
            let proof = crate::readmit_module_instance_family_ordinary_v1(
                &recovered,
                &roots,
                &memory,
                || false,
            )
            .unwrap();
            drop((recovered, scratch, bootstrap_values, recovery));
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let forbidden = roots
                .state_root(Ns::Business)
                .get(
                    ModuleInstanceKeyV1::new("map-key", F::Version)
                        .unwrap()
                        .as_bytes(),
                    &state,
                )
                .unwrap()
                .unwrap();
            let values = store.value_session(physical());
            let source = Forbid {
                source: &values,
                forbidden,
            };
            let parent = ExecutionParent {
                roots: roots.clone(),
                commit: [4; 32],
                authority: oasis7_formal_protocol::incremental_execution::AuthorityBinding {
                    token: [5; 32],
                    epoch: 7,
                },
            };
            let live = parent.fence().unwrap();
            let mut tx =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits())
                    .unwrap();
            let mut writer = store.value_session(physical());
            let mut work = logical();
            let initial = work.clone();
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
                        sink: &mut writer,
                        memory: &memory,
                        io: &mut work,
                        cancelled: || false
                    }
                )
                .unwrap()
            );
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 0).unwrap();
            let copy_charge = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
            let delta = copy_charge.retain(StateDeltaV1 {
                before: roots.state_root(Ns::Business).clone(),
                after: sealed.record.after.state_root(Ns::Business).clone(),
                writes: sealed
                    .record
                    .writes
                    .iter()
                    .map(|w| w.mutation.clone())
                    .collect(),
                new_nodes: sealed.new_nodes.clone(),
            });
            writer.put_state_pages(&delta, &memory).unwrap();
            drop(delta);
            let scratch = Scratch {
                memory: &memory,
                class: MemoryClass::Ordinary,
                pages: RefCell::new(BTreeMap::new()),
            };
            let verifier = TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 1 << 20,
            };
            let after = crate::accept_module_instance_transition_v1(
                &proof,
                &sealed.record,
                &verifier,
                &mut crate::ModuleInstanceRecoveryContext {
                    root: sealed.record.after.state_root(Ns::Business),
                    state: &state,
                    values: &source,
                    scratch: &scratch,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    state_io: &mut scan(),
                    value_io: &mut work,
                    scratch_io: &mut scratch_io(),
                    max_scalar_bytes: 165,
                },
                &mut 1000,
                || false,
            )
            .unwrap();
            after.validate_binding(&sealed.record.after).unwrap();
            let measured = (
                initial.nodes_remaining - work.nodes_remaining,
                initial.read_bytes_remaining - work.read_bytes_remaining,
                initial.write_bytes_remaining - work.write_bytes_remaining,
            );
            assert_eq!(*baseline.get_or_insert(measured), measured);
            eprintln!(
                "instance delta {compression:?} text={length} logical={measured:?} state={:?} values={:?} writes={:?}",
                state.remaining_io().unwrap(),
                values.remaining_io().unwrap(),
                writer.remaining_io().unwrap()
            );
        }
    }
}
