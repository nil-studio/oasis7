use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{Pages, Writer, io};
use crate::incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100000,
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    }
}
fn rows(
    source: &crate::WorldState,
    pages: &Pages,
    memory: &MemoryBudget,
) -> Vec<ReservedValue<ExecutionWriteV1>> {
    let mut output = Vec::new();
    prepare_factory_authority_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(pages),
            memory,
            io: &mut io(),
            key_work: &mut keys(),
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut FactoryAuthorityWorkV1 {
            cells_remaining: 100000,
            max_pending_node_bytes: 4 << 20,
        },
        &mut |row| {
            output.push(row);
            Ok(())
        },
    )
    .unwrap();
    output
}
#[test]
fn actual_current_site_opener_reads_inserted_owner_body_and_rejects_deleted_metadata() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let before = crate::WorldState::default();
    let before_rows = rows(&before, &pages, &memory);
    let mut writes: Vec<StateWriteV1> =
        before_rows.iter().map(|row| row.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
        writes,
        &pages,
        4 << 20,
    )
    .unwrap();
    for node in delta.new_nodes {
        Writer(&pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes),
            )
            .unwrap();
    }
    let mut roots = RootSetV1::empty("site-current".into(), [1; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap() = delta.after;
    let mut vi = io();
    let mut ki = keys();
    let mut read = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut ki,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let parent = qualify_factory_authority_family_v1(
        &roots,
        &mut read,
        &mut scan(),
        &mut scan(),
        &mut 100000,
        1 << 20,
    )
    .unwrap();
    let execution_parent = ExecutionParent {
        roots: roots.clone(),
        commit: [2; 32],
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 4,
        },
    };
    let live = execution_parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        execution_parent,
        [4; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 32,
            max_facts: 8,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let mut after = crate::WorldState::default();
    after.factory_site_authorities.insert(
        "site".into(),
        crate::FactorySiteAuthorityV1 {
            site_id: "site".into(),
            owner_agent_id: "owner".into(),
            active: true,
            chunk_ready: true,
            ..Default::default()
        },
    );
    for row in rows(&after, &pages, &memory) {
        tx.write(row.namespace, &row.mutation.key, row.mutation.after)
            .unwrap();
    }
    let mut oi = io();
    let mut ok = keys();
    let guard = {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut oi,
            },
            key_work: &mut ok,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let key = FactoryAuthorityRecordKeyV1 {
            kind: FactoryAuthorityRecordKindV1::Site,
            id: "site",
        };
        let guard = overlay.transaction.guard();
        let body = open_factory_authority_body_overlay_v1(
            key,
            &parent,
            &pages,
            &mut overlay,
            &mut scan(),
            &mut 100000,
            1 << 20,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        let guard = overlay.transaction.guard();
        let mut owner = Vec::new();
        read_qualified_factory_authority_cell_overlay_v1(
            FactoryAuthorityCellRequestV1 {
                key,
                cell: FactoryAuthorityCellV1::OwnerAgentId,
                qualification: &body,
            },
            &mut overlay,
            4096,
            &mut owner,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(
            owner,
            after.factory_site_authorities["site"]
                .owner_agent_id
                .as_bytes()
        );
        let metadata = WorldFieldKeyV1::new(
            key.kind().descriptor().unwrap(),
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .unwrap();
        overlay
            .transaction
            .write(StateNamespaceV1::Business, metadata.as_bytes(), None)
            .unwrap();
        let guard = overlay.transaction.guard();
        assert!(
            open_factory_authority_body_overlay_v1(
                key,
                &parent,
                &pages,
                &mut overlay,
                &mut scan(),
                &mut 100000,
                1 << 20,
                &guard,
                &live
            )
            .is_err()
        );
        overlay.transaction.guard()
    };
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
