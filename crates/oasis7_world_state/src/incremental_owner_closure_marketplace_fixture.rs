use super::*;
use crate::{
    incremental_field_member::{FIELD_MEMBER_DOMAIN, FIELD_MEMBER_HEADER_BYTES},
    incremental_semantic_key::{SemanticKeyV1, write_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1, incremental_value::ValueStreamWriter,
};

pub(super) fn marketplace_visual() -> oasis7_agent_api::world_entity::ModuleVisualEntity {
    use oasis7_agent_api::{world_action::GeoPos, world_entity::ModuleVisualAnchor};
    oasis7_agent_api::world_entity::ModuleVisualEntity {
        entity_id: "visual/内部".into(),
        module_id: "module/α".into(),
        kind: "artifact\0mesh".into(),
        label: Some(String::new()),
        anchor: ModuleVisualAnchor::Absolute {
            pos: GeoPos::new(i64::MIN, 0, i64::MAX),
        },
    }
}

pub(super) fn marketplace_listing() -> crate::state::ModuleArtifactListingState {
    crate::state::ModuleArtifactListingState {
        order_id: u64::MAX,
        seller_agent_id: "seller/雪".into(),
        price_kind: oasis7_agent_api::world_action::ResourceKind::Data,
        price_amount: i64::MIN,
        listed_at: u64::MAX,
    }
}

pub(super) fn marketplace_bids() -> Vec<crate::state::ModuleArtifactBidState> {
    [
        crate::state::ModuleArtifactBidState {
            order_id: u64::MAX,
            bidder_agent_id: "bidder/東京".into(),
            price_kind: oasis7_agent_api::world_action::ResourceKind::Electricity,
            price_amount: i64::MIN,
            bid_at: 0,
        },
        crate::state::ModuleArtifactBidState {
            order_id: u64::MAX,
            bidder_agent_id: "bidder/β".into(),
            price_kind: oasis7_agent_api::world_action::ResourceKind::Data,
            price_amount: i64::MAX,
            bid_at: u64::MAX,
        },
    ]
    .into()
}

impl FixtureWriteContext<'_> {
    pub(super) fn add_child_delta(&mut self, delta: StateDeltaV1) {
        for node in &delta.new_nodes {
            let charge = self
                .memory
                .reserve(node.bytes.capacity(), MemoryClass::Ordinary)
                .unwrap();
            self.values
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    charge.retain(node.bytes.clone()),
                )
                .unwrap();
        }
        self.state.add_delta(delta);
    }

    pub(super) fn make_member<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        kind: WorldFieldRecordKindV1,
        key: &K,
        payload: MemberPayloadRefV1<'_>,
    ) -> StateWriteV1 {
        let member = write_member_record_v1(
            field,
            kind,
            key,
            payload,
            &mut MemberRecordWriteContext {
                sink: self.values,
                memory: self.memory,
                io: self.value_io,
                key_work: self.key_work,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 16 << 10,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        let FieldDestination::State(namespace) = field.destination else {
            unreachable!();
        };
        let _ = namespace;
        StateWriteV1 {
            key: member.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(member.value_root),
        }
    }

    pub(super) fn append_member<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        kind: WorldFieldRecordKindV1,
        key: &K,
        payload: MemberPayloadRefV1<'_>,
    ) {
        let FieldDestination::State(namespace) = field.destination else {
            unreachable!();
        };
        let write = self.make_member(field, kind, key, payload);
        self.writes.entry(namespace).or_default().push(write);
    }

    pub(super) fn append_child<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        payload: MemberPayloadRefV1<'_>,
        writes: &mut Vec<StateWriteV1>,
    ) {
        writes.push(self.make_member(field, WorldFieldRecordKindV1::Entry, key, payload));
    }

    pub(super) fn finish_marketplace_record<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        owner: crate::incremental_module_marketplace::ModuleMarketplaceFieldV1,
        field: FieldDescriptor,
        outer_key: &K,
        mut child_writes: Vec<StateWriteV1>,
        issue: Option<FixtureIssue>,
    ) {
        child_writes.sort_unstable_by(|left, right| left.key.cmp(&right.key));
        assert!(
            child_writes
                .windows(2)
                .all(|pair| pair[0].key < pair[1].key)
        );
        let _workspace = self.memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        let delta = StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            child_writes,
            self.state,
            16 << 20,
        )
        .unwrap();
        let child = delta.after.clone();
        self.add_child_delta(delta);
        self.finish_marketplace_root(owner, field, outer_key, child, issue);
    }

    pub(super) fn finish_marketplace_bids_record(
        &mut self,
        owner: crate::incremental_module_marketplace::ModuleMarketplaceFieldV1,
        field: FieldDescriptor,
        outer_key: &str,
        bids: &[crate::state::ModuleArtifactBidState],
        mut child_writes: Vec<StateWriteV1>,
        issue: Option<FixtureIssue>,
    ) {
        child_writes.sort_unstable_by(|left, right| left.key.cmp(&right.key));
        assert!(
            child_writes
                .windows(2)
                .all(|pair| pair[0].key < pair[1].key)
        );
        let _workspace = self.memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        let delta = StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            child_writes,
            self.state,
            16 << 20,
        )
        .unwrap();
        let mut child = delta.after.clone();
        self.add_child_delta(delta);

        let mut remaining = u64::MAX;
        let mut cancelled = || false;
        {
            let mut sink = FixtureRecordStore {
                values: self.values,
                state: self.state,
            };
            crate::incremental_module_marketplace::bid_membership::append_groups(
                field,
                &mut child,
                bids,
                &mut MemberRecordWriteContext {
                    sink: &mut sink,
                    memory: self.memory,
                    io: self.value_io,
                    key_work: self.key_work,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 4096,
                        record_bytes: 16 << 20,
                    },
                    cancelled: &mut cancelled,
                },
                &mut remaining,
            )
            .unwrap();
        }
        self.finish_marketplace_root(owner, field, outer_key, child, issue);
    }

    pub(super) fn finish_marketplace_root<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        owner: crate::incremental_module_marketplace::ModuleMarketplaceFieldV1,
        field: FieldDescriptor,
        outer_key: &K,
        mut child: StateRootV1,
        issue: Option<FixtureIssue>,
    ) {
        if issue == Some(FixtureIssue::WrongModuleMarketplaceChildCount) {
            child.leaf_count = child.leaf_count.checked_add(1).unwrap();
        }
        let mut semantic = Vec::new();
        write_semantic_key_v1(outer_key, &mut semantic).unwrap();
        let mut schema = owner.schema();
        if issue == Some(FixtureIssue::InvalidModuleMarketplacePayload) {
            schema[0] ^= 1;
        }
        let reference =
            RecordRootRefV1::new(schema, *blake3::hash(&semantic).as_bytes(), child).unwrap();
        self.append_member(
            field,
            WorldFieldRecordKindV1::Entry,
            outer_key,
            MemberPayloadRefV1::RecordRoot(&reference),
        );
    }

    pub(super) fn append_raw_member(
        &mut self,
        field: FieldDescriptor,
        kind: WorldFieldRecordKindV1,
        key: &str,
        codec: MemberPayloadCodecV1,
        payload: &[u8],
    ) {
        let mut semantic = Vec::new();
        write_semantic_key_v1(key, &mut semantic).unwrap();
        let key_digest = *blake3::hash(&semantic).as_bytes();
        let field_key = WorldFieldKeyV1::new(field, kind, Some(key_digest)).unwrap();
        let total =
            FIELD_MEMBER_HEADER_BYTES + field_key.as_bytes().len() + semantic.len() + payload.len();
        let mut header = [0; FIELD_MEMBER_HEADER_BYTES];
        let domain_len = FIELD_MEMBER_DOMAIN.len();
        header[..domain_len].copy_from_slice(FIELD_MEMBER_DOMAIN);
        header[domain_len..domain_len + 2]
            .copy_from_slice(&(field_key.as_bytes().len() as u16).to_be_bytes());
        header[domain_len + 2] = codec as u8;
        header[domain_len + 3..domain_len + 11]
            .copy_from_slice(&(semantic.len() as u64).to_be_bytes());
        header[domain_len + 11..].copy_from_slice(&(payload.len() as u64).to_be_bytes());

        let mut not_cancelled = || false;
        let mut writer = ValueStreamWriter::new(
            self.values,
            self.memory,
            self.value_io,
            total as u64,
            &mut not_cancelled,
        )
        .unwrap();
        let writing = (|| {
            writer.write_all(&header)?;
            writer.write_all(field_key.as_bytes())?;
            writer.write_all(&semantic)?;
            writer.write_all(payload)
        })();
        let root = writer.finish().unwrap();
        writing.unwrap();
        let FieldDestination::State(namespace) = field.destination else {
            unreachable!();
        };
        self.writes
            .entry(namespace)
            .or_default()
            .push(StateWriteV1 {
                key: field_key.as_bytes().to_vec(),
                before: None,
                after: Some(root.digest().unwrap()),
            });
    }
}
