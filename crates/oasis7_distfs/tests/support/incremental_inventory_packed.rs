#![cfg(all(unix, not(target_arch = "wasm32")))]
//! The initial GEN read cut shared by the real Agent/Runtime physical tests.
//! This deliberately stops before any event or Runtime commit. The producer
//! and complete six-root qualifier are real; no Runtime dependency is needed.
use oasis7_agent_api::world_action::{BodyKernelView, GeoPos};
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StateNodeWriteV1, StatePageWriter, StateRootV1,
        StateScanBudget, StateWriteV1,
    },
    incremental_value::{ValueIoBudget, ValueReadContext, ValueStreamWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    AgentCell, AgentState, DomainEvent, MaterialLedgerId, WorldState,
    incremental_agent_genesis::{AgentGenesisContext, write_agent_cell_fields},
    incremental_agent_mutation::BodyMutationContext,
    incremental_event_fields::{EVENT_SEQUENCE_KEY_V1, EventSequenceV1},
    incremental_field_member::{
        MemberPayloadRefV1, MemberRecordLimitsV1, MemberRecordWriteContext, write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory::{
        InventoryFamilyWorkV1, InventoryMaterialScopeWorkV1,
        build_parent_inventory_material_scope_v1, qualify_inventory_family_v1,
    },
    incremental_inventory_genesis::{
        InventoryGenesisContext, InventoryWriteSink, prepare_inventory_genesis_v1,
    },
    incremental_projection::{FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS},
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use std::{collections::BTreeMap, io::Write, path::PathBuf};

fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 128 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 128 << 20,
    }
}
fn state_physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 128 << 10,
        ..physical()
    }
}
fn discovery_physical(roots: &RootSetV1) -> Result<PageIoBudget, PageStoreError> {
    // GEN/cold qualification scans all six roots; parent material scope scans
    // Business again. At depth 256, n leaves occupy <=1+256*n nonempty pages.
    // A <=1200-byte State page has one frame. With <=256 packed entries and
    // the four-row index tail, recipe/frame each cost <=10 physical reads;
    // the format read makes <=21 per page. Keep the old stage/seal 128Ki
    // separate and leave physical bytes, writes, Values and memory unchanged.
    let limited = || PageStoreError::ResourceLimited("inventory fixture discovery read bound");
    let pages = roots
        .state
        .iter()
        .chain(std::iter::once(roots.state_root(Ns::Business)))
        .try_fold(0u64, |total, root| {
            let pages = if root.leaf_count == 0 {
                0
            } else {
                root.leaf_count
                    .checked_mul(256)
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(limited)?
            };
            total.checked_add(pages).ok_or_else(limited)
        })?;
    let mut allowance = state_physical();
    allowance.reads_remaining = pages
        .checked_mul(21)
        .and_then(|reads| allowance.reads_remaining.checked_add(reads))
        .ok_or_else(limited)?;
    Ok(allowance)
}
fn production_io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        leaves_remaining: 1_000_000,
    }
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oasis7-inventory-packed-{}-{nonce}",
            std::process::id()
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
struct Rows(Vec<ReservedValue<StateWriteV1>>);
impl InventoryWriteSink for Rows {
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        self.0.push(write);
        Ok(())
    }
}
fn source() -> WorldState {
    let mut source = WorldState {
        time: 11,
        ..WorldState::default()
    };
    let mut agent = AgentState::new("alice", GeoPos::new(0, 0, 0));
    agent.body_view = BodyKernelView {
        mass_kg: 10,
        radius_cm: 10,
        thrust_limit: 10,
        cross_section_cm2: 10,
    };
    source
        .agents
        .insert("alice".into(), AgentCell::new(agent, 3));
    // This is the inventory-affecting normalization in World::new_with_state,
    // called before the original fixture's borrowed GEN producers.
    source.migrate_compat_material_ledgers();
    source
}
fn delta_heap(delta: &StateDeltaV1) -> usize {
    let StateDeltaV1 {
        before,
        after,
        writes,
        new_nodes,
    } = delta;
    let mut heap = std::mem::size_of::<StateDeltaV1>();
    let mut add = |n: usize| heap = heap.checked_add(n).unwrap();
    for root in [before, after] {
        let StateRootV1 {
            namespace: _,
            node_root,
            leaf_count: _,
        } = root;
        add(node_root.capacity());
    }
    add(writes.capacity() * std::mem::size_of::<StateWriteV1>());
    for write in writes {
        add(write.key.capacity());
    }
    add(new_nodes.capacity() * std::mem::size_of::<StateNodeWriteV1>());
    for node in new_nodes {
        add(node.digest.capacity());
        add(node.bytes.capacity());
    }
    heap
}
fn install(source: &WorldState, store: &IncrementalPageStore, memory: &MemoryBudget) -> RootSetV1 {
    let mut fields = BTreeMap::<u8, Vec<StateWriteV1>>::new();
    let mut sink = store.value_session(physical());
    write_agent_cell_fields(
        "alice",
        &source.agents["alice"],
        &mut AgentGenesisContext {
            mutation: BodyMutationContext {
                sink: &mut sink,
                memory,
                io: &mut production_io(),
                cancelled: || false,
            },
            emit: |namespace, key: &[u8], value| {
                fields
                    .entry(namespace as u8)
                    .or_default()
                    .push(StateWriteV1 {
                        key: key.to_vec(),
                        before: None,
                        after: Some(value),
                    });
                Ok(())
            },
            mailbox_original: |_: u64, _: &DomainEvent| panic!("GEN mailbox is empty"),
            max_scalar_bytes: 4096,
        },
    )
    .unwrap();
    // The original Runtime clock wrapper uses these two registered producers.
    let field = WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|field| {
            field.name == "time"
                && field.owner == FieldOwner::WorldState
                && field.shape == FieldShape::Value
                && field.destination == FieldDestination::State(Ns::Allocator)
        })
        .unwrap();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64,
    };
    let mut io = production_io();
    let metadata = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut sink,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: 64,
                record_bytes: 512,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let time = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::U64(source.time),
        &mut MemberRecordWriteContext {
            sink: &mut sink,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 64,
                payload_bytes: 8,
                record_bytes: 512,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    for (key, value) in [
        (metadata.field_key.as_bytes(), metadata.value_root),
        (time.field_key.as_bytes(), time.value_root),
    ] {
        fields
            .entry(Ns::Allocator as u8)
            .or_default()
            .push(StateWriteV1 {
                key: key.to_vec(),
                before: None,
                after: Some(value),
            });
    }
    let bytes = EventSequenceV1 { next: 1, era: 0 }.canonical_bytes();
    let mut sequence_io = production_io();
    let mut writer = ValueStreamWriter::new(
        &mut sink,
        memory,
        &mut sequence_io,
        bytes.len() as u64,
        || false,
    )
    .unwrap();
    writer.write_all(&bytes).unwrap();
    let sequence = writer.finish().unwrap().digest().unwrap();
    fields
        .entry(Ns::Allocator as u8)
        .or_default()
        .push(StateWriteV1 {
            key: EVENT_SEQUENCE_KEY_V1.to_vec(),
            before: None,
            after: Some(sequence),
        });
    let mut inventory = Rows(Vec::new());
    prepare_inventory_genesis_v1(
        source,
        &mut InventoryGenesisContext {
            values: &mut sink,
            writes: &mut inventory,
            memory,
            io: &mut production_io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            records_remaining: 10_000,
            cancelled: || false,
        },
    )
    .unwrap();
    fields
        .entry(Ns::Business as u8)
        .or_default()
        .extend(inventory.0.iter().map(|row| (**row).clone()));
    let mut roots = RootSetV1::empty("agent-event-physical".into(), [2; 32]).unwrap();
    let pages = store
        .state_session(state_physical(), MemoryClass::Ordinary)
        .unwrap();
    for (namespace, mut writes) in fields {
        writes.sort_unstable_by(|a, b| a.key.cmp(&b.key));
        assert!(writes.windows(2).all(|pair| pair[0].key != pair[1].key));
        let index = namespace as usize - 1;
        let mut charge = memory
            .reserve(
                writes.len() * (512 << 10) + (128 << 10),
                MemoryClass::Ordinary,
            )
            .unwrap();
        let delta =
            StateDeltaV1::build(roots.state[index].clone(), writes, &pages, 4 << 20).unwrap();
        let retained = charge.split(delta_heap(&delta)).unwrap();
        drop(charge);
        let delta = retained.retain(delta);
        eprintln!(
            "GEN namespace={namespace} writes={} pages={} root={}",
            delta.writes.len(),
            delta.new_nodes.len(),
            delta.after.node_root
        );
        sink.put_state_pages_cancelled(&delta, memory, &mut || false)
            .unwrap();
        roots.state[index] = delta.after.clone();
    }
    assert_eq!(roots.state.len(), 6, "retain every original namespace");
    roots
}
fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::ResourceLimited(label) => ValueTreeError::ResourceLimited(label),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(error) => ValueTreeError::Io(error),
        PageStoreError::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "material physical page",
        )),
        PageStoreError::Corrupt(label) => ValueTreeError::Invalid(label),
        PageStoreError::UnsupportedPlatform => {
            ValueTreeError::Invalid("material physical platform")
        }
    }
}
enum StateAllowance {
    StageOnly,
    DiscoveryAndStage,
}
fn run(compression: PageCompression, allowance: StateAllowance) {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&directory.0.join("pages"), memory.clone(), compression)
            .unwrap();
    let source = source();
    let roots = install(&source, &store, &memory);
    let expected_success = matches!(allowance, StateAllowance::DiscoveryAndStage);
    let state_account = match allowance {
        StateAllowance::StageOnly => state_physical(),
        StateAllowance::DiscoveryAndStage => discovery_physical(&roots).unwrap(),
    };
    let initial_reads = state_account.reads_remaining;
    let state = store
        .state_session(state_account, MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    // These are exactly MaterialFixtureV1::build's logical allowances; the
    // State discovery scope changes only operation count, not other limits.
    let mut logical = ValueIoBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 1 << 30,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 30,
    };
    let mut family = InventoryFamilyWorkV1 {
        records_remaining: 1_000_000,
        index_steps_remaining: 1_000_000,
        max_semantic_bytes: 4096,
    };
    let mut read = WorldFieldReadContextV1 {
        root: roots.state_root(Ns::Business),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut logical,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut qualification_scan = scan();
    let result =
        qualify_inventory_family_v1(&roots, &mut read, &mut qualification_scan, &mut family);
    eprintln!(
        "GEN {compression:?} height={} roots={} qualification={:?}; State={:?}; Values={:?}; scan nodes={} bytes={} leaves={}; logical nodes={} read={} write={}; keys={} records={} index={}",
        roots.height,
        hex::encode(roots.digest().unwrap()),
        result.as_ref().map(|_| ()),
        state.remaining_io().unwrap(),
        values.remaining_io().unwrap(),
        qualification_scan.nodes_remaining,
        qualification_scan.read_bytes_remaining,
        qualification_scan.leaves_remaining,
        read.values.io.nodes_remaining,
        read.values.io.read_bytes_remaining,
        read.values.io.write_bytes_remaining,
        read.key_work.bytes_remaining,
        family.records_remaining,
        family.index_steps_remaining
    );
    if !expected_success {
        assert!(
            matches!(
                &result,
                Err(ValueTreeError::ResourceLimited("physical read allowance"))
            ),
            "GEN {compression:?} must preserve the first typed physical limit: {:?}",
            result.as_ref().map(|_| ())
        );
        let remaining = state.remaining_io().unwrap();
        assert_eq!(remaining.reads_remaining, 0);
        assert!(remaining.read_bytes_remaining > 0);
        return;
    }
    let qualification = result.unwrap_or_else(|error| {
        panic!("GEN {compression:?} complete inventory qualifier must succeed: {error:?}")
    });
    let world = source.material_ledgers.get(&MaterialLedgerId::World);
    let mut scope = InventoryMaterialScopeWorkV1 {
        max_entries_per_map: source
            .materials
            .len()
            .max(world.map_or(0, |map| map.len()))
            .max(1),
        max_material_bytes: source
            .materials
            .keys()
            .chain(world.into_iter().flat_map(|map| map.keys()))
            .map(String::len)
            .max()
            .unwrap_or(1)
            .max(1),
        pending_entries_remaining: 1_000_000,
        comparison_bytes_remaining: 64 << 20,
    };
    let result = build_parent_inventory_material_scope_v1(
        &qualification,
        &roots,
        &mut read,
        &mut scan(),
        &mut family,
        &mut scope,
    );
    eprintln!(
        "GEN {compression:?} material scope={:?}; State={:?}; Values={:?}",
        result.as_ref().map(|_| ()),
        state.remaining_io().unwrap(),
        values.remaining_io().unwrap()
    );
    result.unwrap_or_else(|error| {
        panic!("GEN {compression:?} material scope must succeed: {error:?}")
    });
    assert!(
        state.remaining_io().unwrap().reads_remaining >= state_physical().reads_remaining,
        "GEN/cold discovery must leave the original 128Ki stage/seal allowance"
    );
    eprintln!(
        "GEN {compression:?} measured discovery State reads={} bytes={} Values reads={} bytes={}",
        initial_reads - state.remaining_io().unwrap().reads_remaining,
        (128 << 20) - state.remaining_io().unwrap().read_bytes_remaining,
        100_000 - values.remaining_io().unwrap().reads_remaining,
        (128 << 20) - values.remaining_io().unwrap().read_bytes_remaining
    );
}
#[test]
fn packed_raw_gen_inventory_rejects_stage_only_physical_account() {
    run(PageCompression::Raw, StateAllowance::StageOnly);
}
#[test]
fn packed_zstd_gen_inventory_rejects_stage_only_physical_account() {
    run(PageCompression::Zstd, StateAllowance::StageOnly);
}
#[test]
fn packed_raw_gen_inventory_qualifies_with_discovery_and_stage_accounts() {
    run(PageCompression::Raw, StateAllowance::DiscoveryAndStage);
}
#[test]
fn packed_zstd_gen_inventory_qualifies_with_discovery_and_stage_accounts() {
    run(PageCompression::Zstd, StateAllowance::DiscoveryAndStage);
}
