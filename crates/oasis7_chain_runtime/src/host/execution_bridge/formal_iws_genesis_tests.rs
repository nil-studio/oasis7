use super::super::super::formal_iws_genesis::*;
use super::*;
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_world_state::{
    incremental_event_fields::{MAILBOX_EVENT_REFERENCE_BYTES, mailbox_event_id_v1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_pending_action_record::{
        PendingActionPositionV1, PendingActionReadWorkV1, recover_pending_action_record_v1,
    },
};
fn transfer_work() -> IwsGenesisTransferWorkV1 {
    let outer_nodes: u64 = 1_000_000;
    let mut outer_reads = work::physical_io();
    // An SMP node fits one Raw frame. Its read needs FORMAT plus recipe and
    // frame lookup; each packed member needs prefix/header, at most nine
    // comparisons (256 entries), and payload. Include one marker fallback:
    // 1 + 12 + 12 + 1 = 26 physical reads per scanned node.
    outer_reads.reads_remaining = outer_nodes.checked_mul(26).unwrap();
    IwsGenesisTransferWorkV1 {
        objects_physical: std::array::from_fn(|_| work::physical_io()),
        objects_logical: std::array::from_fn(|_| work::logical()),
        page_publish: std::array::from_fn(|_| work::physical_io()),
        outer_reads,
        outer_scan: StateScanBudget {
            nodes_remaining: outer_nodes,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 100_000,
        },
        canonical: canonical_work(),
        originals_remaining: 100_000,
        max_original_bytes: 8 << 20,
        mailbox_search_steps: 100_000,
    }
}
#[test]
fn real_genesis_without_snapshot_material_transfers_pages_and_originals_to_both_endpoints() {
    let fixture = Fixture::new();
    let memory: [MemoryBudget; 3] =
        std::array::from_fn(|_| MemoryBudget::new(512 << 20, 32 << 20).unwrap());
    let _input = memory[0].reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let world = source();
    let cancelled = AtomicBool::new(false);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|index| {
        IncrementalPageStore::create(
            &fixture.0.join(format!("replica-{index}")),
            memory[index].clone(),
            PageCompression::Raw,
        )
        .unwrap()
    });
    let mut logical = work::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut transfer = transfer_work();
    let result = prepare_iws_genesis_for_endpoints_v1(
        &world,
        &stores[0],
        [&stores[1], &stores[2]],
        &config(),
        &mut RuntimeGenesisPhysicalContextV1 {
            memory: &memory[0],
            values: &mut logical,
            keys: &mut keys,
            limits: limits(),
            cancelled: &cancelled,
        },
        &mut work::work(),
        work::physical(),
        &mut transfer,
    )
    .unwrap_or_else(|error| {
        panic!(
            "GEN transfer: {error:?}; remaining objects={:?}, publish={:?}, outer={:?}, scan={:?}",
            transfer.objects_physical,
            transfer.page_publish,
            transfer.outer_reads,
            (
                transfer.outer_scan.nodes_remaining,
                transfer.outer_scan.read_bytes_remaining,
                transfer.outer_scan.leaves_remaining
            ),
        )
    });
    assert_eq!(result.summary.typed.runtime_fields, 46);
    for store in &stores[1..] {
        let pool = store.memory_budget();
        let state = store
            .state_session_with_cancelled(work::physical_io(), MemoryClass::Recovery, &cancelled)
            .unwrap();
        let values = store.value_session_with_cancelled(work::physical_io(), &cancelled);
        let mut logical = work::logical();
        let mut read = ValueReadContext {
            source: &values,
            memory: pool,
            class: MemoryClass::Recovery,
            io: &mut logical,
        };
        let key = MailboxKeyV1::new("alice", Some(0)).unwrap();
        let id = result
            .roots
            .state_root(StateNamespaceV1::Scheduler)
            .get(key.as_bytes(), &state)
            .unwrap()
            .unwrap();
        let root = read.load_root(&id).unwrap();
        let mut reference = [0; MAILBOX_EVENT_REFERENCE_BYTES];
        read.read_range(
            &root,
            0,
            reference.len() as u64,
            &mut &mut reference[..],
            || false,
        )
        .unwrap();
        let event_id = mailbox_event_id_v1(&reference).unwrap();
        let mut physical = work::physical_io();
        let objects =
            IncrementalObjectStore::open(store, MemoryClass::Recovery, &mut physical, &cancelled)
                .unwrap();
        let mut object_logical = work::logical();
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut object_logical,
            class: MemoryClass::Recovery,
            cancelled: &cancelled,
        };
        let raw = objects
            .read_object_owned(&event_id, 8 << 20, &mut io)
            .unwrap();
        let actual: oasis7_world_state::WorldEvent = serde_cbor::from_slice(&raw).unwrap();
        assert!(world.journal().events.contains(&actual));
        drop(read);
        let mut pending_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: result.roots.state_root(StateNamespaceV1::Scheduler),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: pool,
                class: MemoryClass::Recovery,
                io: &mut logical,
            },
            key_work: &mut pending_keys,
            semantic_bytes: 4096,
            record_bytes: 9 << 20,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e)))
            },
            cancelled: || false,
        };
        let pending = recover_pending_action_record_v1(
            PendingActionPositionV1 { era: 0, ordinal: 0 },
            &mut context,
            &mut StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 100_000,
            },
            &mut PendingActionReadWorkV1 {
                max_retained_trees: 4096,
                max_tree_heap: 1 << 20,
                json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
                    cells_remaining: 100_000,
                    json_nodes_remaining: 100_000,
                    max_depth: 64,
                    max_payload_bytes: 1 << 20,
                },
            },
        )
        .unwrap()
        .unwrap();
        let original = pending.value().original().unwrap();
        let action = objects
            .read_object_owned(&original.id, 8 << 20, &mut io)
            .unwrap();
        let actual: ActionEnvelope = serde_cbor::from_slice(&action).unwrap();
        assert_eq!(pending.value().envelope(), &actual);
        assert!(
            matches!(actual.action,Action::QueryObservation{ref agent_id} if agent_id=="alice")
        );
        assert!(state.take_read_error().is_none());
    }
}
#[test]
fn a_replica_page_failure_returns_no_genesis_root_and_preserves_prior_bytes() {
    let fixture = Fixture::new();
    let cancelled = AtomicBool::new(false);
    let memory: [MemoryBudget; 3] =
        std::array::from_fn(|_| MemoryBudget::new(512 << 20, 32 << 20).unwrap());
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|index| {
        IncrementalPageStore::create(
            &fixture.0.join(format!("replica-{index}")),
            memory[index].clone(),
            PageCompression::Raw,
        )
        .unwrap()
    });
    let bytes = b"existing immutable data before failed GEN";
    let id = *blake3::hash(bytes).as_bytes();
    for store in &stores[1..] {
        let value = store
            .memory_budget()
            .reserve(bytes.len(), MemoryClass::Ordinary)
            .unwrap()
            .retain(bytes.to_vec());
        store
            .put_page(id, &value, &mut work::physical_io(), &cancelled)
            .unwrap();
    }
    let _input = memory[0].reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let world = source();
    let mut logical = work::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut transfer = transfer_work();
    transfer.page_publish[1].writes_remaining = 0;
    let result = prepare_iws_genesis_for_endpoints_v1(
        &world,
        &stores[0],
        [&stores[1], &stores[2]],
        &config(),
        &mut RuntimeGenesisPhysicalContextV1 {
            memory: &memory[0],
            values: &mut logical,
            keys: &mut keys,
            limits: limits(),
            cancelled: &cancelled,
        },
        &mut work::work(),
        work::physical(),
        &mut transfer,
    );
    assert!(matches!(
        result,
        Err(IwsGenesisTransferErrorV1::Store(
            oasis7_distfs::incremental_page_store::PageStoreError::ResourceLimited(
                "physical write allowance"
            )
        ))
    ));
    for store in &stores[1..] {
        let actual = store
            .read_page(
                &id,
                bytes.len(),
                MemoryClass::Recovery,
                &mut work::physical_io(),
                &cancelled,
            )
            .unwrap();
        assert_eq!(actual.as_slice(), bytes);
    }
}
