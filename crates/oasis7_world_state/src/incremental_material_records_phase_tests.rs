//! Real GEN -> Recovery selected proof -> Ordinary setter admission boundary.
use super::*;
use crate::{
    WorldState,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::{SemanticKeyBudget, hash_semantic_key_v1},
    incremental_typed_record_test_support::{ClassedPages, Pages, Writer},
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 200_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 200_000,
        leaves_remaining: 10_000,
        read_bytes_remaining: 64 << 20,
    }
}
struct Fixture {
    memory: MemoryBudget,
    pages: Pages,
    roots: RootSetV1,
    proof: ReservedValue<MaterialRecordQualificationV1>,
    owner: [u8; 32],
    body: ReservedValue<StateRootV1>,
    _input: oasis7_proto::memory_budget::MemoryReservation,
}
fn fixture() -> Fixture {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut source = WorldState::default();
    source.material_profiles.clear();
    source.direct_material_transfer_receipts.clear();
    source.material_profiles.insert(
        "outer profile".into(),
        MaterialProfileV1 {
            kind: "different inner kind".into(),
            tier: 1,
            category: "category".into(),
            stack_limit: 8,
            transport_loss_class: MaterialTransportLossClass::Low,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Standard,
        },
    );
    let pages = Pages::new(&memory);
    let mut out = Writer(&pages);
    let mut vi = io();
    let mut kw = keys();
    let mut rows = Vec::with_capacity(5);
    prepare_material_records_genesis_v1(
        &source,
        &mut MemberRecordWriteContext {
            sink: &mut out,
            memory: &memory,
            io: &mut vi,
            key_work: &mut kw,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 2 << 20,
                payload_bytes: 2 << 20,
                record_bytes: 4 << 20,
            },
            cancelled: || false,
        },
        &mut 1000,
        &mut MaterialWorkBudgetV1 {
            cells_remaining: 10_000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        &mut |row: ReservedValue<ExecutionWriteV1>| {
            assert!(rows.len() < 5);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut roots = RootSetV1::empty("record-class-boundary".into(), [2; 32]).unwrap();
    for namespace in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == namespace)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = StateDeltaV1::build(
            StateRootV1::empty(namespace).unwrap(),
            writes,
            &pages,
            8 << 20,
        )
        .unwrap();
        for n in &delta.new_nodes {
            let id = *blake3::Hash::from_hex(&n.digest).unwrap().as_bytes();
            out.put_node(
                id,
                memory
                    .reserve(n.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(n.bytes.clone()),
            )
            .unwrap();
        }
        *roots
            .state
            .iter_mut()
            .find(|r| r.namespace == namespace)
            .unwrap() = delta.after;
    }
    let mut vi = io();
    let mut kw = keys();
    let body = inspect_material_record_root_v1(
        MaterialRecordKeyV1::Profile("outer profile"),
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
    )
    .unwrap()
    .unwrap();
    let owner = hash_semantic_key_v1(
        "outer profile",
        &memory,
        MemoryClass::Recovery,
        &mut keys(),
        &mut || false,
    )
    .unwrap()
    .digest;
    let recovery = ClassedPages::new(&pages, &memory, MemoryClass::Recovery);
    let mut vi = io();
    let mut kw = keys();
    let proof = qualify_material_body_v1(
        MaterialRecordKindV1::Profile,
        owner,
        &mut WorldFieldReadContextV1 {
            root: &body,
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut 1000,
        2 << 20,
    )
    .unwrap();
    Fixture {
        memory,
        pages,
        roots,
        proof,
        owner,
        body,
        _input,
    }
}
#[test]
fn material_recovery_selected_proof_cannot_enter_ordinary_setter_without_readmission() {
    let Fixture {
        memory,
        pages,
        roots,
        proof,
        owner: _,
        body: _,
        _input,
    } = fixture();
    let mut out = Writer(&pages);
    for after in [9, 8] {
        let p = ExecutionParent {
            roots: roots.clone(),
            commit: [3; 32],
            authority: AuthorityBinding {
                token: [4; 32],
                epoch: 5,
            },
        };
        let live = p.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            p,
            [6; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let guard = tx.guard();
        let mut vi = io();
        let mut kw = keys();
        let mut wi = io();
        let mut wk = keys();
        let result = change_qualified_material_cell_v1(
            MaterialCellChangeV1 {
                key: MaterialRecordKeyV1::Profile("outer profile"),
                cell: MaterialCellV1::StackLimit,
                qualification: &proof,
                expected: Some(MaterialCellValueRefV1::I64(8)),
                after: Some(MaterialCellValueRefV1::I64(after)),
                max_current_payload_bytes: 8,
                max_pending_node_bytes: 8 << 20,
            },
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut vi,
                },
                key_work: &mut kw,
                semantic_bytes: 2 << 20,
                record_bytes: 4 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            &mut MemberRecordWriteContext {
                sink: &mut out,
                memory: &memory,
                io: &mut wi,
                key_work: &mut wk,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 2 << 20,
                    payload_bytes: 2 << 20,
                    record_bytes: 4 << 20,
                },
                cancelled: || false,
            },
            &guard,
            &live,
        );
        assert!(
            matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "Recovery proof entered Ordinary setter before explicit readmission, after={after}"
        );
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[path = "incremental_material_records_phase_green_tests.rs"]
mod green;
