use super::*;
use oasis7_distfs::incremental_page_store::{
    PageCompression, PageIoBudget,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionParent, ExecutionTransaction, TransactionLimits, TransitionVerifier,
    },
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateScanBudget, StateWriteV1,
    },
    incremental_value::{VALUE_CHUNK_BYTES, ValueIoBudget, ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::incremental_module_fields::ModuleStateKeyV1;
use oasis7_runtime::{
    BodyMutationContext, IwsStepContext, ModuleStateContentBudgetV1, ModuleStateRecoveryContextV1,
    World, WorldEvent, WorldEventBody, read_iws_module_state,
};
use oasis7_wasm_abi::ModuleStateUpdate;
use std::{
    io::{self, Write},
    sync::{Arc, atomic::AtomicBool},
};

const EVENT_OPERATION: [u8; 32] = [0x72; 32];
const MODULE_KEY: &str = "instance#stable-2\0雪";
const OTHER_MODULE_KEY: &str = "instance#untouched";

fn page_io() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 512 << 20,
    }
}

fn value_io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 512 << 20,
    }
}

fn state_byte(index: usize) -> u8 {
    (index as u8).wrapping_mul(37) ^ ((index >> 16) as u8).wrapping_add(19)
}

fn module_event(id: u64, time: u64, state: Vec<u8>) -> WorldEvent {
    WorldEvent {
        id,
        time,
        caused_by: Some(oasis7_runtime::CausedBy::Effect {
            intent_id: "effect:module-original".into(),
        }),
        body: WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
            module_id: MODULE_KEY.to_owned(),
            trace_id: "trace:module-state-update".into(),
            state,
        }),
    }
}

fn seed_module_state(
    world: &World,
    parent: &mut ExecutionParent,
    store: &oasis7_distfs::incremental_page_store::IncrementalPageStore,
    memory: &MemoryBudget,
) -> [u8; 32] {
    let mut writes = Vec::new();
    let _writes_charge = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let mut values = store.value_session(page_io());
    let mut module_budget = ModuleStateContentBudgetV1 {
        states_remaining: 2,
        logical_bytes_remaining: 20,
        max_state_bytes: 1024,
    };
    world
        .write_iws_module_state_genesis(
            &mut BodyMutationContext {
                sink: &mut values,
                memory,
                io: &mut value_io(),
                cancelled: || false,
            },
            &mut module_budget,
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
    assert_eq!(module_budget.states_remaining, 0);
    assert_eq!(module_budget.logical_bytes_remaining, 0);

    let state = store
        .state_session(page_io(), MemoryClass::Ordinary)
        .unwrap();
    writes.sort_unstable_by(|a, b| a.key.cmp(&b.key));
    assert!(writes.windows(2).all(|pair| pair[0].key != pair[1].key));
    let delta = StateDeltaV1::build(
        parent.roots.state[Ns::Business as usize - 1].clone(),
        writes,
        &state,
        4 << 20,
    )
    .unwrap();
    let delta = memory
        .reserve(4 << 20, MemoryClass::Ordinary)
        .unwrap()
        .retain(delta);
    store
        .value_session(page_io())
        .put_state_pages_cancelled(&delta, memory, &mut || false)
        .unwrap();
    parent.roots.state[Ns::Business as usize - 1] = delta.after.clone();
    let current = ModuleStateKeyV1::new(MODULE_KEY).unwrap();
    parent.roots.state[Ns::Business as usize - 1]
        .get(current.as_bytes(), &state)
        .unwrap()
        .unwrap()
}

/// Rejects any attempt to read an unchanged Business value during the step.
struct ForbidValueRoots<'a, V> {
    inner: &'a V,
    forbidden: [[u8; 32]; 2],
}
impl<V: ValueNodeReader> ValueNodeReader for ForbidValueRoots<'_, V> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        assert!(
            !self.forbidden.contains(id),
            "step read an unchanged value root"
        );
        self.inner.read_node(id, max_bytes, memory, class)
    }
}

