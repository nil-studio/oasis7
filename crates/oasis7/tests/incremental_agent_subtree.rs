#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7::AgentState;
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    AgentCell, BodyMutationContext, BodyMutationError, DomainEvent, World, WorldState,
    incremental_agent_fields::*, incremental_agent_genesis::AgentGenesisContext,
    incremental_agent_subtree::*, incremental_field_member::SemanticKeyBudget,
    incremental_field_read::WorldFieldReadContextV1,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::Directory;
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::ResourceLimited(error) => ValueTreeError::ResourceLimited(error),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(error) => ValueTreeError::Io(error),
        PageStoreError::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "agent physical page",
        )),
        PageStoreError::Corrupt(error) => ValueTreeError::Invalid(error),
        PageStoreError::UnsupportedPlatform => ValueTreeError::Invalid("agent physical platform"),
    }
}
fn source_shape(summary_units: usize, slots: usize) -> World {
    use oasis7_runtime::agent_state::{BodySlotType, CargoEntityEntry, CargoEntityKind};
    let mut cell = AgentCell::new(
        AgentState::new(
            "agent 界",
            oasis7_agent_api::world_action::GeoPos::new(i64::MIN, 0, i64::MAX),
        ),
        u64::MAX,
    );
    cell.state.body.kind = " raw body ".into();
    cell.state.body.height_cm = i64::MIN;
    cell.state.resources.amounts.clear();
    cell.state
        .resources
        .amounts
        .insert(oasis7_agent_api::world_action::ResourceKind::Electricity, 0);
    cell.state.body_state.frame_kind = "".into();
    cell.state.body_state.slot_capacity = u16::MAX;
    cell.state.body_state.expansion_level = u16::MAX;
    let slot = cell.state.body_state.slots[0].clone();
    cell.state.body_state.slots.resize(slots, slot);
    for slot in &mut cell.state.body_state.slots {
        slot.slot_id = " repeat ".into();
        slot.slot_type = BodySlotType::Universal;
        slot.installed_module = Some("".into());
        slot.locked = true;
    }
    for _ in 0..2 {
        cell.state.body_state.cargo_entries.push(CargoEntityEntry {
            entity_id: " repeat ".into(),
            entity_kind: CargoEntityKind::InterfaceModuleItem,
            quantity: i64::MIN,
            size_per_unit: i64::MAX,
        });
    }
    let mut activity = oasis7_runtime::agent_cell::AgentActivityV1::idle(u64::MAX);
    activity.operation_kind = Some("".into());
    activity.operation_id = Some(0);
    cell.activity = Some(activity);
    cell.intent=Some(serde_json::from_value(serde_json::json!({"schema_version":u32::MAX,"agent_id":"agent 界","intent_id":" raw intent ","kind":" raw kind ","summary":"界".repeat(summary_units),"target_id":"","status":" raw status ","source":" raw source ","logical_time":u64::MAX,"event_seq":u64::MAX,"updated_at":0})).unwrap());
    let mut state = WorldState::default();
    state.agents.insert("agent 界".into(), cell);
    World::new_with_state(state)
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget) -> RootSetV1 {
    seed_shape(store, memory, 22_000, 2)
}
fn seed_shape(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    summary_units: usize,
    slots: usize,
) -> RootSetV1 {
    let scalar_limit = (summary_units * 3 + 1024).max(128 << 10);
    let _caller = memory
        .reserve(
            (2 << 20) + summary_units * 3 + slots * 1024,
            MemoryClass::Ordinary,
        )
        .unwrap();
    let world = source_shape(summary_units, slots);
    let mut values = store.value_session(physical());
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::with_capacity(512);
    let count = world
        .write_iws_agent_genesis_fields(
            "agent 界",
            &mut AgentGenesisContext {
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory,
                    io: &mut logical(),
                    cancelled: || false,
                },
                emit: |ns, key: &[u8], id| {
                    assert!(rows.len() < 512);
                    rows.push((ns, key.to_vec(), id));
                    Ok(())
                },
                mailbox_original: |_, _: &DomainEvent| -> Result<[u8; 32], BodyMutationError> {
                    panic!("empty mailbox must not obtain a history witness")
                },
                max_scalar_bytes: scalar_limit as u64,
            },
        )
        .unwrap();
    assert_eq!(count as usize, rows.len());
    let mut roots = RootSetV1::empty("agent-root-only".into(), [0x71; 32]).unwrap();
    for ns in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
    ] {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|row| row.0 == ns)
            .map(|(_, key, id)| StateWriteV1 {
                key: key.clone(),
                before: None,
                after: Some(*id),
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let retained = memory
            .reserve(
                writes.len() * (512 << 10) + (128 << 10),
                MemoryClass::Ordinary,
            )
            .unwrap();
        let delta = retained.retain(
            StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, &state, 8 << 20).unwrap(),
        );
        values
            .put_state_pages_cancelled(&delta, memory, &mut || false)
            .unwrap();
        *roots
            .state
            .iter_mut()
            .find(|root| root.namespace == ns)
            .unwrap() = delta.after.clone();
    }
    roots
}
#[test]
fn actual_borrowed_agent_genesis_cold_root_structure_preserves_original_cells() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("agent-subtree-pages");
        let roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory)
        };
        // No source World, source store, producer pool, or original DTO survives.
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let state = store.state_session(physical(), class).unwrap();
            let values = store.value_session(physical());
            let mut value_io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut ctx = WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut value_io,
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 128 << 10,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let proof = qualify_agent_subtree_structure_v1(
                &roots,
                &mut ctx,
                &mut StateScanBudget {
                    nodes_remaining: 1_000_000,
                    read_bytes_remaining: 256 << 20,
                    leaves_remaining: 1000,
                },
                &mut AgentSubtreeScanWorkV1 {
                    max_agents: 1,
                    cells_remaining: 1000,
                    max_scalar_bytes: 128 << 10,
                },
                &mut |_| panic!("structure-only fixture has no mailbox references"),
            )
            .unwrap();
            proof.validate_binding(&roots).unwrap();
            assert_eq!(proof.agent_count(), 1);
            assert_eq!(proof.mailbox_reference_count(), 0);
            use AgentFieldV1 as F;
            use AgentSubtreeValueV1 as V;
            for (field, ordinal, expected, text) in [
                (F::Identity, 0, Some(V::Text { utf8_bytes: 9 }), "agent 界"),
                (F::PositionX, 0, Some(V::I64(i64::MIN)), ""),
                (F::PositionZ, 0, Some(V::I64(i64::MAX)), ""),
                (
                    F::BodyKind,
                    0,
                    Some(V::Text { utf8_bytes: 10 }),
                    " raw body ",
                ),
                (F::BodyHeight, 0, Some(V::I64(i64::MIN)), ""),
                (F::ResourceAmount, 0, Some(V::I64(0)), ""),
                (F::ResourceAmount, 1, None, ""),
                (F::SlotCapacity, 0, Some(V::U16(u16::MAX)), ""),
                (F::ExpansionLevel, 0, Some(V::U16(u16::MAX)), ""),
                (F::SlotCount, 0, Some(V::U64(2)), ""),
                (F::SlotNext, 0, Some(V::U64(2)), ""),
                (F::SlotId, 1, Some(V::Text { utf8_bytes: 8 }), " repeat "),
                (F::SlotType, 1, Some(V::U8(5)), ""),
                (F::SlotInstalled, 1, Some(V::OptionalText(Some(0))), ""),
                (F::SlotLocked, 1, Some(V::Bool(true)), ""),
                (F::CargoKind, 1, Some(V::U8(2)), ""),
                (F::CargoQuantity, 1, Some(V::I64(i64::MIN)), ""),
                (F::CargoUnitSize, 1, Some(V::I64(i64::MAX)), ""),
                (F::ActivityPresent, 0, Some(V::Bool(true)), ""),
                (F::ActivityOperationId, 0, Some(V::OptionalU64(Some(0))), ""),
                (F::IntentSchema, 0, Some(V::U32(u32::MAX)), ""),
                (
                    F::IntentSource,
                    0,
                    Some(V::Text { utf8_bytes: 12 }),
                    " raw source ",
                ),
            ] {
                let _output = memory.reserve(1024, class).unwrap();
                let mut output = Vec::with_capacity(1024);
                assert_eq!(
                    read_agent_subtree_scalar_raw_v1(
                        &roots,
                        AgentSubtreeScalarRequestV1 {
                            agent: "agent 界",
                            field,
                            ordinal
                        },
                        &mut ctx,
                        128 << 10,
                        &mut output
                    )
                    .unwrap(),
                    expected
                );
                assert_eq!(output, text.as_bytes());
            }
            let _output = memory.reserve(66_000, class).unwrap();
            let mut output = Vec::with_capacity(66_000);
            assert_eq!(
                read_agent_subtree_scalar_raw_v1(
                    &roots,
                    AgentSubtreeScalarRequestV1 {
                        agent: "agent 界",
                        field: F::IntentSummary,
                        ordinal: 0
                    },
                    &mut ctx,
                    128 << 10,
                    &mut output
                )
                .unwrap(),
                Some(V::Text { utf8_bytes: 66_000 })
            );
            assert_eq!(output.len(), 66_000);
            assert!(output.chunks_exact(3).all(|bytes| bytes == "界".as_bytes()));
        }
    }
}

#[path = "support/incremental_agent_subtree_pages.rs"]
mod pages;

#[path = "support/incremental_agent_subtree_scaling.rs"]
mod scaling;
