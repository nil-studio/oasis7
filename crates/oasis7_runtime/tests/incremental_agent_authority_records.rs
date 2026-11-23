#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_agent_authority_records::*, incremental_field_member::*,
    incremental_field_read::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, physical};
fn physical_scope() -> PageIoBudget {
    let mut io = physical();
    io.reads_remaining = 2_000_000;
    io
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
fn source() -> WorldState {
    let mut world = WorldState::default();
    world.agent_intent_ledger.insert(" outer intent ".into(),serde_json::from_value(serde_json::json!({
  "schema_version":u32::MAX,"agent_id":" embedded agent ","intent_id":" embedded intent ","kind":" Raw Kind ","summary":" original summary ",
  "target_id":null,"effect_intent_id":"","intent_tick":0,"world_id":" raw world ","reorg_epoch":u64::MAX,"authority_scope":null,
  "status":" original status ","source":" legacy source ","logical_time":u64::MAX,"event_seq":17,"updated_at":u64::MAX,
  "receipt_ref":"","reason_code":null,"reason_summary":" 原reason ","replaced_by":null,"actor_id":" actor ","request_digest":" RawDigest "
 })).unwrap());
    world.agent_location_authorities.insert(" outer agent ".into(),serde_json::from_value(serde_json::json!({"agent_id":" original agent ","location_id":" original location ","active":false,"authority_revision":0,"effective_at":u64::MAX})).unwrap());
    world.location_anchors.insert("".into(),serde_json::from_value(serde_json::json!({"location_id":" embedded anchor ","active":false,"authority_revision":0,"effective_at":0})).unwrap());
    world
}
fn work() -> AgentAuthorityWorkBudgetV1 {
    AgentAuthorityWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
        scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 100_000,
        },
    }
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> RootSetV1 {
    let mut values = store.value_session(physical_scope());
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(32);
    let mut remaining = 100_000;
    let count = prepare_agent_authority_records_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut remaining,
        &mut work(),
        &mut |row| {
            assert!(rows.len() < 32);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let expected = source.agent_intent_ledger.len()
        + source.agent_location_authorities.len()
        + source.location_anchors.len();
    assert_eq!(count, expected as u64);
    assert_eq!(rows.len(), 6 + expected);
    let mut roots = RootSetV1::empty("agent_authority-source".into(), [0x41; 32]).unwrap();
    for ns in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == ns)
            .map(|r| StateWriteV1 {
                key: r.mutation.key.clone(),
                before: None,
                after: r.mutation.after,
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, &state, 8 << 20).unwrap(),
        );
        values.put_state_pages(&delta, memory).unwrap();
        let index = roots.state.iter().position(|r| r.namespace == ns).unwrap();
        roots.state[index] = delta.after.clone();
    }
    roots
}

fn qualify(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    class: MemoryClass,
) -> ReservedValue<AgentAuthorityFamilyQualificationV1> {
    let state = store.state_session(physical_scope(), class).unwrap();
    let values = store.value_session(physical_scope());
    let root = &roots.state[0];
    qualify_agent_authority_family_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|_| ValueTreeError::Invalid("agent authority qualifier"))
            },
            cancelled: || false,
        },
        &mut work().scan,
        &mut work().scan,
        &mut 1000,
        1 << 20,
    )
    .unwrap()
}
#[test]
fn original_agent_authority_records_cold_recover_all_three_families() {
    use AgentAuthorityColumnV1 as C;
    use AgentAuthorityRecordKindV1 as K;
    use IntentFieldV1 as I;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory, &source())
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let _caller = memory.reserve(16 << 10, class).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let qualification = qualify(&store, &memory, &roots, class);
            assert_eq!(qualification.record_counts(), [1, 1, 1]);
            qualification
                .validate_binding(&roots.digest().unwrap())
                .unwrap();
            let state = store.state_session(physical_scope(), class).unwrap();
            let values = store.value_session(physical_scope());
            for (kind, id, column, expected, text) in [
                (
                    K::IntentLedger,
                    " outer intent ",
                    C::Intent(I::SchemaVersion),
                    AgentAuthorityValueV1::Unsigned(u64::from(u32::MAX)),
                    "",
                ),
                (
                    K::IntentLedger,
                    " outer intent ",
                    C::Intent(I::AgentId),
                    AgentAuthorityValueV1::Text { utf8_bytes: 16 },
                    " embedded agent ",
                ),
                (
                    K::IntentLedger,
                    " outer intent ",
                    C::Intent(I::TargetId),
                    AgentAuthorityValueV1::Unit,
                    "",
                ),
                (
                    K::IntentLedger,
                    " outer intent ",
                    C::Intent(I::EffectIntentId),
                    AgentAuthorityValueV1::Text { utf8_bytes: 0 },
                    "",
                ),
                (
                    K::IntentLedger,
                    " outer intent ",
                    C::Intent(I::IntentTick),
                    AgentAuthorityValueV1::Unsigned(0),
                    "",
                ),
                (
                    K::IntentLedger,
                    " outer intent ",
                    C::Intent(I::ReorgEpoch),
                    AgentAuthorityValueV1::Unsigned(u64::MAX),
                    "",
                ),
                (
                    K::AgentLocation,
                    " outer agent ",
                    C::LocationId,
                    AgentAuthorityValueV1::Text { utf8_bytes: 19 },
                    " original location ",
                ),
                (
                    K::AgentLocation,
                    " outer agent ",
                    C::Active,
                    AgentAuthorityValueV1::Bool(false),
                    "",
                ),
                (
                    K::LocationAnchor,
                    "",
                    C::AuthorityRevision,
                    AgentAuthorityValueV1::Unsigned(0),
                    "",
                ),
            ] {
                let key = AgentAuthorityRecordKeyV1 { kind, id };
                let root = roots
                    .state
                    .iter()
                    .find(|root| root.namespace == kind.namespace())
                    .unwrap();
                let mut context = WorldFieldReadContextV1 {
                    root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("agent authority cold point"))
                    },
                    cancelled: || false,
                };
                let record = open_qualified_agent_authority_record_v1(
                    key,
                    &roots,
                    &qualification,
                    &mut context,
                )
                .unwrap()
                .unwrap();
                let _out = memory.reserve(128, class).unwrap();
                let mut output = Vec::new();
                let got = read_qualified_agent_authority_cell_v1(
                    AgentAuthorityCellRequestV1 {
                        key,
                        cell: AgentAuthorityCellV1::Column(column),
                        qualification: &record,
                    },
                    &mut context,
                    128,
                    &mut output,
                )
                .unwrap();
                assert_eq!(got, expected);
                assert_eq!(output, text.as_bytes());
            }
        }
    }
}

#[path = "support/incremental_agent_authority_overlay.rs"]
mod overlay;

#[path = "support/incremental_agent_authority_points.rs"]
mod points;

#[path = "support/incremental_agent_authority_pages.rs"]
mod pages;

#[path = "support/incremental_agent_authority_scaling.rs"]
mod scaling;

#[path = "support/incremental_agent_authority_malformed.rs"]
mod malformed;

#[path = "support/incremental_agent_authority_carriers.rs"]
mod carriers;

#[path = "support/incremental_agent_authority_missing.rs"]
mod missing;

#[path = "support/incremental_agent_authority_put.rs"]
mod put;

#[path = "support/incremental_agent_authority_oracle.rs"]
mod oracle;
