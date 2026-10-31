use super::*;
use crate::{
    incremental_field_member::{
        MemberPayloadRefV1, MemberRecordLimitsV1, MemberRecordWriteContext, write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_projection::{FieldDestination, WORLD_STATE_FIELDS},
    incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateDeltaV1, StateNamespaceV1, StateRootV1, StateScanBudget,
        StateWriteV1,
    },
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

struct StatePage(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for StatePage {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

struct StatePages {
    memory: MemoryBudget,
    pages: RefCell<BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>>,
}
impl StatePages {
    fn new(memory: MemoryBudget) -> Self {
        Self {
            memory,
            pages: RefCell::new(BTreeMap::new()),
        }
    }

    fn add_delta(&self, delta: StateDeltaV1) {
        for node in delta.new_nodes {
            let charge = self
                .memory
                .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                .unwrap();
            self.pages
                .borrow_mut()
                .insert(node.digest, Arc::new(charge.retain(node.bytes)));
        }
    }
}
impl MerkleNodeReader for StatePages {
    type Node = StatePage;

    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }

    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }

    fn read_node(&self, id: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        let page = self.pages.borrow().get(id).cloned();
        if page.as_ref().is_some_and(|page| page.len() > max_bytes) {
            return Err("owner test state page bound".into());
        }
        Ok(page.map(StatePage))
    }
}

#[derive(Default)]
struct ValuePages {
    pages: RefCell<BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>>,
}
impl ValueNodeWriter for ValuePages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if blake3::hash(bytes.as_slice()).as_bytes() != &id {
            return Err(ValueTreeError::Invalid("owner test value hash"));
        }
        self.pages.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for ValuePages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let page = self
            .pages
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("owner test missing value page"))?;
        if class != MemoryClass::Ordinary || !page.uses_budget(memory) || page.len() > max_bytes {
            return Err(ValueTreeError::Invalid("owner test value pool/class/bound"));
        }
        Ok(page)
    }
}

struct Fixture {
    memory: MemoryBudget,
    state: StatePages,
    values: ValuePages,
    roots: [StateRootV1; 6],
}

fn fixture(include_u64_entries: bool) -> Fixture {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let state = StatePages::new(memory.clone());
    let mut values = ValuePages::default();
    let mut value_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    };
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut writes = BTreeMap::<StateNamespaceV1, Vec<StateWriteV1>>::new();
    for field in WORLD_STATE_FIELDS.iter().copied() {
        let FieldDestination::State(namespace) = field.destination else {
            continue;
        };
        let metadata = write_field_metadata_v1(
            field,
            &mut PresenceRecordWriteContext {
                sink: &mut values,
                memory: &memory,
                io: &mut value_io,
                key_work: &mut key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        writes.entry(namespace).or_default().push(StateWriteV1 {
            key: metadata.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(metadata.value_root),
        });

        if include_u64_entries {
            let Some(scalar) = u64_field(field) else {
                continue;
            };
            let entry = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &(),
                MemberPayloadRefV1::U64(scalar.value(&crate::state::WorldState::default())),
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut value_io,
                    key_work: &mut key_work,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: U64_SEMANTIC_BYTES,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            writes.entry(namespace).or_default().push(StateWriteV1 {
                key: entry.field_key.as_bytes().to_vec(),
                before: None,
                after: Some(entry.value_root),
            });
        }
    }

    let namespaces = [
        StateNamespaceV1::Business,
        StateNamespaceV1::Allocator,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Effects,
        StateNamespaceV1::Artifacts,
    ];
    let mut roots = Vec::with_capacity(namespaces.len());
    for namespace in namespaces {
        let mut state_writes = writes.remove(&namespace).unwrap_or_default();
        state_writes.sort_by(|left, right| left.key.cmp(&right.key));
        let delta = StateDeltaV1::build(
            StateRootV1::empty(namespace).unwrap(),
            state_writes,
            &state,
            16 << 20,
        )
        .unwrap();
        roots.push(delta.after.clone());
        state.add_delta(delta);
    }
    assert!(writes.is_empty());
    Fixture {
        memory,
        state,
        values,
        roots: roots.try_into().unwrap(),
    }
}

fn verify(
    fixture: &Fixture,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<UnsupportedWorldStateOwnersV1>, WorldStateOwnerError> {
    let mut value_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    };
    let mut values = ValueReadContext {
        source: &fixture.values,
        memory: &fixture.memory,
        class: MemoryClass::Ordinary,
        io: &mut value_io,
    };
    let mut state_io = StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 10_000,
    };
    verify_world_state_owner_coverage(
        &fixture.roots,
        [0x5a; 32],
        &fixture.state,
        &mut values,
        &mut state_io,
        cancelled,
    )
}

#[test]
fn complete_registered_metadata_and_u64_families_are_typed_but_not_complete_world_owners() {
    let fixture = fixture(true);
    let report = verify(&fixture, || false).unwrap();
    assert_eq!(report.body_id(), [0x5a; 32]);
    assert_eq!(report.checked_u64_fields(), 7);
    assert!(report.unsupported_fields().contains(&"resources"));
    assert!(report.runtime_registry_missing());
    assert!(report.cognition_registry_missing());
    assert!(!report.unsupported_fields().is_empty());
}

#[test]
fn missing_required_u64_entry_fails_closed_even_when_all_metadata_is_present() {
    let fixture = fixture(false);
    assert!(matches!(
        verify(&fixture, || false),
        Err(WorldStateOwnerError::Invalid(
            "required scalar-u64 metadata/entry missing or duplicate"
        ))
    ));
}

#[test]
fn full_owner_scan_observes_cancellation() {
    let fixture = fixture(true);
    assert!(matches!(
        verify(&fixture, || true),
        Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled))
    ));
}
