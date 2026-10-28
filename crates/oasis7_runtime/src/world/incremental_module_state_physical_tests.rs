use super::super::physical::{Directory, install_fixture_pages, logical_io, physical_io};
use super::*;
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression,
    objects::{IncrementalObjectStore, ObjectIo},
};
use std::{
    io::{self, Write},
    sync::atomic::AtomicBool,
};

fn step(
    world: &World,
    event: &WorldEvent,
    parent: ExecutionParent,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> ReservedValue<oasis7_formal_protocol::incremental_execution::PreparedTransition> {
    let state = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_io());
    let mut sink = store.value_session(physical_io());
    let cancel = AtomicBool::new(false);
    let objects =
        IncrementalObjectStore::open(store, MemoryClass::Ordinary, &mut physical_io(), &cancel)
            .unwrap();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &state, memory, limits()).unwrap();
    let mut quota = logical_io();
    world
        .contribute_iws_module_state_event(
            event,
            &mut txn,
            &mut ModuleStateEventContext {
                values: &values,
                mutation: BodyMutationContext {
                    sink: &mut sink,
                    memory,
                    io: &mut quota,
                    cancelled: || false,
                },
                max_event_bytes: 8 << 20,
                persist_original: |mut reader: &mut dyn Read, length, id| {
                    let stored = objects
                        .write_object(
                            &mut reader,
                            length,
                            Some(id),
                            &mut ObjectIo {
                                physical: &mut physical_io(),
                                logical: &mut logical_io(),
                                class: MemoryClass::Ordinary,
                                cancelled: &cancel,
                            },
                        )
                        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                    if stored != id {
                        return Err(BodyMutationError::Invalid("module original identity"));
                    }
                    Ok(())
                },
            },
        )
        .unwrap();
    let guard = txn.guard();
    txn.seal(&guard, &live, 0).unwrap()
}

