use super::*;
use crate::{
    incremental_field_member::{
        FIELD_MEMBER_DOMAIN, FIELD_MEMBER_HEADER_BYTES, MemberPayloadCodecV1, MemberPayloadRefV1,
        MemberRecordLimitsV1, MemberRecordWriteContext, write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_projection::{FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS},
    incremental_semantic_key::{SemanticKeyBudget, SemanticKeyV1, write_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{
        MerkleNodeReader, StateDeltaV1, StateNamespaceV1, StateRootV1, StateScanBudget,
        StateWriteV1,
    },
    incremental_value::{
        ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueStreamWriter, ValueTreeError,
    },
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::Arc,
};

struct StatePage(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for StatePage {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

struct StatePages {
    memory: MemoryBudget,
    read_class: Cell<MemoryClass>,
    pages: RefCell<BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>>,
}
impl StatePages {
    fn new(memory: MemoryBudget) -> Self {
        Self {
            memory,
            read_class: Cell::new(MemoryClass::Ordinary),
            pages: RefCell::new(BTreeMap::new()),
        }
    }

    fn set_read_class(&self, class: MemoryClass) {
        self.read_class.set(class);
    }

    fn add_delta(&self, delta: StateDeltaV1) {
        for node in delta.new_nodes {
            let charge = self
                .memory
                .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                .unwrap();
            self.pages
                .borrow_mut()
                .insert(node.digest, Arc::new(charge.retain(node.bytes)));
        }
    }
}
impl MerkleNodeReader for StatePages {
    type Node = StatePage;

    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }

    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.read_class.get())
    }

    fn read_node(&self, id: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        let page = self.pages.borrow().get(id).cloned();
        if page.as_ref().is_some_and(|page| page.len() > max_bytes) {
            return Err("owner test state page bound".into());
        }
        Ok(page.map(StatePage))
    }
}

type ChargedValuePages = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;

#[derive(Default)]
struct ValuePages {
    pages: RefCell<ChargedValuePages>,
}
impl ValueNodeWriter for ValuePages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if blake3::hash(bytes.as_slice()).as_bytes() != &id {
            return Err(ValueTreeError::Invalid("owner test value hash"));
        }
        self.pages.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for ValuePages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let page = self
            .pages
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("owner test missing value page"))?;
        if !page.uses_budget(memory) || page.len() > max_bytes {
            return Err(ValueTreeError::Invalid("owner test value pool/class/bound"));
        }
        Ok(page)
    }
}

