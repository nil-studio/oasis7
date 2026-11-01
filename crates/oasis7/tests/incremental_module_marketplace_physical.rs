#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_agent_api::{
    world_action::{GeoPos, ResourceKind},
    world_entity::{ModuleVisualAnchor, ModuleVisualEntity},
};
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
        TransitionVerifier,
    },
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateScanBudget,
        StateScanContext, StateWriteV1,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    ModuleArtifactBidState, ModuleArtifactListingState, WorldState,
    incremental_field_member::{
        MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1, MemberRecordWriteContext,
        SemanticKeyBudget, WorldFieldKeyV1, WorldFieldRecordKindV1, read_member_record_profile_v1,
    },
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_module_marketplace::{
        ModuleArtifactBidChangeV1, ModuleMarketplaceFieldV1, change_module_bid_overlay_v1,
        change_module_bid_price_amount_overlay_v1, prepare_module_marketplace_genesis_v1,
    },
    incremental_owner_closure::verify_world_state_owner_coverage,
    incremental_projection::{FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS},
    incremental_world_u64::{WorldU64FieldV1, prepare_world_u64_genesis_v1},
};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};

#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;

use fixture::{Directory, logical, physical};

const MAX_TEST_RECORD_BYTES: u64 = 2 << 20;
const MAX_TEST_VALUE_BYTES: u64 = 8 << 20;

fn source() -> WorldState {
    WorldState {
        module_visual_entities: BTreeMap::from([(
            "visual/雪".into(),
            ModuleVisualEntity {
                entity_id: "entity\0α".into(),
                module_id: "module/猫".into(),
                kind: "artifact\0mesh".into(),
                label: Some(String::new()),
                anchor: ModuleVisualAnchor::Absolute {
                    pos: GeoPos::new(i64::MIN, 17, i64::MAX),
                },
            },
        )]),
        module_artifact_listings: BTreeMap::from([(
            "listing/1".into(),
            ModuleArtifactListingState {
                order_id: u64::MAX,
                seller_agent_id: "seller/原".into(),
                price_kind: ResourceKind::Data,
                price_amount: i64::MIN,
                listed_at: 19,
            },
        )]),
        module_artifact_bids: BTreeMap::from([(
            "bids/组件".into(),
            vec![
                ModuleArtifactBidState {
                    order_id: 7,
                    bidder_agent_id: "buyer/一".into(),
                    price_kind: ResourceKind::Electricity,
                    price_amount: -5,
                    bid_at: 20,
                },
                ModuleArtifactBidState {
                    order_id: 8,
                    bidder_agent_id: "buyer/二".into(),
                    price_kind: ResourceKind::Data,
                    price_amount: 17,
                    bid_at: 21,
                },
            ],
        )]),
        ..WorldState::default()
    }
}

fn push_row(
    memory: &MemoryBudget,
    rows: &mut Vec<ReservedValue<oasis7_formal_protocol::incremental_execution::ExecutionWriteV1>>,
    namespace: StateNamespaceV1,
    key: &[u8],
    value_root: [u8; 32],
) {
    let charge = memory
        .reserve(
            std::mem::size_of::<oasis7_formal_protocol::incremental_execution::ExecutionWriteV1>()
                + key.len()
                + 128,
            MemoryClass::Ordinary,
        )
        .unwrap();
    rows.push(charge.retain(
        oasis7_formal_protocol::incremental_execution::ExecutionWriteV1 {
            namespace,
            mutation: StateWriteV1 {
                key: key.to_vec(),
                before: None,
                after: Some(value_root),
            },
        },
    ));
}