// Independent streaming oracle: small maintained-ABI skeleton supplies the
// metadata, and a 256-entry maintained-ABI scalar palette supplies each u8.
// It never calls the production original reader or builds a large CBOR model.
struct OriginalOracle<'a> {
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    palette: [Vec<u8>; 256],
    payload_bytes: usize,
    state: &'a [u8],
    position: usize,
    state_position: usize,
    scalar_position: usize,
    hash: blake3::Hasher,
}
impl<'a> OriginalOracle<'a> {
    fn new(event: &'a WorldEvent) -> Self {
        let WorldEventBody::ModuleStateUpdated(update) = &event.body else {
            panic!()
        };
        assert_eq!(update.state.len(), 2 << 20);
        let mut skeleton = module_event(event.id, vec![255]);
        skeleton.time = event.time;
        skeleton.caused_by = event.caused_by.clone();
        let encoded = oasis7_wasm_abi::encode_canonical_cbor(&skeleton).unwrap();
        let markers = encoded
            .windows(3)
            .enumerate()
            .filter(|(_, w)| *w == [0x81, 0x18, 0xff])
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        assert_eq!(markers.len(), 1);
        let index = markers[0];
        let mut prefix = encoded[..index].to_vec();
        prefix.extend_from_slice(&[0x9a, 0, 0x20, 0, 0]);
        let palette: [Vec<u8>; 256] =
            std::array::from_fn(|i| oasis7_wasm_abi::encode_canonical_cbor(&(i as u8)).unwrap());
        let payload_bytes = update
            .state
            .iter()
            .map(|byte| palette[*byte as usize].len())
            .sum();
        Self {
            prefix,
            suffix: encoded[index + 3..].to_vec(),
            palette,
            payload_bytes,
            state: &update.state,
            position: 0,
            state_position: 0,
            scalar_position: 0,
            hash: blake3::Hasher::new(),
        }
    }
    fn finish(&self) {
        assert_eq!(self.state_position, self.state.len());
        assert_eq!(self.scalar_position, 0);

        assert_eq!(
            self.position,
            self.prefix.len() + self.payload_bytes + self.suffix.len()
        );
    }
}
impl Write for OriginalOracle<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for actual in bytes {
            let expected = if self.position < self.prefix.len() {
                self.prefix[self.position]
            } else if self.state_position < self.state.len() {
                let scalar = &self.palette[self.state[self.state_position] as usize];
                let byte = scalar[self.scalar_position];
                self.scalar_position += 1;
                if self.scalar_position == scalar.len() {
                    self.scalar_position = 0;
                    self.state_position += 1;
                }
                byte
            } else {
                self.suffix[self.position - self.prefix.len() - self.payload_bytes]
            };
            assert_eq!(*actual, expected, "original offset {}", self.position);
            self.position += 1;
        }
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct RawStateOracle<'a> {
    state: &'a [u8],
    position: usize,
}
impl Write for RawStateOracle<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        assert_eq!(
            bytes,
            &self.state[self.position..self.position + bytes.len()]
        );
        self.position += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn funded_large_module_event_reopens_original_state_and_continues_without_cache() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(12 << 20, 3 << 20).unwrap();
    let (mut world, parent, pages, values) = setup(&memory);
    let charge = memory
        .reserve((2 << 20) + 4096, MemoryClass::Ordinary)
        .unwrap();
    let event = charge.retain(module_event(
        1,
        (0..2usize << 20)
            .map(|i| (i as u8) ^ ((i >> 16) as u8))
            .collect(),
    ));
    let path = directory.0.join("module-state");
    let store = IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
    install_fixture_pages(&store, &memory, &pages, &values);
    let cancelled = AtomicBool::new(false);
    IncrementalObjectStore::create(&store, &mut physical_io(), &cancelled).unwrap();
    let first = step(&world, &event, parent.clone(), &store, &memory);
    let mut state = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    state.persist_transition_pages(&first).unwrap();
    let next = ExecutionParent {
        commit: [8; 32],
        roots: first.record.after.clone(),
        authority: parent.authority,
    };
    let expected = step(
        &world,
        &module_event(2, vec![0, 24, 255]),
        next.clone(),
        &store,
        &memory,
    );
    drop(state);
    drop(store);
    world.state.module_states.clear();
    let recovery = MemoryBudget::new(12 << 20, 3 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        recovery.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let state = store
        .state_session(physical_io(), MemoryClass::Recovery)
        .unwrap();
    TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &state,
        memory: &recovery,
        class: MemoryClass::Recovery,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&first.record)
    .unwrap();
    let source = store.value_session(physical_io());
    let mut logical = logical_io();
    let mut reader = ValueReadContext {
        source: &source,
        memory: &recovery,
        class: MemoryClass::Recovery,
        io: &mut logical,
    };
    let key = ModuleStateKeyV1::new("module#stable-2").unwrap();
    let id = first.record.after.state[0]
        .get(key.as_bytes(), &state)
        .unwrap()
        .unwrap();
    let value = reader.load_root(&id).unwrap();
    reader.verify_closure(&id, 8 << 20, || false).unwrap();
    let WorldEventBody::ModuleStateUpdated(update) = &event.body else {
        panic!()
    };
    let mut output = RawStateOracle {
        state: &update.state,
        position: 0,
    };
    reader
        .read_range(&value, 0, value.0.logical_len, &mut output, || false)
        .unwrap();
    assert_eq!(output.position, update.state.len());
    let objects = IncrementalObjectStore::open(
        &store,
        MemoryClass::Recovery,
        &mut physical_io(),
        &cancelled,
    )
    .unwrap();
    let _oracle_charge = recovery.reserve(32 << 10, MemoryClass::Recovery).unwrap();
    let mut oracle = OriginalOracle::new(&event);
    objects
        .read_object_into(
            &first.record.facts[0].id,
            8 << 20,
            &mut oracle,
            &mut ObjectIo {
                physical: &mut physical_io(),
                logical: &mut logical_io(),
                class: MemoryClass::Recovery,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    oracle.finish();
    assert_eq!(*oracle.hash.finalize().as_bytes(), first.record.facts[0].id);
    let resumed = step(
        &world,
        &module_event(2, vec![0, 24, 255]),
        next,
        &store,
        &recovery,
    );
    assert_eq!(resumed.record, expected.record);
    assert_eq!(resumed.new_nodes, expected.new_nodes);
    assert!(world.state.module_states.is_empty());
}
