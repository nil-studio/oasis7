use super::*;
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberRecordInfoV1, MemberRecordLimitsV1, MemberRecordWriteContext,
        SemanticKeyBudget, WorldFieldKeyV1, WorldFieldRecordKindV1, read_member_record_v1,
    },
    incremental_semantic_key::{SemanticKeyV1, write_semantic_key_v1},
    state::{ModuleArtifactBidState, ModuleArtifactListingState, WorldState},
};
use oasis7_agent_api::{
    world_action::{GeoPos, ResourceKind},
    world_entity::{ModuleVisualAnchor, ModuleVisualEntity},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{
        ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError,
    },
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{collections::BTreeMap, sync::Arc};

struct Values {
    memory: MemoryBudget,
    pages: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
}

impl Values {
    fn new(memory: MemoryBudget) -> Self {
        Self {
            memory,
            pages: BTreeMap::new(),
        }
    }
}

impl ValueNodeWriter for Values {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if blake3::hash(bytes.as_slice()).as_bytes() != &id {
            return Err(ValueTreeError::Invalid("marketplace test value hash"));
        }
        self.pages.insert(id, Arc::new(bytes));
        Ok(())
    }
}

impl ValueNodeReader for Values {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let bytes = self
            .pages
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("marketplace test missing page"))?;
        if class != MemoryClass::Ordinary || !bytes.uses_budget(memory) || bytes.len() > max_bytes {
            return Err(ValueTreeError::Invalid("marketplace test pool/class/bound"));
        }
        Ok(bytes)
    }
}

impl StatePageWriter for Values {}

struct StatePage(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for StatePage {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl MerkleNodeReader for Values {
    type Node = StatePage;

    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }

    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }

    fn read_node(&self, id: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        if id.len() != 64 {
            return Err("marketplace test state digest length".into());
        }
        let mut digest = [0; 32];
        for (index, pair) in id.as_bytes().chunks_exact(2).enumerate() {
            let hex = |byte| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            };
            let (Some(high), Some(low)) = (hex(pair[0]), hex(pair[1])) else {
                return Err("marketplace test state digest encoding".into());
            };
            digest[index] = (high << 4) | low;
        }
        let Some(page) = self.pages.get(&digest).cloned() else {
            return Ok(None);
        };
        if page.len() > max_bytes {
            return Err("marketplace test state page bound".into());
        }
        Ok(Some(StatePage(page)))
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
                order_id: u64::MAX,
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

fn read_member(
    root: &[u8; 32],
    field_key: &WorldFieldKeyV1,
    values: &Values,
    memory: &MemoryBudget,
) -> (MemberRecordInfoV1, Vec<u8>, Vec<u8>) {
    let mut io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 16 << 20,
        write_bytes_remaining: 0,
    };
    let mut context = ValueReadContext {
        source: values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut io,
    };
    let mut semantic = Vec::new();
    let mut payload = Vec::new();
    let info = read_member_record_v1(
        root,
        field_key,
        &mut context,
        MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        &mut semantic,
        &mut payload,
        || false,
    )
    .unwrap();
    (info, semantic, payload)
}

fn child_payload<K: SemanticKeyV1 + ?Sized>(
    root: &StateRootV1,
    descriptor: super::FieldDescriptor,
    key: &K,
    values: &Values,
    memory: &MemoryBudget,
) -> (MemberPayloadCodecV1, Vec<u8>) {
    let mut semantic = Vec::new();
    write_semantic_key_v1(key, &mut semantic).unwrap();
    let key_digest = *blake3::hash(&semantic).as_bytes();
    let field_key =
        WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Entry, Some(key_digest)).unwrap();
    let value_root = root.get(field_key.as_bytes(), values).unwrap().unwrap();
    let (info, actual_semantic, payload) = read_member(&value_root, &field_key, values, memory);
    assert_eq!(actual_semantic, semantic);
    (info.codec, payload)
}