fn prepare_state_roots(path: &std::path::Path, compression: PageCompression) -> Vec<u8> {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(path, memory.clone(), compression).unwrap();
    let _batch = memory.reserve(256 << 10, MemoryClass::Ordinary).unwrap();
    let source = source();
    let mut rows = Vec::with_capacity(WORLD_STATE_FIELDS.len() + 32);
    let mut value_io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };

    // Metadata for every state descriptor makes the synthetic roots complete
    // at the schema-marker layer. The module-instance child visitor's eight
    // derived descriptors also need explicit empty-map markers. The
    // marketplace and seven u64 fields get their metadata from their real
    // typed producers below.
    let market_names = ModuleMarketplaceFieldV1::ALL.map(ModuleMarketplaceFieldV1::name);
    let u64_names = WorldU64FieldV1::ALL.map(WorldU64FieldV1::name);
    let mut value_writer = store.value_session(physical());
    let mut records_remaining = 10_000;
    for field in WORLD_STATE_FIELDS {
        let FieldDestination::State(namespace) = field.destination else {
            continue;
        };
        if market_names.contains(&field.name) || u64_names.contains(&field.name) {
            continue;
        }
        let record = write_field_metadata_v1(
            *field,
            &mut PresenceRecordWriteContext {
                sink: &mut value_writer,
                memory: &memory,
                io: &mut value_io,
                key_work: &mut key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: 0,
                    record_bytes: 512,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        push_row(
            &memory,
            &mut rows,
            namespace,
            record.field_key.as_bytes(),
            record.value_root,
        );
    }

    for name in [
        "module_instances.instance_id",
        "module_instances.module_id",
        "module_instances.module_version",
        "module_instances.wasm_hash",
        "module_instances.owner_agent_id",
        "module_instances.install_target",
        "module_instances.active",
        "module_instances.installed_at",
    ] {
        let descriptor = oasis7_runtime::incremental_projection::FieldDescriptor {
            owner: FieldOwner::WorldState,
            name,
            destination: FieldDestination::State(StateNamespaceV1::Business),
            shape: FieldShape::Map,
        };
        let record = write_field_metadata_v1(
            descriptor,
            &mut PresenceRecordWriteContext {
                sink: &mut value_writer,
                memory: &memory,
                io: &mut value_io,
                key_work: &mut key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: 0,
                    record_bytes: 512,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        push_row(
            &memory,
            &mut rows,
            StateNamespaceV1::Business,
            record.field_key.as_bytes(),
            record.value_root,
        );
    }

    let mut member_write = MemberRecordWriteContext {
        sink: &mut value_writer,
        memory: &memory,
        io: &mut value_io,
        key_work: &mut key_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 64 << 10,
            payload_bytes: 1 << 20,
            record_bytes: MAX_TEST_RECORD_BYTES,
        },
        cancelled: || false,
    };
    prepare_world_u64_genesis_v1(
        &source,
        &mut member_write,
        &mut records_remaining,
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    prepare_module_marketplace_genesis_v1(
        &source,
        &mut member_write,
        &mut records_remaining,
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    drop(value_writer);

    rows.sort_by(|left, right| {
        left.namespace
            .cmp(&right.namespace)
            .then_with(|| left.mutation.key.cmp(&right.mutation.key))
    });
    assert!(rows.windows(2).all(|pair| {
        (pair[0].namespace, pair[0].mutation.key.as_slice())
            < (pair[1].namespace, pair[1].mutation.key.as_slice())
    }));

    let mut roots = RootSetV1::empty("marketplace-physical".into(), [41; 32]).unwrap();
    let state_source = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    for index in 0..roots.state.len() {
        let namespace = roots.state[index].namespace;
        let writes = rows
            .iter()
            .filter(|row| row.namespace == namespace)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect::<Vec<_>>();
        if writes.is_empty() {
            continue;
        }
        assert!(writes.iter().all(|write| write.before.is_none()));
        let delta_workspace = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let delta = delta_workspace.retain(
            StateDeltaV1::build(roots.state[index].clone(), writes, &state_source, 8 << 20)
                .unwrap(),
        );
        let mut page_writer = store.value_session(physical());
        page_writer.put_state_pages(&delta, &memory).unwrap();
        roots.state[index] = delta.after.clone();
    }
    roots.canonical_bytes().unwrap()
}

fn verify_world_state_closure(
    store: &IncrementalPageStore,
    roots: &RootSetV1,
    memory: &MemoryBudget,
) -> u64 {
    let state = store
        .state_session(physical_full_scan(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_owner_coverage());
    // Generic value closure and owner qualification are one acceptance scan.
    // This is the sum of their former independent finite test allowances,
    // shared continuously across both phases.
    let mut value_io = oasis7_formal_protocol::incremental_value::ValueIoBudget {
        nodes_remaining: 200_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 256 << 20,
    };
    let mut value_context = ValueReadContext {
        source: &values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut value_io,
    };
    let mut state_io = StateScanBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100_000,
    };
    let mut leaves = 0u64;
    for root in &roots.state {
        let mut scan = StateScanContext {
            source: &state,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut state_io,
        };
        let scanned = scan.visit_entries(
            root,
            |entry| {
                value_context.verify_closure(&entry.value, MAX_TEST_VALUE_BYTES, || false)?;
                Ok::<(), ValueTreeError>(())
            },
            || false,
        );
        match scanned {
            Ok(count) => {
                leaves += count;
            }
            Err(error) => panic!(
                "generic closure scan failed: {error:?}; state scan remaining nodes={} bytes={} leaves={}; value remaining nodes={} bytes={}; physical remaining={:?}",
                state_io.nodes_remaining,
                state_io.read_bytes_remaining,
                state_io.leaves_remaining,
                value_context.io.nodes_remaining,
                value_context.io.read_bytes_remaining,
                state.remaining_io()
            ),
        }
    }
    assert!(state.take_read_error().is_none());
    assert!(leaves >= WORLD_STATE_FIELDS.len() as u64 - 8);
    let generic_scan_nodes_remaining = state_io.nodes_remaining;
    let generic_scan_bytes_remaining = state_io.read_bytes_remaining;
    let generic_scan_leaves_remaining = state_io.leaves_remaining;
    let generic_scan_nodes = 2_000_000 - state_io.nodes_remaining;
    let generic_scan_bytes = (256 << 20) - state_io.read_bytes_remaining;
    let generic_scan_leaves = 100_000 - state_io.leaves_remaining;
    let generic_value_nodes_remaining = value_context.io.nodes_remaining;
    let generic_value_bytes_remaining = value_context.io.read_bytes_remaining;
    let generic_value_output_remaining = value_context.io.write_bytes_remaining;
    let generic_value_nodes = 200_000 - value_context.io.nodes_remaining;
    let generic_value_bytes = (512 << 20) - value_context.io.read_bytes_remaining;
    let generic_value_output_bytes = (256 << 20) - value_context.io.write_bytes_remaining;
    let generic_physical = state.remaining_io().unwrap();
    let generic_value_physical = values.remaining_io().unwrap();

    let report_result = verify_world_state_owner_coverage(
        roots,
        &state,
        &mut value_context,
        &mut state_io,
        MAX_TEST_VALUE_BYTES,
        || false,
    );
    let owner_state_remaining = state.remaining_io().unwrap();
    let owner_value_remaining = values.remaining_io().unwrap();
    let report = report_result.unwrap_or_else(|error| {
        panic!(
            "owner coverage failed: {error:?}; state physical remaining={owner_state_remaining:?}; value physical remaining={owner_value_remaining:?}; state logical remaining nodes={} bytes={} leaves={}; value logical remaining nodes={} bytes={} output_bytes={}",
            state_io.nodes_remaining,
            state_io.read_bytes_remaining,
            state_io.leaves_remaining,
            value_io.nodes_remaining,
            value_io.read_bytes_remaining,
            value_io.write_bytes_remaining,
        )
    });
    assert_eq!(report.checked_module_marketplace_fields(), 3);
    assert!(
        !report.unsupported_fields().is_empty(),
        "the owner report must remain incomplete for the other WorldState families"
    );
    assert!(state.take_read_error().is_none());
    drop(report);
    let owner_value_nodes_remaining = value_io.nodes_remaining;
    let owner_value_bytes_remaining = value_io.read_bytes_remaining;
    let owner_value_output_remaining = value_io.write_bytes_remaining;
    let physical_remaining = state.remaining_io().unwrap();
    eprintln!(
        "combined generic+owner closure: generic_leaves={leaves}; generic scan nodes={generic_scan_nodes} bytes={generic_scan_bytes} leaves={generic_scan_leaves}; owner scan nodes={} bytes={} leaves={}; generic value nodes={generic_value_nodes} bytes={generic_value_bytes} output_bytes={generic_value_output_bytes}; owner value nodes={} bytes={} output_bytes={}; generic state physical reads={} bytes={}; owner state physical reads={} bytes={}; generic value physical reads={} bytes={}; owner value physical reads={} bytes={}",
        generic_scan_nodes_remaining - state_io.nodes_remaining,
        generic_scan_bytes_remaining - state_io.read_bytes_remaining,
        generic_scan_leaves_remaining - state_io.leaves_remaining,
        generic_value_nodes_remaining - owner_value_nodes_remaining,
        generic_value_bytes_remaining - owner_value_bytes_remaining,
        generic_value_output_remaining - owner_value_output_remaining,
        2_000_000 - generic_physical.reads_remaining,
        (256 << 20) - generic_physical.read_bytes_remaining,
        generic_physical.reads_remaining - physical_remaining.reads_remaining,
        generic_physical.read_bytes_remaining - physical_remaining.read_bytes_remaining,
        OWNER_CLOSURE_VALUE_READS - generic_value_physical.reads_remaining,
        (256 << 20) - generic_value_physical.read_bytes_remaining,
        generic_value_physical.reads_remaining - owner_value_remaining.reads_remaining,
        generic_value_physical.read_bytes_remaining - owner_value_remaining.read_bytes_remaining,
    );
    leaves
}

fn literal_text_semantic(value: &str) -> Vec<u8> {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.push(1);
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    bytes
}

fn literal_bid_price_semantic(ordinal: u64) -> Vec<u8> {
    literal_bid_member_semantic(ordinal, "price_amount")
}

fn literal_bid_member_semantic(ordinal: u64, name: &str) -> Vec<u8> {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.push(5); // ("bid", ordinal)
    bytes.push(5);
    bytes.push(1);
    bytes.extend_from_slice(&3u64.to_be_bytes());
    bytes.extend_from_slice(b"bid");
    bytes.push(2);
    bytes.extend_from_slice(&ordinal.to_be_bytes());
    bytes.push(1); // pair's right side: "price_amount"
    bytes.extend_from_slice(&(name.len() as u64).to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes
}

fn read_bid_price(
    store: &IncrementalPageStore,
    roots: &[StateRootV1; 6],
    memory: &MemoryBudget,
    outer_key: &str,
    ordinal: u64,
) -> i64 {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let field = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    let outer_semantic = literal_text_semantic(outer_key);
    let outer_key = WorldFieldKeyV1::new(
        field,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&outer_semantic).as_bytes()),
    )
    .unwrap();
    let outer_value = roots[0]
        .get(outer_key.as_bytes(), &state)
        .unwrap()
        .expect("marketplace outer record root");
    let mut value_io = logical();
    let mut value_context = ValueReadContext {
        source: &values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut value_io,
    };
    let mut returned_key = Vec::with_capacity(256);
    let mut root_bytes = [0u8; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &outer_value,
        &outer_key,
        &mut value_context,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: outer_semantic.len() as u64,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: 8192,
            },
        },
        &mut returned_key,
        &mut &mut root_bytes[..],
        || false,
    )
    .unwrap();
    assert_eq!(returned_key, outer_semantic);
    let reference = RecordRootRefV1::from_canonical_bytes(&root_bytes).unwrap();
    let child = reference
        .validate_binding(
            blake3::hash(b"oasis7.iws.market.bids-record.v2\0").as_bytes(),
            blake3::hash(&outer_semantic).as_bytes(),
            StateNamespaceV1::Business,
        )
        .unwrap();

    let semantic = literal_bid_price_semantic(ordinal);
    let member_key = WorldFieldKeyV1::new(
        field,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&semantic).as_bytes()),
    )
    .unwrap();
    let value = child
        .get(member_key.as_bytes(), &state)
        .unwrap()
        .expect("bid price member");
    let mut returned_semantic = Vec::with_capacity(256);
    let mut amount = [0u8; 8];
    read_member_record_profile_v1(
        &value,
        &member_key,
        &mut value_context,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::I64,
            semantic_bytes: semantic.len() as u64,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 8,
                record_bytes: 8192,
            },
        },
        &mut returned_semantic,
        &mut &mut amount[..],
        || false,
    )
    .unwrap();
    assert_eq!(returned_semantic, semantic);
    assert!(state.take_read_error().is_none());
    i64::from_be_bytes(amount)
}