struct OriginalOracle {
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    palette: [Vec<u8>; 256],
    payload_bytes: usize,
    payload_len: usize,
    position: usize,
    state_position: usize,
    scalar_position: usize,
    hash: blake3::Hasher,
}
impl OriginalOracle {
    fn new(metadata: &WorldEvent, payload_len: usize) -> Self {
        let skeleton = module_event(metadata.id, metadata.time, vec![u8::MAX]);
        let encoded = oasis7_wasm_abi::encode_canonical_cbor(&skeleton).unwrap();
        let marker = [0x81, 0x18, 0xff];
        let positions = encoded
            .windows(marker.len())
            .enumerate()
            .filter(|(_, bytes)| *bytes == marker)
            .map(|(position, _)| position)
            .collect::<Vec<_>>();
        assert_eq!(positions.len(), 1);
        let index = positions[0];
        let payload_len_u32 = u32::try_from(payload_len).unwrap();
        // CBOR major type 4 with a 32-bit array length, matching Vec<u8>'s
        // maintained ABI representation (not a byte string).
        let mut prefix = encoded[..index].to_vec();
        prefix.push(0x9a);
        prefix.extend(payload_len_u32.to_be_bytes());
        let palette: [Vec<u8>; 256] = std::array::from_fn(|byte| {
            oasis7_wasm_abi::encode_canonical_cbor(&(byte as u8)).unwrap()
        });
        let payload_bytes = (0..payload_len)
            .map(|index| palette[state_byte(index) as usize].len())
            .sum();
        Self {
            prefix,
            suffix: encoded[index + marker.len()..].to_vec(),
            palette,
            payload_bytes,
            payload_len,
            position: 0,
            state_position: 0,
            scalar_position: 0,
            hash: blake3::Hasher::new(),
        }
    }

    fn encoded_len(&self) -> usize {
        self.prefix.len() + self.payload_bytes + self.suffix.len()
    }