fn outer_child(
    owner: ModuleMarketplaceFieldV1,
    outer_key: &str,
    writes: &[ReservedValue<ExecutionWriteV1>],
    values: &Values,
    memory: &MemoryBudget,
) -> StateRootV1 {
    let descriptor = owner.descriptor().unwrap();
    let mut semantic = Vec::new();
    write_semantic_key_v1(&outer_key, &mut semantic).unwrap();
    let semantic_digest = *blake3::hash(&semantic).as_bytes();
    let write = writes
        .iter()
        .find(|write| {
            let key = WorldFieldKeyV1::from_bytes(&write.mutation.key).unwrap();
            key.name == owner.name()
                && key.kind == WorldFieldRecordKindV1::Entry
                && key.semantic_key == Some(semantic_digest)
        })
        .expect("outer record write");
    let key = WorldFieldKeyV1::from_bytes(&write.mutation.key).unwrap();
    let root = write.mutation.after.unwrap();
    let (info, actual_semantic, payload) = read_member(&root, &key, values, memory);
    assert_eq!(info.codec, MemberPayloadCodecV1::RecordRoot);
    assert_eq!(actual_semantic, semantic);
    let reference =
        oasis7_formal_protocol::incremental_record_root::RecordRootRefV1::from_canonical_bytes(
            &payload,
        )
        .unwrap();
    assert_eq!(reference.schema, owner.schema());
    reference
        .validate_binding(
            &owner.schema(),
            &semantic_digest,
            StateNamespaceV1::Business,
        )
        .unwrap()
        .clone()
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
    let mut values = Values::new(memory.clone());
    let mut io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 16 << 20,
        write_bytes_remaining: 16 << 20,
    };
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let mut remaining = 1000;
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
    assert!(remaining < 1000);
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
            .is_some_and(|root| values.pages.contains_key(&root))
    }));

    let visual_root = outer_child(
        ModuleMarketplaceFieldV1::VisualEntities,
        "visual/雪",
        &writes,
        &values,
        &memory,
    );
    let visual_field = ModuleMarketplaceFieldV1::VisualEntities
        .descriptor()
        .unwrap();
    for (key, codec, expected) in [
        (
            "entity_id",
            MemberPayloadCodecV1::Text,
            "entity/原文".as_bytes(),
        ),
        (
            "module_id",
            MemberPayloadCodecV1::Text,
            "module/α".as_bytes(),
        ),
        (
            "kind",
            MemberPayloadCodecV1::Text,
            b"artifact\0mesh".as_slice(),
        ),
        ("label_text", MemberPayloadCodecV1::Text, b"".as_slice()),
    ] {
        assert_eq!(
            child_payload(&visual_root, visual_field, key, &values, &memory),
            (codec, expected.to_vec())
        );
    }
    assert_eq!(
        child_payload(
            &visual_root,
            visual_field,
            "label_present",
            &values,
            &memory
        ),
        (MemberPayloadCodecV1::Bool, vec![1])
    );
    assert_eq!(
        child_payload(&visual_root, visual_field, "anchor_kind", &values, &memory),
        (MemberPayloadCodecV1::U64, 2u64.to_be_bytes().to_vec())
    );
    for (key, value) in [
        ("anchor_x_cm", i64::MIN),
        ("anchor_y_cm", 0),
        ("anchor_z_cm", i64::MAX),
    ] {
        assert_eq!(
            child_payload(&visual_root, visual_field, key, &values, &memory),
            (MemberPayloadCodecV1::I64, value.to_be_bytes().to_vec())
        );
    }

    let listing_root = outer_child(
        ModuleMarketplaceFieldV1::ArtifactListings,
        "listing/订单",
        &writes,
        &values,
        &memory,
    );
    let listing_field = ModuleMarketplaceFieldV1::ArtifactListings
        .descriptor()
        .unwrap();
    assert_eq!(
        child_payload(&listing_root, listing_field, "order_id", &values, &memory),
        (MemberPayloadCodecV1::U64, u64::MAX.to_be_bytes().to_vec())
    );
    assert_eq!(
        child_payload(
            &listing_root,
            listing_field,
            "seller_agent_id",
            &values,
            &memory
        ),
        (MemberPayloadCodecV1::Text, "seller/λ".as_bytes().to_vec())
    );
    assert_eq!(
        child_payload(&listing_root, listing_field, "price_kind", &values, &memory),
        (MemberPayloadCodecV1::U64, 1u64.to_be_bytes().to_vec())
    );
    assert_eq!(
        child_payload(
            &listing_root,
            listing_field,
            "price_amount",
            &values,
            &memory
        ),
        (MemberPayloadCodecV1::I64, i64::MIN.to_be_bytes().to_vec())
    );
    assert_eq!(
        child_payload(&listing_root, listing_field, "listed_at", &values, &memory),
        (MemberPayloadCodecV1::U64, u64::MAX.to_be_bytes().to_vec())
    );

    let bids_root = outer_child(
        ModuleMarketplaceFieldV1::ArtifactBids,
        "bid-map-key",
        &writes,
        &values,
        &memory,
    );
    let bids_field = ModuleMarketplaceFieldV1::ArtifactBids.descriptor().unwrap();
    assert_eq!(
        child_payload(&bids_root, bids_field, "length", &values, &memory),
        (MemberPayloadCodecV1::U64, 2u64.to_be_bytes().to_vec())
    );
    assert_eq!(
        child_payload(&bids_root, bids_field, "next_bid_id", &values, &memory),
        (MemberPayloadCodecV1::U64, 2u64.to_be_bytes().to_vec())
    );
    assert_eq!(
        child_payload(&bids_root, bids_field, "sequence_root_id", &values, &memory),
        (MemberPayloadCodecV1::U64, 1u64.to_be_bytes().to_vec())
    );
    for (ordinal, bidder, amount, at, kind) in [
        (0u64, "bidder/東京", i64::MIN, 0u64, 0u64),
        (1u64, "bidder/β", i64::MAX, u64::MAX, 1u64),
    ] {
        let key = |field: &'static str| (("bid", ordinal), field);
        let index = if ordinal == 0 {
            [u64::MAX, u64::MAX, 0, 0, 0, 0]
        } else {
            [0, u64::MAX, 1, 1, 0, 0]
        };
        for (name, value) in [
            ("index_left_id", index[0]),
            ("index_right_id", index[1]),
            ("index_left_size", index[2]),
            ("index_left_height", index[3]),
            ("index_right_size", index[4]),
            ("index_right_height", index[5]),
        ] {
            assert_eq!(
                child_payload(&bids_root, bids_field, &key(name), &values, &memory),
                (MemberPayloadCodecV1::U64, value.to_be_bytes().to_vec())
            );
        }
        assert_eq!(
            child_payload(&bids_root, bids_field, &key("present"), &values, &memory),
            (MemberPayloadCodecV1::Unit, Vec::new())
        );
        assert_eq!(
            child_payload(&bids_root, bids_field, &key("order_id"), &values, &memory),
            (MemberPayloadCodecV1::U64, u64::MAX.to_be_bytes().to_vec())
        );
        assert_eq!(
            child_payload(
                &bids_root,
                bids_field,
                &key("bidder_agent_id"),
                &values,
                &memory
            ),
            (MemberPayloadCodecV1::Text, bidder.as_bytes().to_vec())
        );
        assert_eq!(
            child_payload(&bids_root, bids_field, &key("price_kind"), &values, &memory),
            (MemberPayloadCodecV1::U64, kind.to_be_bytes().to_vec())
        );
        assert_eq!(
            child_payload(
                &bids_root,
                bids_field,
                &key("price_amount"),
                &values,
                &memory
            ),
            (MemberPayloadCodecV1::I64, amount.to_be_bytes().to_vec())
        );
        assert_eq!(
            child_payload(&bids_root, bids_field, &key("bid_at"), &values, &memory),
            (MemberPayloadCodecV1::U64, at.to_be_bytes().to_vec())
        );
    }
    assert!(memory.used() > 0);
}

#[test]
fn first_sink_failure_stops_later_marketplace_fields_without_mutating_world() {
    let world = source();
    let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
    let mut values = Values::new(memory.clone());
    let mut io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 8 << 20,
        write_bytes_remaining: 8 << 20,
    };
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 2 << 20,
    };
    let before = world.clone();
    let mut remaining = 1000;
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