struct CancelAfterPut<S> {
    inner: S,
    cancelled: Rc<Cell<bool>>,
    puts: usize,
}
impl<S> CancelAfterPut<S> {
    fn new(inner: S, cancelled: Rc<Cell<bool>>) -> Self {
        Self {
            inner,
            cancelled,
            puts: 0,
        }
    }
}
impl<S: ValueNodeReader> ValueNodeReader for CancelAfterPut<S> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.inner.read_node(id, max_bytes, memory, class)
    }
}
impl<S: ValueNodeWriter> ValueNodeWriter for CancelAfterPut<S> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        self.puts += 1;
        self.cancelled.set(true);
        Ok(())
    }
}
impl<S: StatePageWriter> StatePageWriter for CancelAfterPut<S> {}

fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 64,
        max_writes: 16,
        max_facts: 16,
        max_pending_node_bytes: 16 << 20,
    }
}

fn physical_full_scan() -> PageIoBudget {
    PageIoBudget {
        // A full sparse-tree traversal physically reloads shared path pages
        // while visiting leaves. The finite allowance covers the registered
        // WorldState schema at up to 256 leaves per scan; bytes stay at the
        // normal physical fixture limit.
        reads_remaining: 2_000_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}

// The largest observed owner scan used 120,414 physical value reads. Keep a
// finite margin without widening its independent 256 MiB byte allowance.
const OWNER_CLOSURE_VALUE_READS: u64 = 128_000;

fn physical_owner_coverage() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: OWNER_CLOSURE_VALUE_READS,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}

