//! Bounded fresh-world current cells and empty side indexes, never a legacy
//! all-H import. No original Snapshot survives this constructor.
use super::*;
use oasis7_formal_protocol::{incremental_execution::RootSetV1, incremental_state::*};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_cognition_family::{
        CognitionCurrentStateQualificationV1, qualify_cognition_current_state_v1,
    },
    incremental_cognition_fields::CognitionEnvelopeRefV1,
    incremental_cognition_genesis::prepare_cognition_state_genesis_v1,
    incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1},
};
pub(super) struct EnabledGenesis {
    pub state: [StateRootV1; 6],
    pub indexes: [CognitionHistoryIndexRefV1; 4],
    pub heads: [Option<[u8; 32]>; 3],
}
pub(super) fn genesis(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    all: bool,
) -> EnabledGenesis {
    let source_memory = MemoryBudget::new(80 << 20, 0).unwrap();
    let _source = source_memory
        .reserve(
            if all {
                (64 << 20) + (1 << 20)
            } else {
                32 << 10
            },
            MemoryClass::Ordinary,
        )
        .unwrap();
    let _input = memory.reserve(32 << 10, MemoryClass::Ordinary).unwrap();
    let head = oasis7_agent_api::world_cognition::cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &serde_json::json!({"head_seq":0,"events":[]}),
    );
    let mut source = serde_json::json!({
        "schema_version":"cognition-persistence.v1",
        "cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":0,"head_digest":head,"events":[]},
        "responses":[],"commit_records":[],"receipt_registry":[],"receipt_lineage_registry":[],
        "feedback_outbox":{},"idempotency_index":{},"staged_actions":{},"scheduler_state":{},"continuations":[],"recovery":null
    });
    if all {
        source["feedback_outbox"]["large"] = serde_json::Value::String("s".repeat(64 << 20));
        source["continuation_contexts"] = serde_json::json!({});
        source["continuation_evaluations"] = serde_json::json!({});
        source["reorg_invalidation_epoch"] = serde_json::json!(0);
        source["cognition_economy"] =
            serde_json::to_value(oasis7_runtime::CognitionEconomyStateV1::default()).unwrap();
        source["agent_delegation"] =
            serde_json::json!({"grants":{},"spent":{},"decisions":{},"receipts":{},"controls":{}});
    }
    let parse = |value: &str| {
        *blake3::Hash::from_hex(value.strip_prefix("blake3:").unwrap())
            .unwrap()
            .as_bytes()
    };
    let heads = [
        Some(parse(&head)),
        if all {
            Some(parse(
                source["cognition_economy"]["head_digest"].as_str().unwrap(),
            ))
        } else {
            None
        },
        if all {
            Some(parse(
                source["cognition_economy"]["provision_head_digest"]
                    .as_str()
                    .unwrap(),
            ))
        } else {
            None
        },
    ];
    let _batch = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let mut writes = Vec::with_capacity(64);
    let mut writer = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    prepare_cognition_state_genesis_v1(
        CognitionEnvelopeRefV1::new(&source).unwrap(),
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut io,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: (64 << 20) + 8192,
                record_bytes: (64 << 20) + 8192,
            },
            cancelled: || false,
        },
        &mut 64,
        &mut JsonWriteWorkV1 {
            cells_remaining: 4096,
            json_nodes_remaining: 512,
            max_json_depth: 8,
            max_pending_node_bytes: 2 << 20,
        },
        &mut |write| {
            assert!(writes.len() < 64);
            writes.push(write);
            Ok(())
        },
    )
    .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let roots = std::array::from_fn(|index| {
        let namespace = [
            StateNamespaceV1::Business,
            StateNamespaceV1::Allocator,
            StateNamespaceV1::Scheduler,
            StateNamespaceV1::Authorization,
            StateNamespaceV1::Effects,
            StateNamespaceV1::Artifacts,
        ][index];
        let count = writes
            .iter()
            .filter(|write| write.namespace == namespace)
            .count();
        let admission = memory
            .reserve(count * (512 << 10) + (128 << 10), MemoryClass::Ordinary)
            .unwrap();
        let mut changes = Vec::with_capacity(count);
        for write in writes.iter().filter(|write| write.namespace == namespace) {
            changes.push(StateWriteV1 {
                key: write.mutation.key.clone(),
                before: None,
                after: write.mutation.after,
            });
        }
        changes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = admission.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                changes,
                &state,
                2 << 20,
            )
            .unwrap(),
        );
        writer
            .put_state_pages_cancelled(&delta, memory, &mut || false)
            .unwrap();
        delta.after.clone()
    });
    let indexes = std::array::from_fn(|index| {
        if index >= 2 && !all {
            return CognitionHistoryIndexRefV1 {
                present: false,
                records: 0,
                root: StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            };
        }
        let root = prepare_cognition_history_index_v1(
            CognitionHistoryFieldV1::ALL[index],
            0,
            std::iter::empty(),
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory,
                io: &mut io,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 256,
                    payload_bytes: 256,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
            &mut CognitionHistoryIndexWorkV1 {
                cells_remaining: 8,
                max_pending_node_bytes: 1 << 20,
            },
        )
        .unwrap();
        CognitionHistoryIndexRefV1 {
            present: true,
            records: 0,
            root,
        }
    });
    EnabledGenesis {
        state: roots,
        indexes,
        heads,
    }
}
pub(super) fn qualify_current(
    roots: &RootSetV1,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> ReservedValue<CognitionCurrentStateQualificationV1> {
    let state = store.state_session(physical(), class).unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    qualify_cognition_current_state_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Allocator),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: (64 << 20) + 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 128 << 20,
            leaves_remaining: 1024,
        },
        &mut JsonReadWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 1024,
            max_depth: 8,
            max_payload_bytes: (64 << 20) + 8192,
        },
        &mut 256,
    )
    .unwrap()
}