struct Fixture {
    memory: MemoryBudget,
    state: StatePages,
    values: ValuePages,
    roots: [StateRootV1; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureIssue {
    WrongTextKey,
    InvalidTextPayload,
    WrongTextCodec,
    WrongBytesCodec,
    MissingTextMetadata,
    MissingModuleMarketplaceMetadata,
    WrongModuleMarketplaceKey,
    WrongModuleMarketplaceCodec,
    InvalidModuleMarketplacePayload,
    MissingModuleMarketplaceChild,
    UnknownModuleMarketplaceChild,
    WrongModuleMarketplaceChildCount,
    EmptyModuleMarketplaceMaps,
    InvalidModuleBidParentLink,
    SharedModuleBidChild,
    ModuleBidChildCycle,
    MissingInstalledTargetMetadata,
    WrongInstalledTargetKey,
    WrongInstalledTargetCodec,
    MissingWorldBalanceMetadata,
    WrongWorldBalanceKey,
    WrongWorldBalanceCodec,
    EmptyTargetAndBalanceMaps,
    MissingModuleInstanceMetadata,
    MissingModuleInstanceSibling,
    WrongModuleInstanceSemanticKey,
    WrongModuleInstanceCodec,
    EmptyModuleInstances,
}

struct FixtureWriteContext<'a> {
    values: &'a mut ValuePages,
    memory: &'a MemoryBudget,
    state: &'a StatePages,
    value_io: &'a mut ValueIoBudget,
    key_work: &'a mut SemanticKeyBudget,
    writes: &'a mut BTreeMap<StateNamespaceV1, Vec<StateWriteV1>>,
}

fn marketplace_visual() -> oasis7_agent_api::world_entity::ModuleVisualEntity {
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

fn marketplace_listing() -> crate::state::ModuleArtifactListingState {
    crate::state::ModuleArtifactListingState {
        order_id: u64::MAX,
        seller_agent_id: "seller/雪".into(),
        price_kind: oasis7_agent_api::world_action::ResourceKind::Data,
        price_amount: i64::MIN,
        listed_at: u64::MAX,
    }
}

fn marketplace_bids() -> Vec<crate::state::ModuleArtifactBidState> {
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
    fn make_member<K: SemanticKeyV1 + ?Sized>(
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

    fn append_member<K: SemanticKeyV1 + ?Sized>(
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

    fn append_child<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        payload: MemberPayloadRefV1<'_>,
        writes: &mut Vec<StateWriteV1>,
    ) {
        writes.push(self.make_member(field, WorldFieldRecordKindV1::Entry, key, payload));
    }

    fn finish_marketplace_record<K: SemanticKeyV1 + ?Sized>(
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
        let mut child = delta.after.clone();
        self.state.add_delta(delta);
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

    fn append_raw_member(
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

fn fixture(include_u64_entries: bool) -> Fixture {
    fixture_with_issue(include_u64_entries, None)
}

fn fixture_with_issue(include_u64_entries: bool, issue: Option<FixtureIssue>) -> Fixture {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let state = StatePages::new(memory.clone());
    let mut values = ValuePages::default();
    let mut value_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    };
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut writes = BTreeMap::<StateNamespaceV1, Vec<StateWriteV1>>::new();
    macro_rules! fixture_writer {
        () => {
            FixtureWriteContext {
                values: &mut values,
                memory: &memory,
                state: &state,
                value_io: &mut value_io,
                key_work: &mut key_work,
                writes: &mut writes,
            }
        };
    }
    for field in WORLD_STATE_FIELDS.iter().copied() {
        let FieldDestination::State(namespace) = field.destination else {
            continue;
        };
        let omit_metadata = (issue == Some(FixtureIssue::MissingTextMetadata)
            && field.name == "node_identity_bindings")
            || (issue == Some(FixtureIssue::MissingModuleMarketplaceMetadata)
                && field.name == "module_visual_entities")
            || (issue == Some(FixtureIssue::MissingInstalledTargetMetadata)
                && field.name == "installed_module_targets")
            || (issue == Some(FixtureIssue::MissingWorldBalanceMetadata)
                && field.name == "main_token_balances");
        if !omit_metadata {
            let metadata = write_field_metadata_v1(
                field,
                &mut PresenceRecordWriteContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut value_io,
                    key_work: &mut key_work,
                    limits: PresenceRecordLimitsV1 {
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            writes.entry(namespace).or_default().push(StateWriteV1 {
                key: metadata.field_key.as_bytes().to_vec(),
                before: None,
                after: Some(metadata.value_root),
            });
        }

        if include_u64_entries {
            if let Some(scalar) = u64_field(field) {
                let entry = write_member_record_v1(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    &(),
                    MemberPayloadRefV1::U64(scalar.value(&crate::state::WorldState::default())),
                    &mut MemberRecordWriteContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut value_io,
                        key_work: &mut key_work,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: U64_SEMANTIC_BYTES,
                            payload_bytes: 8,
                            record_bytes: 4096,
                        },
                        cancelled: || false,
                    },
                )
                .unwrap();
                writes.entry(namespace).or_default().push(StateWriteV1 {
                    key: entry.field_key.as_bytes().to_vec(),
                    before: None,
                    after: Some(entry.value_root),
                });
            }

            if let Some(set) = id_set_field(field) {
                let entry = match id_set_key_kind(set) {
                    OwnerKeyKind::Text => write_member_record_v1(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        "typed-owner-set-key",
                        MemberPayloadRefV1::Unit,
                        &mut MemberRecordWriteContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut value_io,
                            key_work: &mut key_work,
                            limits: MemberRecordLimitsV1 {
                                semantic_bytes: 1024,
                                payload_bytes: 8,
                                record_bytes: 4096,
                            },
                            cancelled: || false,
                        },
                    ),
                    OwnerKeyKind::Action => write_member_record_v1(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        &42u64,
                        MemberPayloadRefV1::Unit,
                        &mut MemberRecordWriteContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut value_io,
                            key_work: &mut key_work,
                            limits: MemberRecordLimitsV1 {
                                semantic_bytes: 1024,
                                payload_bytes: 8,
                                record_bytes: 4096,
                            },
                            cancelled: || false,
                        },
                    ),
                }
                .unwrap();
                writes.entry(namespace).or_default().push(StateWriteV1 {
                    key: entry.field_key.as_bytes().to_vec(),
                    before: None,
                    after: Some(entry.value_root),
                });
            }

            if let Some(map) = number_map_field(field) {
                let (key_kind, codec) = number_map_profile(map);
                let payload = match codec {
                    MemberPayloadCodecV1::U64 => MemberPayloadRefV1::U64(123),
                    MemberPayloadCodecV1::I64 => MemberPayloadRefV1::I64(-123),
                    _ => unreachable!(),
                };
                let entry = match key_kind {
                    OwnerKeyKind::Text => write_member_record_v1(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        "typed-owner-map-key",
                        payload,
                        &mut MemberRecordWriteContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut value_io,
                            key_work: &mut key_work,
                            limits: MemberRecordLimitsV1 {
                                semantic_bytes: 1024,
                                payload_bytes: 8,
                                record_bytes: 4096,
                            },
                            cancelled: || false,
                        },
                    ),
                    OwnerKeyKind::Action => write_member_record_v1(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        &43u64,
                        payload,
                        &mut MemberRecordWriteContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut value_io,
                            key_work: &mut key_work,
                            limits: MemberRecordLimitsV1 {
                                semantic_bytes: 1024,
                                payload_bytes: 8,
                                record_bytes: 4096,
                            },
                            cancelled: || false,
                        },
                    ),
                }
                .unwrap();
                writes.entry(namespace).or_default().push(StateWriteV1 {
                    key: entry.field_key.as_bytes().to_vec(),
                    before: None,
                    after: Some(entry.value_root),
                });
            }

            if field.name == "installed_module_targets"
                && issue != Some(FixtureIssue::EmptyTargetAndBalanceMaps)
            {
                match issue {
                    Some(FixtureIssue::WrongInstalledTargetKey) => fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        &43u64,
                        MemberPayloadRefV1::Unit,
                    ),
                    Some(FixtureIssue::WrongInstalledTargetCodec) => fixture_writer!()
                        .append_member(
                            field,
                            WorldFieldRecordKindV1::Entry,
                            "target-key",
                            MemberPayloadRefV1::U64(1),
                        ),
                    _ => fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        "target-key",
                        MemberPayloadRefV1::Text("infra/東京"),
                    ),
                }
            }

            if matches!(field.name, "main_token_balances" | "node_asset_balances")
                && issue != Some(FixtureIssue::EmptyTargetAndBalanceMaps)
            {
                let main = field.name == "main_token_balances";
                let identity = if main {
                    "inner-account/雪"
                } else {
                    "inner-node/é"
                };
                if issue == Some(FixtureIssue::WrongWorldBalanceKey) && main {
                    fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        &43u64,
                        MemberPayloadRefV1::IdentityU64Triple {
                            identity,
                            amounts: [0, u64::MAX, 17],
                        },
                    );
                } else if issue == Some(FixtureIssue::WrongWorldBalanceCodec) && main {
                    fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        "main-map-key",
                        MemberPayloadRefV1::U64(7),
                    );
                } else {
                    fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        if main { "main-map-key" } else { "node-map-key" },
                        MemberPayloadRefV1::IdentityU64Triple {
                            identity,
                            amounts: [0, u64::MAX, 17],
                        },
                    );
                }
            }
        }

        if let Some(owner) = text_byte_field(field) {
            if owner == TextByteOwnerField::ModuleStates {
                if issue == Some(FixtureIssue::WrongBytesCodec) {
                    fixture_writer!().append_raw_member(
                        field,
                        WorldFieldRecordKindV1::Bytes,
                        "module/α",
                        MemberPayloadCodecV1::Text,
                        b"a byte map cannot use Text codec",
                    );
                } else {
                    fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Bytes,
                        "module/α",
                        MemberPayloadRefV1::Bytes(&[0x00, 0xff, 0x80, 0x41]),
                    );
                }
            } else {
                let (key, value) = match owner {
                    TextByteOwnerField::ModuleArtifactOwners => ("artifact/α\0", "owner/λ\0"),
                    TextByteOwnerField::NodeIdentityBindings => ("node/東京", "identity/é"),
                    TextByteOwnerField::NodeMainTokenAccountBindings => ("node/β", "account/🪙"),
                    TextByteOwnerField::ModuleStates => unreachable!(),
                };
                match issue.filter(|_| owner == TextByteOwnerField::ModuleArtifactOwners) {
                    Some(FixtureIssue::WrongTextKey) => fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        &17u64,
                        MemberPayloadRefV1::Text(value),
                    ),
                    Some(FixtureIssue::InvalidTextPayload) => fixture_writer!().append_raw_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        key,
                        MemberPayloadCodecV1::Text,
                        b"\xff",
                    ),
                    Some(FixtureIssue::WrongTextCodec) => fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        key,
                        MemberPayloadRefV1::U64(0),
                    ),
                    _ => fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        key,
                        MemberPayloadRefV1::Text(value),
                    ),
                }
            }
        }

        if let Some(owner) = super::module_marketplace::field_from_descriptor(field)
            && issue != Some(FixtureIssue::EmptyModuleMarketplaceMaps)
        {
            use crate::incremental_module_marketplace::ModuleMarketplaceFieldV1 as Market;

            match owner {
                Market::VisualEntities => {
                    let visual = marketplace_visual();
                    let mut children = Vec::new();
                    {
                        let mut writer = fixture_writer!();
                        writer.append_child(
                            field,
                            "entity_id",
                            MemberPayloadRefV1::Text(&visual.entity_id),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "module_id",
                            MemberPayloadRefV1::Text(&visual.module_id),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "kind",
                            MemberPayloadRefV1::Text(&visual.kind),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "label_present",
                            MemberPayloadRefV1::Bool(visual.label.is_some()),
                            &mut children,
                        );
                        if issue != Some(FixtureIssue::MissingModuleMarketplaceChild) {
                            writer.append_child(
                                field,
                                "label_text",
                                MemberPayloadRefV1::Text(visual.label.as_deref().unwrap()),
                                &mut children,
                            );
                        }
                        let anchor_kind = match &visual.anchor {
                            oasis7_agent_api::world_entity::ModuleVisualAnchor::Agent {
                                agent_id,
                            } => {
                                writer.append_child(
                                    field,
                                    "anchor_agent_id",
                                    MemberPayloadRefV1::Text(agent_id),
                                    &mut children,
                                );
                                0
                            }
                            oasis7_agent_api::world_entity::ModuleVisualAnchor::Location {
                                location_id,
                            } => {
                                writer.append_child(
                                    field,
                                    "anchor_location_id",
                                    MemberPayloadRefV1::Text(location_id),
                                    &mut children,
                                );
                                1
                            }
                            oasis7_agent_api::world_entity::ModuleVisualAnchor::Absolute {
                                pos,
                            } => {
                                writer.append_child(
                                    field,
                                    "anchor_x_cm",
                                    MemberPayloadRefV1::I64(pos.x_cm),
                                    &mut children,
                                );
                                writer.append_child(
                                    field,
                                    "anchor_y_cm",
                                    MemberPayloadRefV1::I64(pos.y_cm),
                                    &mut children,
                                );
                                writer.append_child(
                                    field,
                                    "anchor_z_cm",
                                    MemberPayloadRefV1::I64(pos.z_cm),
                                    &mut children,
                                );
                                2
                            }
                        };
                        writer.append_child(
                            field,
                            "anchor_kind",
                            MemberPayloadRefV1::U64(anchor_kind),
                            &mut children,
                        );
                        if issue == Some(FixtureIssue::UnknownModuleMarketplaceChild) {
                            writer.append_child(
                                field,
                                "unregistered_child",
                                MemberPayloadRefV1::Unit,
                                &mut children,
                            );
                        }
                        if issue == Some(FixtureIssue::WrongModuleMarketplaceKey) {
                            writer.finish_marketplace_record(owner, field, &17u64, children, None);
                        } else {
                            writer.finish_marketplace_record(
                                owner,
                                field,
                                "visual/雪",
                                children,
                                issue,
                            );
                        }
                    }
                }
                Market::ArtifactListings
                    if issue == Some(FixtureIssue::WrongModuleMarketplaceCodec) =>
                {
                    fixture_writer!().append_member(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        "listing/α",
                        MemberPayloadRefV1::Text("wrong codec"),
                    );
                }
                Market::ArtifactListings => {
                    let listing = marketplace_listing();
                    let mut children = Vec::new();
                    {
                        let mut writer = fixture_writer!();
                        writer.append_child(
                            field,
                            "order_id",
                            MemberPayloadRefV1::U64(listing.order_id),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "seller_agent_id",
                            MemberPayloadRefV1::Text(&listing.seller_agent_id),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "price_kind",
                            MemberPayloadRefV1::U64(match listing.price_kind {
                                oasis7_agent_api::world_action::ResourceKind::Electricity => 0,
                                oasis7_agent_api::world_action::ResourceKind::Data => 1,
                            }),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "price_amount",
                            MemberPayloadRefV1::I64(listing.price_amount),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "listed_at",
                            MemberPayloadRefV1::U64(listing.listed_at),
                            &mut children,
                        );
                        writer.finish_marketplace_record(
                            owner,
                            field,
                            "listing/α",
                            children,
                            issue,
                        );
                    }
                }
                Market::ArtifactBids => {
                    let bids = marketplace_bids();
                    let mut children = Vec::new();
                    {
                        let mut writer = fixture_writer!();
                        writer.append_child(
                            field,
                            "length",
                            MemberPayloadRefV1::U64(bids.len() as u64),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "next_bid_id",
                            MemberPayloadRefV1::U64(bids.len() as u64),
                            &mut children,
                        );
                        writer.append_child(
                            field,
                            "sequence_root_id",
                            MemberPayloadRefV1::U64(1),
                            &mut children,
                        );
                        for (ordinal, bid) in bids.iter().enumerate() {
                            let ordinal = ordinal as u64;
                            let bid_id = ordinal;
                            let bid_key = ("bid", bid_id);
                            let mut index_fields = if ordinal == 0 {
                                [u64::MAX, u64::MAX, 0, 0, 0, 0, 1]
                            } else {
                                [0, u64::MAX, 1, 1, 0, 0, u64::MAX]
                            };
                            if issue == Some(FixtureIssue::InvalidModuleBidParentLink)
                                && ordinal == 0
                            {
                                index_fields[6] = u64::MAX;
                            }
                            if issue == Some(FixtureIssue::SharedModuleBidChild) && ordinal == 1 {
                                index_fields = [0, 0, 1, 1, 1, 1, u64::MAX];
                            }
                            if issue == Some(FixtureIssue::ModuleBidChildCycle) {
                                index_fields = if ordinal == 0 {
                                    [u64::MAX, 1, 0, 0, 2, 2, 1]
                                } else {
                                    [0, u64::MAX, 1, 1, 0, 0, 0]
                                };
                            }
                            writer.append_child(
                                field,
                                &(bid_key, "index_node"),
                                MemberPayloadRefV1::FixedU64x7(index_fields),
                                &mut children,
                            );
                            writer.append_child(
                                field,
                                &(bid_key, "present"),
                                MemberPayloadRefV1::Unit,
                                &mut children,
                            );
                            writer.append_child(
                                field,
                                &(bid_key, "order_id"),
                                MemberPayloadRefV1::U64(bid.order_id),
                                &mut children,
                            );
                            writer.append_child(
                                field,
                                &(bid_key, "bidder_agent_id"),
                                MemberPayloadRefV1::Text(&bid.bidder_agent_id),
                                &mut children,
                            );
                            writer.append_child(
                                field,
                                &(bid_key, "price_kind"),
                                MemberPayloadRefV1::U64(match bid.price_kind {
                                    oasis7_agent_api::world_action::ResourceKind::Electricity => 0,
                                    oasis7_agent_api::world_action::ResourceKind::Data => 1,
                                }),
                                &mut children,
                            );
                            writer.append_child(
                                field,
                                &(bid_key, "price_amount"),
                                MemberPayloadRefV1::I64(bid.price_amount),
                                &mut children,
                            );
                            writer.append_child(
                                field,
                                &(bid_key, "bid_at"),
                                MemberPayloadRefV1::U64(bid.bid_at),
                                &mut children,
                            );
                        }
                        writer.finish_marketplace_record(owner, field, "bid-key", children, issue);
                    }
                }
            }
        }
    }

    let instance = crate::ModuleInstanceState {
        instance_id: "inner-id-does-not-replace-map-key".into(),
        module_id: "module/α".into(),
        module_version: "v2\0β".into(),
        wasm_hash: "sha256:original".into(),
        owner_agent_id: "owner/雪".into(),
        install_target:
            oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
                location_id: "  site/東京  ".into(),
            },
        active: true,
        installed_at: u64::MAX,
    };
    for field in crate::incremental_module_instance_fields::ModuleInstanceFieldV1::ALL {
        let descriptor = FieldDescriptor {
            owner: FieldOwner::WorldState,
            name: field.name(),
            destination: FieldDestination::State(StateNamespaceV1::Business),
            shape: FieldShape::Map,
        };
        if issue != Some(FixtureIssue::MissingModuleInstanceMetadata)
            || field != crate::incremental_module_instance_fields::ModuleInstanceFieldV1::Owner
        {
            let metadata = write_field_metadata_v1(
                descriptor,
                &mut PresenceRecordWriteContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut value_io,
                    key_work: &mut key_work,
                    limits: PresenceRecordLimitsV1 {
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            writes
                .entry(StateNamespaceV1::Business)
                .or_default()
                .push(StateWriteV1 {
                    key: metadata.field_key.as_bytes().to_vec(),
                    before: None,
                    after: Some(metadata.value_root),
                });
        }
        if issue == Some(FixtureIssue::MissingModuleInstanceSibling)
            && field == crate::incremental_module_instance_fields::ModuleInstanceFieldV1::WasmHash
        {
            continue;
        }
        if issue == Some(FixtureIssue::EmptyModuleInstances) {
            continue;
        }
        crate::incremental_module_instance_fields::visit_module_instance_fields(
            &instance,
            |actual, value| {
                if actual != field {
                    return Ok::<_, ()>(());
                }
                let key = match issue {
                    Some(FixtureIssue::WrongModuleInstanceSemanticKey)
                        if field
                            == crate::incremental_module_instance_fields::ModuleInstanceFieldV1::Active =>
                    {
                        "different-map-key"
                    }
                    _ => "outer-map-key",
                };
                let payload = match (field, value) {
                    (
                        crate::incremental_module_instance_fields::ModuleInstanceFieldV1::Active,
                        crate::incremental_module_instance_fields::ModuleInstanceScalarRefV1::Bool(value),
                    ) if issue == Some(FixtureIssue::WrongModuleInstanceCodec) => {
                        MemberPayloadRefV1::U64(u64::from(value))
                    }
                    (_, crate::incremental_module_instance_fields::ModuleInstanceScalarRefV1::Text(value)) => {
                        MemberPayloadRefV1::Text(value)
                    }
                    (
                        _,
                        crate::incremental_module_instance_fields::ModuleInstanceScalarRefV1::Target(
                            crate::incremental_module_instance_fields::ModuleInstallTargetRefV1::SelfAgent,
                        ),
                    ) => MemberPayloadRefV1::Unit,
                    (
                        _,
                        crate::incremental_module_instance_fields::ModuleInstanceScalarRefV1::Target(
                            crate::incremental_module_instance_fields::ModuleInstallTargetRefV1::LocationInfrastructure(value),
                        ),
                    ) => MemberPayloadRefV1::Text(value),
                    (_, crate::incremental_module_instance_fields::ModuleInstanceScalarRefV1::Bool(value)) => {
                        MemberPayloadRefV1::Bool(value)
                    }
                    (_, crate::incremental_module_instance_fields::ModuleInstanceScalarRefV1::U64(value)) => {
                        MemberPayloadRefV1::U64(value)
                    }
                };
                fixture_writer!().append_member(
                    descriptor,
                    WorldFieldRecordKindV1::Entry,
                    key,
                    payload,
                );
                Ok(())
            },
        )
        .unwrap();
    }

    let namespaces = [
        StateNamespaceV1::Business,
        StateNamespaceV1::Allocator,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Effects,
        StateNamespaceV1::Artifacts,
    ];
    let mut roots = Vec::with_capacity(namespaces.len());
    for namespace in namespaces {
        let mut state_writes = writes.remove(&namespace).unwrap_or_default();
        state_writes.sort_by(|left, right| left.key.cmp(&right.key));
        let delta = StateDeltaV1::build(
            StateRootV1::empty(namespace).unwrap(),
            state_writes,
            &state,
            16 << 20,
        )
        .unwrap();
        roots.push(delta.after.clone());
        state.add_delta(delta);
    }
    assert!(writes.is_empty());
    Fixture {
        memory,
        state,
        values,
        roots: roots.try_into().unwrap(),
    }
}

fn verify(
    fixture: &Fixture,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<UnsupportedWorldStateOwnersV1>, WorldStateOwnerError> {
    verify_with_class(fixture, MemoryClass::Ordinary, cancelled)
}

fn verify_with_class(
    fixture: &Fixture,
    class: MemoryClass,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<UnsupportedWorldStateOwnersV1>, WorldStateOwnerError> {
    let mut value_io = ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    };
    let mut values = ValueReadContext {
        source: &fixture.values,
        memory: &fixture.memory,
        class,
        io: &mut value_io,
    };
    let mut state_io = StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 10_000,
    };
    verify_world_state_owner_coverage(
        &fixture.roots,
        [0x5a; 32],
        &fixture.state,
        &mut values,
        &mut state_io,
        1 << 20,
        cancelled,
    )
}

#[path = "incremental_owner_closure_marketplace_tests.rs"]
mod marketplace_tests;
#[path = "incremental_owner_closure_registry_tests.rs"]
mod registry_tests;