#[test]
fn marketplace_owner_closure_and_bid_update_survive_raw_zstd_cold_reopen() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let genesis_bytes = prepare_state_roots(&path, compression);

        // Only the canonical RootSet bytes cross the close/reopen boundary.
        let root_set = RootSetV1::from_canonical_bytes(&genesis_bytes).unwrap();
        {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            verify_world_state_closure(&store, &root_set, &memory);
            assert_eq!(
                read_bid_price(&store, &root_set.state, &memory, "bids/组件", 1),
                17
            );
        }

        let committed_bytes = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let parent = ExecutionParent {
                commit: [42; 32],
                roots: root_set.clone(),
                authority: AuthorityBinding {
                    token: [43; 32],
                    epoch: 2,
                },
            };
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [44; 32], &state, &memory, limits())
                    .unwrap();
            let guard = transaction.guard();
            let mut value_writer = store.value_session(physical());
            let mut value_io = logical();
            let mut key_work = SemanticKeyBudget {
                bytes_remaining: 64 << 10,
            };
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut transaction,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut value_io,
                },
                key_work: &mut key_work,
                semantic_bytes: 4096,
                record_bytes: MAX_TEST_RECORD_BYTES,
                take_state_error: || {
                    state.take_read_error().map(|error| match error {
                        PageStoreError::ResourceLimited(reason) => {
                            ValueTreeError::ResourceLimited(reason)
                        }
                        PageStoreError::Cancelled => ValueTreeError::Cancelled,
                        _ => ValueTreeError::Invalid("marketplace physical state read"),
                    })
                },
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut value_writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 64 << 10,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 1 << 20,
                    record_bytes: MAX_TEST_RECORD_BYTES,
                },
                cancelled: || false,
            };
            assert!(
                change_module_bid_price_amount_overlay_v1(
                    "bids/组件",
                    1,
                    17,
                    27,
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap()
            );
            let guard = transaction.guard();
            let prepared = transaction.seal(&guard, &live, 0).unwrap();
            let verifier_state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let verified = TransitionVerifier {
                parent: &parent,
                operation: [44; 32],
                source: &verifier_state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 16 << 20,
            }
            .verify(&prepared.record)
            .unwrap();
            assert_eq!(verified.record.writes.len(), 1);
            let mut destination = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            destination.persist_transition_pages(&verified).unwrap();
            verified.record.after.canonical_bytes().unwrap()
        };

        let committed = RootSetV1::from_canonical_bytes(&committed_bytes).unwrap();
        {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            verify_world_state_closure(&store, &committed, &memory);
            assert_eq!(
                read_bid_price(&store, &committed.state, &memory, "bids/组件", 1),
                27
            );

            // A cancellation after the real value writer accepted a node must
            // poison the transaction; the previously committed root stays the
            // only published value.
            let parent = ExecutionParent {
                commit: [45; 32],
                roots: committed.clone(),
                authority: AuthorityBinding {
                    token: [43; 32],
                    epoch: 2,
                },
            };
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [46; 32], &state, &memory, limits())
                    .unwrap();
            let guard = transaction.guard();
            let cancelled = Rc::new(Cell::new(false));
            let mut value_writer =
                CancelAfterPut::new(store.value_session(physical()), cancelled.clone());
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut transaction,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 64 << 10,
                },
                semantic_bytes: 4096,
                record_bytes: MAX_TEST_RECORD_BYTES,
                take_state_error: || {
                    state.take_read_error().map(|error| match error {
                        PageStoreError::ResourceLimited(reason) => {
                            ValueTreeError::ResourceLimited(reason)
                        }
                        PageStoreError::Cancelled => ValueTreeError::Cancelled,
                        _ => ValueTreeError::Invalid("marketplace physical state read"),
                    })
                },
                cancelled: || cancelled.get(),
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut value_writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 64 << 10,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 1 << 20,
                    record_bytes: MAX_TEST_RECORD_BYTES,
                },
                cancelled: || cancelled.get(),
            };
            let result = change_module_bid_price_amount_overlay_v1(
                "bids/组件",
                1,
                27,
                29,
                &mut read,
                &mut write,
                &guard,
                &live,
            );
            assert!(result.is_err(), "cancelled value write must not complete");
            assert!(cancelled.get());
            assert!(value_writer.puts > 0, "cancellation must follow a real put");
            let guard = transaction.guard();
            assert!(transaction.seal(&guard, &live, 0).is_err());
        }

        // Drop the writer, transaction and memory pool; only the earlier
        // committed RootSet remains. The aborted value prefix cannot replace
        // its bid amount.
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        assert_eq!(
            read_bid_price(&store, &committed.state, &memory, "bids/组件", 1),
            27
        );
    }
}

#[path = "support/incremental_module_marketplace_bid_membership_physical.rs"]
mod bid_membership_physical_tests;
