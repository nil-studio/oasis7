use super::*;
use crate::{
    incremental_field_member::{
        MemberRecordLimitsV1, MemberRecordWriteContext, SemanticKeyBudget, WorldFieldKeyV1,
    },
    state::{ModuleArtifactBidState, ModuleArtifactListingState, WorldState},
};
use oasis7_agent_api::{
    world_action::{GeoPos, ResourceKind},
    world_entity::{ModuleVisualAnchor, ModuleVisualEntity},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_value::{ValueIoBudget, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::collections::BTreeMap;

#[derive(Default)]
struct Values(BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>);

impl ValueNodeWriter for Values {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if blake3::hash(bytes.as_slice()).as_bytes() != &id {
            return Err(ValueTreeError::Invalid("marketplace test value hash"));
        }
        self.0.insert(id, bytes);
        Ok(())
    }
}

fn source() -> WorldState {
    let mut world = WorldState::default();
    world.module_visual_entities.insert(
        "visual/雪".into(),
        ModuleVisualEntity {
            entity_id: "entity/原文".into(),
            module_id: "module/α".into(),
            kind: "artifact\0mesh".into(),
            label: Some(String::new()),
            anchor: ModuleVisualAnchor::Absolute {
                pos: GeoPos::new(i64::MIN, 0, i64::MAX),
            },
        },
    );
    world.module_artifact_listings.insert(
        "listing/订单".into(),
        ModuleArtifactListingState {
            order_id: u64::MAX,
            seller_agent_id: "seller/λ".into(),
            price_kind: ResourceKind::Data,
            price_amount: i64::MIN,
            listed_at: u64::MAX,
        },
    );
    world.module_artifact_bids.insert(
        "bid-map-key".into(),
        vec![
            ModuleArtifactBidState {
                order_id: u64::MAX,
                bidder_agent_id: "bidder/東京".into(),
                price_kind: ResourceKind::Electricity,
                price_amount: i64::MIN,
                bid_at: 0,
            },
            ModuleArtifactBidState {
                order_id: 0,
                bidder_agent_id: "bidder/β".into(),
                price_kind: ResourceKind::Data,
                price_amount: i64::MAX,
                bid_at: u64::MAX,
            },
        ],
    );
    world
}

fn context<'a>(
    values: &'a mut Values,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    key_work: &'a mut SemanticKeyBudget,
    cancelled: impl FnMut() -> bool,
) -> MemberRecordWriteContext<'a, Values, impl FnMut() -> bool> {
    MemberRecordWriteContext {
        sink: values,
        memory,
        io,
        key_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled,
    }
}

#[test]
fn borrowed_genesis_emits_registered_metadata_and_typed_original_map_entries() {
    let world = source();
    let visual_ptr = world.module_visual_entities["visual/雪"].kind.as_ptr();
    let listing_ptr = world.module_artifact_listings["listing/订单"]
        .seller_agent_id
        .as_ptr();
    let bid_ptr = world.module_artifact_bids["bid-map-key"][0]
        .bidder_agent_id
        .as_ptr();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let mut values = Values::default();
    let mut io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 16 << 20,
        write_bytes_remaining: 16 << 20,
    };
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let mut remaining = 6;
    let mut writes = Vec::<ReservedValue<ExecutionWriteV1>>::new();
    let written = prepare_module_marketplace_genesis_v1(
        &world,
        &mut context(&mut values, &memory, &mut io, &mut key_work, || false),
        &mut remaining,
        &mut |write| {
            writes.push(write);
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(written, 3);
    assert_eq!(remaining, 0);
    assert_eq!(writes.len(), 6);
    assert_eq!(
        world.module_visual_entities["visual/雪"].kind.as_ptr(),
        visual_ptr
    );
    assert_eq!(
        world.module_artifact_listings["listing/订单"]
            .seller_agent_id
            .as_ptr(),
        listing_ptr
    );
    assert_eq!(
        world.module_artifact_bids["bid-map-key"][0]
            .bidder_agent_id
            .as_ptr(),
        bid_ptr
    );

    let mut fields = BTreeMap::<&str, Vec<WorldFieldRecordKindV1>>::new();
    let mut original_keys = BTreeMap::<&str, Vec<String>>::new();
    for write in &writes {
        assert_eq!(write.namespace, StateNamespaceV1::Business);
        assert!(write.mutation.before.is_none());
        let parsed = WorldFieldKeyV1::from_bytes(&write.mutation.key).unwrap();
        assert_eq!(parsed.owner, FieldOwner::WorldState);
        assert_eq!(parsed.namespace, StateNamespaceV1::Business);
        assert_eq!(parsed.shape, FieldShape::Map);
        fields.entry(parsed.name).or_default().push(parsed.kind);
        if parsed.kind == WorldFieldRecordKindV1::Entry {
            original_keys.entry(parsed.name).or_default().push(
                parsed
                    .semantic_key
                    .unwrap()
                    .to_vec()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
            );
        }
    }
    assert_eq!(
        fields["module_visual_entities"],
        [
            WorldFieldRecordKindV1::Metadata,
            WorldFieldRecordKindV1::Entry
        ]
    );
    assert_eq!(
        fields["module_artifact_listings"],
        [
            WorldFieldRecordKindV1::Metadata,
            WorldFieldRecordKindV1::Entry
        ]
    );
    assert_eq!(
        fields["module_artifact_bids"],
        [
            WorldFieldRecordKindV1::Metadata,
            WorldFieldRecordKindV1::Entry
        ]
    );
    assert_eq!(original_keys.len(), 3);
    assert!(writes.iter().all(|write| {
        write
            .mutation
            .after
            .is_some_and(|root| values.0.contains_key(&root))
    }));
    assert!(memory.used() > 0);
}

#[test]
fn first_sink_failure_stops_later_marketplace_fields_without_mutating_world() {
    let world = source();
    let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
    let mut values = Values::default();
    let mut io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 8 << 20,
        write_bytes_remaining: 8 << 20,
    };
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 2 << 20,
    };
    let before = world.clone();
    let mut remaining = 6;
    let mut sink_calls = 0usize;
    let result = prepare_module_marketplace_genesis_v1(
        &world,
        &mut context(&mut values, &memory, &mut io, &mut key_work, || false),
        &mut remaining,
        &mut |_| {
            sink_calls += 1;
            Err(ValueTreeError::Invalid("denied first field"))
        },
    );
    assert!(matches!(
        result,
        Err(MemberRecordError::Value(ValueTreeError::Invalid(
            "denied first field"
        )))
    ));
    assert_eq!(sink_calls, 1);
    assert_eq!(world.module_visual_entities, before.module_visual_entities);
    assert_eq!(
        world.module_artifact_listings,
        before.module_artifact_listings
    );
    assert_eq!(world.module_artifact_bids, before.module_artifact_bids);
}
