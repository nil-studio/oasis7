use super::*;
pub(super) use crate::incremental_field_member::{MemberRecordLimitsV1, SemanticKeyBudget};
pub(super) use crate::incremental_typed_record_test_support::{Pages, Writer};
pub(super) use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, PreparedTransition,
        TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateNodeWriteV1, StateWriteV1},
    incremental_value::{ValueIoBudget, ValueNodeWriter},
};
pub(super) fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 1 << 30,
    }
}
pub(super) fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 1 << 30,
        leaves_remaining: 100000,
    }
}
pub(super) fn work() -> ModuleMarketplaceFamilyWorkV1 {
    ModuleMarketplaceFamilyWorkV1 {
        outer_rows_remaining: 100000,
        changed_cells_remaining: 1_000_000,
        membership_cells_remaining: 1_000_000,
        max_record_bytes: 4 << 20,
    }
}
pub(super) fn source() -> WorldState {
    let mut s = WorldState::default();
    s.module_visual_entities.insert(
        " original/visual ".into(),
        oasis7_agent_api::world_entity::ModuleVisualEntity {
            entity_id: "inner-distinct-entity".into(),
            module_id: "original-module".into(),
            kind: "original-kind".into(),
            label: Some("Ω".repeat(128 << 10)),
            anchor: ModuleVisualAnchor::Absolute {
                pos: oasis7_agent_api::world_action::GeoPos::new(i64::MIN, 0, i64::MAX),
            },
        },
    );
    s.module_artifact_listings.insert(
        " original/listing ".into(),
        ModuleArtifactListingState {
            order_id: u64::MAX,
            seller_agent_id: "卖方".repeat(32768),
            price_kind: ResourceKind::Data,
            price_amount: i64::MIN,
            listed_at: u64::MAX,
        },
    );
    s.module_artifact_bids.insert(
        " original/bids ".into(),
        (0..4)
            .map(|i| ModuleArtifactBidState {
                order_id: if i == 3 { 22 } else { 11 },
                bidder_agent_id: if i == 3 {
                    "other-bidder".into()
                } else {
                    "大买方".repeat(16384)
                },
                price_kind: ResourceKind::Electricity,
                price_amount: i,
                bid_at: i as u64,
            })
            .collect(),
    );
    s
}
pub(super) fn persist(memory: &MemoryBudget, pages: &Pages, nodes: &[StateNodeWriteV1]) {
    for node in nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
}
pub(super) fn install(memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    let mut writer = Writer(pages);
    let mut rows = Vec::new();
    prepare_module_marketplace_genesis_v1(
        &source(),
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 4 << 20,
                record_bytes: 8 << 20,
            },
            cancelled: || false,
        },
        &mut 1_000_000,
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut writes: Vec<StateWriteV1> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let mut roots = RootSetV1::empty("marketplace3-real-gen".into(), [5; 32]).unwrap();
    let delta = StateDeltaV1::build(
        roots.state_root(StateNamespaceV1::Business).clone(),
        writes,
        pages,
        16 << 20,
    )
    .unwrap();
    persist(memory, pages, &delta.new_nodes);
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap() = delta.after;
    roots
}
pub(super) fn context<'a>(
    memory: &'a MemoryBudget,
    pages: &'a Pages,
    roots: &'a RootSetV1,
    quota: &'a mut ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 1 << 20,
        record_bytes: 8 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
pub(super) fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    }
}
pub(super) fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 512,
        max_writes: 32,
        max_facts: 8,
        max_pending_node_bytes: 16 << 20,
    }
}
pub(super) fn outer(
    memory: &MemoryBudget,
    pages: &Pages,
    roots: &RootSetV1,
    owner: ModuleMarketplaceFieldV1,
    name: &str,
) -> RecordRootRefV1 {
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(memory, pages, roots, &mut quota, &mut keys);
    let (key, sem) = ctx
        .key(
            owner.descriptor().unwrap(),
            WorldFieldRecordKindV1::Entry,
            name,
        )
        .unwrap();
    let id = ctx.lookup(&key).unwrap().unwrap();
    let mut bytes = [0; oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &id,
        &key,
        &mut ctx.values,
        MemberReadProfileV1 {
            codec: crate::incremental_field_member::MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: sem,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: bytes.len() as u64,
                record_bytes: 8 << 20,
            },
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        || false,
    )
    .unwrap();
    RecordRootRefV1::from_canonical_bytes(&bytes).unwrap()
}
pub(super) fn child(reference: &RecordRootRefV1, owner: ModuleMarketplaceFieldV1) -> StateRootV1 {
    reference
        .validate_binding(
            &owner.schema(),
            &reference.owner_key,
            StateNamespaceV1::Business,
        )
        .unwrap()
        .clone()
}
pub(super) fn edit_child<K: SemanticKeyV1 + ?Sized>(
    memory: &MemoryBudget,
    pages: &Pages,
    owner: ModuleMarketplaceFieldV1,
    root: &mut StateRootV1,
    key: &K,
    value: Option<MemberPayloadRefV1<'_>>,
) {
    let field = owner.descriptor().unwrap();
    let digest = hash_semantic_key_v1(
        key,
        memory,
        MemoryClass::Ordinary,
        &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        || false,
    )
    .unwrap();
    let field_key =
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(digest.digest)).unwrap();
    let before = root.get(field_key.as_bytes(), pages).unwrap();
    let after = value.map(|value| {
        write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            key,
            value,
            &mut MemberRecordWriteContext {
                sink: &mut Writer(pages),
                memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 4 << 20,
                    record_bytes: 8 << 20,
                },
                cancelled: || false,
            },
        )
        .unwrap()
        .value_root
    });
    let delta = StateDeltaV1::build(
        root.clone(),
        vec![StateWriteV1 {
            key: field_key.as_bytes().to_vec(),
            before,
            after,
        }],
        pages,
        16 << 20,
    )
    .unwrap();
    persist(memory, pages, &delta.new_nodes);
    *root = delta.after;
}
pub(super) fn publish(
    memory: &MemoryBudget,
    pages: &Pages,
    tx: &mut ExecutionTransaction<'_, Pages>,
    owner: ModuleMarketplaceFieldV1,
    name: &str,
    root: StateRootV1,
) {
    write_outer_root(
        owner,
        owner.descriptor().unwrap(),
        name,
        root,
        &mut MemberRecordWriteContext {
            sink: &mut Writer(pages),
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 4 << 20,
                record_bytes: 8 << 20,
            },
            cancelled: || false,
        },
        &mut 10000,
        &mut |row| {
            tx.write(row.namespace, &row.mutation.key, row.mutation.after)
                .map_err(|_| ValueTreeError::Invalid("test publish"))?;
            Ok(())
        },
    )
    .unwrap();
}
pub(super) fn change_bid(
    memory: &MemoryBudget,
    pages: &Pages,
    tx: &mut ExecutionTransaction<'_, Pages>,
    expected: &RecordRootRefV1,
    change: ModuleArtifactBidChangeV1<'_>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) {
    let mut read = io();
    let mut write = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    change_module_bid_overlay_v1(
        " original/bids ",
        expected,
        change,
        &mut 1_000_000,
        &mut WorldFieldOverlayContextV1 {
            transaction: tx,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut read,
            },
            key_work: &mut rk,
            semantic_bytes: 1 << 20,
            record_bytes: 8 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut MemberRecordWriteContext {
            sink: &mut Writer(pages),
            memory,
            io: &mut write,
            key_work: &mut wk,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 4 << 20,
                record_bytes: 8 << 20,
            },
            cancelled: || false,
        },
        guard,
        live,
    )
    .unwrap();
}