    fn finish(&self) {
        assert_eq!(self.state_position, self.payload_len);
        assert_eq!(self.scalar_position, 0);
        assert_eq!(self.position, self.encoded_len());
    }
}
impl Write for OriginalOracle {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for actual in bytes {
            let expected = if self.position < self.prefix.len() {
                self.prefix[self.position]
            } else if self.state_position < self.payload_len {
                let scalar = &self.palette[state_byte(self.state_position) as usize];
                let expected = scalar[self.scalar_position];
                self.scalar_position += 1;
                if self.scalar_position == scalar.len() {
                    self.scalar_position = 0;
                    self.state_position += 1;
                }
                expected
            } else {
                self.suffix[self.position - self.prefix.len() - self.payload_bytes]
            };
            assert_eq!(*actual, expected, "event original offset {}", self.position);
            self.position += 1;
        }
        self.hash.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct StateOracle {
    position: usize,
}
impl Write for StateOracle {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for (offset, byte) in bytes.iter().enumerate() {
            assert_eq!(*byte, state_byte(self.position + offset));
        }
        self.position += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn module_state_event_genesis_step_original_and_recovery_cross_value_chunks() {
    exercise_module_state(2 * VALUE_CHUNK_BYTES + 17);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_state_event_genesis_step_original_and_recovery_are_raw_zstd_stable() {
    exercise_module_state(32 << 20);
}

fn exercise_module_state(state_bytes: usize) {
    // OriginalOracle intentionally uses the CBOR 32-bit array-length encoding.
    assert!(state_bytes > u16::MAX as usize);
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = support::Directory::new();
        let path = directory.0.join("module-state");
        let (after_parent, fact_id, original_length) = {
            let memory = MemoryBudget::new(512 << 20, 16 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let (seed_world, parent) = support::fixture(&store, &memory);
            let mut state = seed_world.state().clone();
            state
                .module_states
                .insert(MODULE_KEY.to_owned(), vec![1, 2, 3]);
            state
                .module_states
                .insert(OTHER_MODULE_KEY.to_owned(), vec![0x44; 17]);
            let world = World::new_with_state(state);
            let mut parent = parent;
            let old_module_value = seed_module_state(&world, &mut parent, &store, &memory);

            let state_source = store
                .state_session(page_io(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(page_io());
            let mut sink = store.value_session(page_io());
            let body_id = {
                let key =
                    oasis7_runtime::incremental_agent_body::AgentBodyKeyV1::new("alice").unwrap();
                parent.roots.state[Ns::Business as usize - 1]
                    .get(key.as_bytes(), &state_source)
                    .unwrap()
                    .unwrap()
            };
            let value_reader = ForbidValueRoots {
                inner: &values,
                forbidden: [old_module_value, body_id],
            };
            let input_charge = memory.reserve(state_bytes, MemoryClass::Ordinary).unwrap();
            let mut payload = Vec::with_capacity(state_bytes);
            payload.extend((0..state_bytes).map(state_byte));
            let event = module_event(1, 12, payload);
            let oracle = OriginalOracle::new(&event, state_bytes);
            let qualified = ModuleStateRecoveryContextV1 {
                root: parent.roots.state_root(Ns::Business),
                state: &state_source,
                values: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                state_io: &mut StateScanBudget {
                    nodes_remaining: 1000000,
                    read_bytes_remaining: 64 << 20,
                    leaves_remaining: 10000,
                },
                value_io: &mut value_io(),
                content: &mut ModuleStateContentBudgetV1 {
                    states_remaining: 1000,
                    logical_bytes_remaining: 64 << 20,
                    max_state_bytes: state_bytes as u64,
                },
            }
            .qualify_family(&parent.roots, || false)
            .unwrap();
            let live = parent.fence().unwrap();
            let mut txn = ExecutionTransaction::begin(
                parent.clone(),
                EVENT_OPERATION,
                &state_source,
                &memory,
                TransactionLimits {
                    max_reads: 32,
                    max_writes: 32,
                    max_facts: 8,
                    max_pending_node_bytes: 4 << 20,
                },
            )
            .unwrap();
            let guard = txn.guard();
            let cancelled = AtomicBool::new(false);
            let mut original_physical = page_io();
            let objects = IncrementalObjectStore::open(
                &store,
                MemoryClass::Ordinary,
                &mut original_physical,
                &cancelled,
            )
            .unwrap();
            let mut event_work = support::original_work();
            event_work.visits_remaining = (state_bytes as u64) * 128 + 1_000_000;
            event_work.bytes_remaining = (state_bytes as u64) * 128 + (1 << 20);
            let mut write_io = value_io();
            let mut mutation = BodyMutationContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                cancelled: || cancelled.load(std::sync::atomic::Ordering::SeqCst),
            };
            let mut step = IwsStepContext::begin(
                &mut txn,
                11,
                &guard,
                &live,
                &value_reader,
                &mut mutation,
                &mut event_work,
            )
            .unwrap();
            let mut object_logical = value_io();
            step.dispatch_module_state_event(
                &qualified,
                &event,
                state_bytes * 2 + 4096,
                |original| {
                    support::persist_original(
                        original,
                        &objects,
                        &mut ObjectIo {
                            physical: &mut original_physical,
                            logical: &mut object_logical,
                            class: MemoryClass::Ordinary,
                            cancelled: &cancelled,
                        },
                    )
                },
            )
            .unwrap();
            assert_eq!(
                value_reader.inner as *const _ as usize,
                &values as *const _ as usize
            );
            let guard = txn.guard();
            let prepared = txn.seal(&guard, &live, 0).unwrap();
            assert_eq!(prepared.record.facts.len(), 1);
            assert_eq!(
                prepared.record.facts[0].kind,
                oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1::Event
            );
            let fact_id = prepared.record.facts[0].id;
            let after_parent = ExecutionParent {
                commit: [0x73; 32],
                roots: prepared.record.after.clone(),
                authority: parent.authority,
            };
            let mut persist_state = store
                .state_session(page_io(), MemoryClass::Ordinary)
                .unwrap();
            persist_state.persist_transition_pages(&prepared).unwrap();
            TransitionVerifier {
                parent: &parent,
                operation: EVENT_OPERATION,
                source: &state_source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&prepared.record)
            .unwrap();
            let accepted = oasis7_runtime::accept_module_state_transition_v1(
                &qualified,
                &prepared.record,
                &TransitionVerifier {
                    parent: &parent,
                    operation: EVENT_OPERATION,
                    source: &state_source,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut ModuleStateRecoveryContextV1 {
                    root: prepared.record.after.state_root(Ns::Business),
                    state: &state_source,
                    values: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    state_io: &mut StateScanBudget {
                        nodes_remaining: 1000000,
                        read_bytes_remaining: 64 << 20,
                        leaves_remaining: 10000,
                    },
                    value_io: &mut value_io(),
                    content: &mut ModuleStateContentBudgetV1 {
                        states_remaining: 1,
                        logical_bytes_remaining: 64 << 20,
                        max_state_bytes: state_bytes as u64,
                    },
                },
                || false,
            )
            .unwrap();
            assert_eq!(accepted.states(), 2);
            assert_eq!(accepted.logical_bytes(), state_bytes as u64 + 17);
            drop(prepared);
            drop(event);
            drop(world);
            drop(input_charge);
            drop(objects);
            drop(state_source);
            drop(store);
            drop(memory);
            (after_parent, fact_id, oracle.encoded_len())
        };

        let recovery = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let reopened =
            IncrementalPageStore::open(&path, recovery.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let state = reopened
            .state_session(page_io(), MemoryClass::Recovery)
            .unwrap();
        let values = reopened.value_session(page_io());
        let mut state_scan = StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 512 << 20,
            leaves_remaining: 4096,
        };
        let mut value_budget = value_io();
        let mut content = ModuleStateContentBudgetV1 {
            states_remaining: 8,
            logical_bytes_remaining: 64 << 20,
            max_state_bytes: state_bytes as u64,
        };
        let qualified = ModuleStateRecoveryContextV1 {
            root: &after_parent.roots.state[Ns::Business as usize - 1],
            state: &state,
            values: &values,
            memory: &recovery,
            class: MemoryClass::Recovery,
            state_io: &mut state_scan,
            value_io: &mut value_budget,
            content: &mut content,
        }
        .qualify_family(&after_parent.roots, || false)
        .unwrap();
        assert_eq!(qualified.states(), 2);
        assert_eq!(qualified.logical_bytes(), state_bytes as u64 + 17);
        let qualified = oasis7_runtime::readmit_module_state_family_ordinary_v1(
            &qualified,
            &after_parent.roots,
            &recovery,
            || false,
        )
        .unwrap();

        let state = reopened
            .state_session(page_io(), MemoryClass::Ordinary)
            .unwrap();
        let recovery_parent = ExecutionParent {
            commit: [0x73; 32],
            roots: after_parent.roots.clone(),
            authority: after_parent.authority,
        };
        let mut txn = ExecutionTransaction::begin(
            recovery_parent,
            [0x74; 32],
            &state,
            &recovery,
            TransactionLimits {
                max_reads: 32,
                max_writes: 32,
                max_facts: 8,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut state_output = StateOracle { position: 0 };
        let length = read_iws_module_state(
            &qualified,
            MODULE_KEY,
            &mut txn,
            &mut oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: &values,
                memory: &recovery,
                class: MemoryClass::Ordinary,
                io: &mut value_budget,
            },
            &mut || false,
            state_bytes as u64,
            &mut state_output,
        )
        .unwrap();
        assert_eq!(length, Some(state_bytes as u64));
        assert_eq!(state_output.position, state_bytes);

        let mut original_physical = page_io();
        let cancelled = AtomicBool::new(false);
        let objects = IncrementalObjectStore::open(
            &reopened,
            MemoryClass::Recovery,
            &mut original_physical,
            &cancelled,
        )
        .unwrap();
        let metadata = module_event(1, 12, Vec::new());
        let mut oracle = OriginalOracle::new(&metadata, state_bytes);
        assert_eq!(oracle.encoded_len() as u64, original_length as u64);
        let read = objects
            .read_object_into(
                &fact_id,
                original_length as u64,
                &mut oracle,
                &mut ObjectIo {
                    physical: &mut original_physical,
                    logical: &mut value_io(),
                    class: MemoryClass::Recovery,
                    cancelled: &cancelled,
                },
            )
            .unwrap();
        assert_eq!(read, original_length as u64);
        oracle.finish();
        assert_eq!(*oracle.hash.finalize().as_bytes(), fact_id);
    }
}
