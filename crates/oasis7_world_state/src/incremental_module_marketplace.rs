//! Borrowed child-record storage for the three WorldState marketplace maps.
//!
//! Each outer String key points to a schema-bound Business StateRoot. Record
//! fields use separate typed leaves so unrelated long text and bids are not
//! rewritten by a scalar update. This module does not authorize marketplace
//! actions or activate a complete World.
use crate::{
    incremental_field_member::{
        MemberPayloadRefV1, MemberReadProfileV1, MemberRecordError, MemberRecordWriteContext,
        WorldFieldKeyV1, read_member_record_profile_v1, write_member_record_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::{
        FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
    },
    incremental_record_tree::{RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{
        CanonicalSemanticKeyRefV1, SemanticKeyV1, hash_semantic_key_v1, write_semantic_key_v1,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
    state::{ModuleArtifactBidState, ModuleArtifactListingState, WorldState},
};
use oasis7_agent_api::{world_action::ResourceKind, world_entity::ModuleVisualAnchor};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionWriteV1, PreparedGuard},
    incremental_record_root::RecordRootRefV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::{self, Write};

#[path = "incremental_module_marketplace_bid_index.rs"]
pub(crate) mod bid_index;

const RECORD_TREE_LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "market record source read",
    pending: "market record source pages",
    tree: "market record tree",
    digest: "market record digest",
    read_allowance: "market record read allowance",
    admission: "market record source admission",
    bound: "market record page bound",
    producer_pool: "market record producer pool",
    page: "market record page",
    write_allowance: "market record write allowance",
};
const MAX_RECORD_TREE_PENDING_BYTES: usize = 16 << 20;
const RECORD_ROOT_BUILD_WORKSPACE: usize = 512;
const MARKETPLACE_KEY_WORKSPACE: usize = 128;

struct SemanticKeyBuffer {
    bytes: [u8; MARKETPLACE_KEY_WORKSPACE],
    length: usize,
}

impl SemanticKeyBuffer {
    fn encode<K: SemanticKeyV1 + ?Sized>(key: &K) -> Result<Self, ValueTreeError> {
        let mut buffer = Self {
            bytes: [0; MARKETPLACE_KEY_WORKSPACE],
            length: 0,
        };
        write_semantic_key_v1(key, &mut buffer)
            .map_err(|_| ValueTreeError::Invalid("marketplace semantic key encoding"))?;
        Ok(buffer)
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

impl Write for SemanticKeyBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(end) = self.length.checked_add(bytes.len()) else {
            return Err(io::Error::other("marketplace semantic key overflow"));
        };
        if end > self.bytes.len() {
            return Err(io::Error::other("marketplace semantic key bound"));
        }
        self.bytes[self.length..end].copy_from_slice(bytes);
        self.length = end;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleMarketplaceFieldV1 {
    VisualEntities,
    ArtifactListings,
    ArtifactBids,
}
impl ModuleMarketplaceFieldV1 {
    pub const ALL: [Self; 3] = [
        Self::VisualEntities,
        Self::ArtifactListings,
        Self::ArtifactBids,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::VisualEntities => "module_visual_entities",
            Self::ArtifactListings => "module_artifact_listings",
            Self::ArtifactBids => "module_artifact_bids",
        }
    }

    pub fn descriptor(self) -> Result<FieldDescriptor, MemberRecordError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.owner == FieldOwner::WorldState
                    && field.name == self.name()
                    && field.shape == FieldShape::Map
                    && field.destination == FieldDestination::State(StateNamespaceV1::Business)
            })
            .ok_or(MemberRecordError::Invalid(
                "module marketplace registration",
            ))
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|field| field.name() == name)
    }

    pub(crate) fn schema(self) -> [u8; 32] {
        let domain: &[u8] = match self {
            Self::VisualEntities => b"oasis7.iws.market.visual-entity-record.v1\0",
            Self::ArtifactListings => b"oasis7.iws.market.listing-record.v1\0",
            Self::ArtifactBids => b"oasis7.iws.market.bids-record.v1\0",
        };
        *blake3::hash(domain).as_bytes()
    }
}

fn record_start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "module marketplace records",
        ))?;
    Ok(())
}

fn tree_policy(record_bytes: u64) -> RecordTreeWritePolicyV1 {
    RecordTreeWritePolicyV1 {
        max_pending_node_bytes: usize::try_from(record_bytes)
            .unwrap_or(usize::MAX)
            .min(MAX_RECORD_TREE_PENDING_BYTES),
        labels: RECORD_TREE_LABELS,
    }
}

fn map_semantic_error(error: crate::incremental_semantic_key::SemanticKeyError) -> ValueTreeError {
    match error {
        crate::incremental_semantic_key::SemanticKeyError::Memory(error) => {
            ValueTreeError::Memory(error)
        }
        crate::incremental_semantic_key::SemanticKeyError::ResourceLimited => {
            ValueTreeError::ResourceLimited("marketplace semantic key")
        }
        crate::incremental_semantic_key::SemanticKeyError::Cancelled => ValueTreeError::Cancelled,
        crate::incremental_semantic_key::SemanticKeyError::Invalid(reason) => {
            ValueTreeError::Invalid(reason)
        }
    }
}

fn append_member<
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    descriptor: FieldDescriptor,
    root: &mut StateRootV1,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<(), MemberRecordError> {
    record_start(context, remaining)?;
    let _root_workspace = context
        .memory
        .reserve(RECORD_ROOT_BUILD_WORKSPACE, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let policy = tree_policy(context.limits.record_bytes);
    let member = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        key,
        payload,
        context,
    )?;
    *root = crate::incremental_record_tree::append(
        root.clone(),
        &member.field_key,
        member.value_root,
        context,
        policy,
    )?;
    Ok(())
}

fn write_outer_root<
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    field: ModuleMarketplaceFieldV1,
    descriptor: FieldDescriptor,
    outer_key: &K,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    sink: &mut impl FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
) -> Result<(), MemberRecordError> {
    record_start(context, remaining)?;
    let _root_workspace = context
        .memory
        .reserve(RECORD_ROOT_BUILD_WORKSPACE, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let owner = hash_semantic_key_v1(
        outer_key,
        context.memory,
        MemoryClass::Ordinary,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(map_semantic_error)?;
    let reference = RecordRootRefV1::new(field.schema(), owner.digest, child)
        .map_err(|_| MemberRecordError::Invalid("module marketplace record root"))?;
    let record = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        outer_key,
        MemberPayloadRefV1::RecordRoot(&reference),
        context,
    )?;
    crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)?;
    Ok(())
}

fn write_metadata<W, C, S>(
    descriptor: FieldDescriptor,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError>
where
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
{
    record_start(context, remaining)?;
    let metadata = write_field_metadata_v1(
        descriptor,
        &mut PresenceRecordWriteContext {
            sink: &mut *context.sink,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut *context.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: context.limits.semantic_bytes,
                record_bytes: context.limits.record_bytes,
            },
            cancelled: &mut context.cancelled,
        },
    )?;
    crate::incremental_field_write::emit(metadata.field_key, metadata.value_root, context, sink)?;
    Ok(())
}

fn empty_child() -> Result<StateRootV1, MemberRecordError> {
    StateRootV1::empty(StateNamespaceV1::Business)
        .map_err(|_| MemberRecordError::Invalid("module marketplace child namespace"))
}

#[derive(Clone, Copy)]
enum MarketplacePriceTarget {
    Listing,
    BidOrdinal(u64),
}

fn change_price_amount_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
>(
    owner: ModuleMarketplaceFieldV1,
    outer_key: &str,
    target: MarketplacePriceTarget,
    expected: i64,
    after: i64,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let read_cancel = &mut context.cancelled;
    let state_error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
            || !context.values.memory.same_pool(write.memory)
        {
            return Err(ValueTreeError::Invalid("marketplace overlay pool/class").into());
        }
        let descriptor = owner.descriptor()?;
        let mut combined_cancelled = || read_cancel() || (write.cancelled)();
        let (outer_field_key, outer_semantic_bytes, outer_root) = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined_cancelled,
            };
            let metadata_key =
                WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let metadata_root = crate::incremental_field_overlay::lookup(
                transaction,
                &metadata_key,
                &mut records,
                state_error,
            )?
            .ok_or(ValueTreeError::Invalid("marketplace metadata missing"))?;
            records.presence(&metadata_root, &metadata_key, 0)?;
            let (outer_field_key, outer_semantic_bytes) =
                records.key(descriptor, WorldFieldRecordKindV1::Entry, outer_key)?;
            let outer_root = crate::incremental_field_overlay::lookup(
                transaction,
                &outer_field_key,
                &mut records,
                state_error,
            )?
            .ok_or(ValueTreeError::Invalid("marketplace outer record missing"))?;
            (outer_field_key, outer_semantic_bytes, outer_root)
        };
        let outer_binding = WorldFieldKeyV1::from_bytes(outer_field_key.as_bytes())
            .map_err(ValueTreeError::Invalid)?;
        let owner_digest = outer_binding
            .semantic_key
            .ok_or(ValueTreeError::Invalid("marketplace owner key digest"))?;
        let mut reference_bytes =
            [0; oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES];
        let outer_info = read_member_record_profile_v1(
            &outer_root,
            &outer_field_key,
            &mut context.values,
            MemberReadProfileV1 {
                codec: crate::incremental_field_member::MemberPayloadCodecV1::RecordRoot,
                semantic_bytes: outer_semantic_bytes,
                limits: crate::incremental_field_member::MemberRecordLimitsV1 {
                    semantic_bytes: context.semantic_bytes,
                    payload_bytes: reference_bytes.len() as u64,
                    record_bytes: context.record_bytes,
                },
            },
            &mut std::io::sink(),
            &mut &mut reference_bytes[..],
            &mut combined_cancelled,
        )?;
        if outer_info.payload_bytes != reference_bytes.len() as u64 {
            return Err(ValueTreeError::Invalid("marketplace record-root length").into());
        }
        let reference = RecordRootRefV1::from_canonical_bytes(&reference_bytes)
            .map_err(|_| ValueTreeError::Invalid("marketplace record-root framing"))?;
        let child = reference
            .validate_binding(&owner.schema(), &owner_digest, StateNamespaceV1::Business)
            .map_err(|_| ValueTreeError::Invalid("marketplace record-root binding"))?
            .clone();
        let child_semantic_buffer = match target {
            MarketplacePriceTarget::Listing
                if owner == ModuleMarketplaceFieldV1::ArtifactListings =>
            {
                SemanticKeyBuffer::encode("price_amount")?
            }
            MarketplacePriceTarget::BidOrdinal(ordinal)
                if owner == ModuleMarketplaceFieldV1::ArtifactBids =>
            {
                let bid_id = bid_index::select_ordinal(
                    &child,
                    descriptor,
                    ordinal,
                    &mut context.values,
                    context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut combined_cancelled,
                )?;
                SemanticKeyBuffer::encode(&(("bid", bid_id), "price_amount"))?
            }
            _ => return Err(ValueTreeError::Invalid("marketplace price target/owner").into()),
        };
        let child_semantic = CanonicalSemanticKeyRefV1::new(child_semantic_buffer.as_slice())
            .map_err(ValueTreeError::Invalid)?;
        let child_semantic_hash = hash_semantic_key_v1(
            &child_semantic,
            context.values.memory,
            context.values.class,
            context.key_work,
            &mut combined_cancelled,
        )
        .map_err(map_semantic_error)?;
        let child_semantic_bytes = child_semantic_hash.encoded_bytes;
        let child_field_key = WorldFieldKeyV1::new(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            Some(child_semantic_hash.digest),
        )
        .map_err(ValueTreeError::Invalid)?;
        let child_value_root = crate::incremental_record_tree::get(
            &child,
            &child_field_key,
            &mut context.values,
            &mut combined_cancelled,
            RECORD_TREE_LABELS,
        )?
        .ok_or(ValueTreeError::Invalid("marketplace child member missing"))?;
        let mut current_bytes = [0; 8];
        let current_info = read_member_record_profile_v1(
            &child_value_root,
            &child_field_key,
            &mut context.values,
            MemberReadProfileV1 {
                codec: crate::incremental_field_member::MemberPayloadCodecV1::I64,
                semantic_bytes: child_semantic_bytes,
                limits: crate::incremental_field_member::MemberRecordLimitsV1 {
                    semantic_bytes: context.semantic_bytes,
                    payload_bytes: 8,
                    record_bytes: context.record_bytes,
                },
            },
            &mut std::io::sink(),
            &mut &mut current_bytes[..],
            &mut combined_cancelled,
        )?;
        if current_info.payload_bytes != 8 {
            return Err(ValueTreeError::Invalid("marketplace scalar payload length").into());
        }
        let current = i64::from_be_bytes(current_bytes);
        if current != expected {
            return Err(ValueTreeError::Invalid("marketplace expected price amount").into());
        }
        if current == after {
            if combined_cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }

        let next_value = {
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || combined_cancelled(),
            };
            let output = write_member_record_v1(
                descriptor,
                WorldFieldRecordKindV1::Entry,
                &child_semantic,
                MemberPayloadRefV1::I64(after),
                &mut output,
            )?;
            if output.field_key != child_field_key {
                return Err(ValueTreeError::Invalid("marketplace child writer key").into());
            }
            output.value_root
        };
        if combined_cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let updated_child = {
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || false,
            };
            crate::incremental_record_tree::edit(
                child,
                &child_field_key,
                (Some(child_value_root), Some(next_value)),
                &mut context.values,
                &mut output,
                &mut combined_cancelled,
                tree_policy(write.limits.record_bytes),
            )?
        };
        let next_reference = RecordRootRefV1::new(owner.schema(), owner_digest, updated_child)
            .map_err(|_| ValueTreeError::Invalid("marketplace updated record root"))?;
        let next_outer = {
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || combined_cancelled(),
            };
            let output = write_member_record_v1(
                descriptor,
                WorldFieldRecordKindV1::Entry,
                outer_key,
                MemberPayloadRefV1::RecordRoot(&next_reference),
                &mut output,
            )?;
            if output.field_key != outer_field_key {
                return Err(ValueTreeError::Invalid("marketplace outer writer key").into());
            }
            output.value_root
        };
        if combined_cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(
            StateNamespaceV1::Business,
            outer_field_key.as_bytes(),
            Some(next_outer),
        )?;
        if combined_cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

/// Changes only a listing's signed price leaf. The record root and owner
/// qualification remain the caller's responsibility; this preserves a
/// fixed path cost independent of seller text or other marketplace entries.
pub fn change_module_listing_price_amount_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
>(
    listing_key: &str,
    expected: i64,
    after: i64,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    change_price_amount_overlay_v1(
        ModuleMarketplaceFieldV1::ArtifactListings,
        listing_key,
        MarketplacePriceTarget::Listing,
        expected,
        after,
        context,
        write,
        guard,
        live,
    )
}

/// Changes one bid's signed amount by ordinal. Scalar edits touch only its
/// price leaf; insertion or deletion must separately reindex positions and is
/// linear in the number of following bids in this ordinal representation.
pub fn change_module_bid_price_amount_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
>(
    bids_key: &str,
    ordinal: u64,
    expected: i64,
    after: i64,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    change_price_amount_overlay_v1(
        ModuleMarketplaceFieldV1::ArtifactBids,
        bids_key,
        MarketplacePriceTarget::BidOrdinal(ordinal),
        expected,
        after,
        context,
        write,
        guard,
        live,
    )
}

fn append_visual<W: ValueNodeReader + ValueNodeWriter + StatePageWriter, C: FnMut() -> bool>(
    descriptor: FieldDescriptor,
    value: &oasis7_agent_api::world_entity::ModuleVisualEntity,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = empty_child()?;
    append_member(
        descriptor,
        &mut root,
        "entity_id",
        MemberPayloadRefV1::Text(&value.entity_id),
        context,
        remaining,
    )?;
    append_member(
        descriptor,
        &mut root,
        "module_id",
        MemberPayloadRefV1::Text(&value.module_id),
        context,
        remaining,
    )?;
    append_member(
        descriptor,
        &mut root,
        "kind",
        MemberPayloadRefV1::Text(&value.kind),
        context,
        remaining,
    )?;
    append_member(
        descriptor,
        &mut root,
        "label_present",
        MemberPayloadRefV1::Bool(value.label.is_some()),
        context,
        remaining,
    )?;
    if let Some(label) = &value.label {
        append_member(
            descriptor,
            &mut root,
            "label_text",
            MemberPayloadRefV1::Text(label),
            context,
            remaining,
        )?;
    }
    match &value.anchor {
        ModuleVisualAnchor::Agent { agent_id } => {
            append_member(
                descriptor,
                &mut root,
                "anchor_kind",
                MemberPayloadRefV1::U64(0),
                context,
                remaining,
            )?;
            append_member(
                descriptor,
                &mut root,
                "anchor_agent_id",
                MemberPayloadRefV1::Text(agent_id),
                context,
                remaining,
            )?;
        }
        ModuleVisualAnchor::Location { location_id } => {
            append_member(
                descriptor,
                &mut root,
                "anchor_kind",
                MemberPayloadRefV1::U64(1),
                context,
                remaining,
            )?;
            append_member(
                descriptor,
                &mut root,
                "anchor_location_id",
                MemberPayloadRefV1::Text(location_id),
                context,
                remaining,
            )?;
        }
        ModuleVisualAnchor::Absolute { pos } => {
            append_member(
                descriptor,
                &mut root,
                "anchor_kind",
                MemberPayloadRefV1::U64(2),
                context,
                remaining,
            )?;
            append_member(
                descriptor,
                &mut root,
                "anchor_x_cm",
                MemberPayloadRefV1::I64(pos.x_cm),
                context,
                remaining,
            )?;
            append_member(
                descriptor,
                &mut root,
                "anchor_y_cm",
                MemberPayloadRefV1::I64(pos.y_cm),
                context,
                remaining,
            )?;
            append_member(
                descriptor,
                &mut root,
                "anchor_z_cm",
                MemberPayloadRefV1::I64(pos.z_cm),
                context,
                remaining,
            )?;
        }
    }
    Ok(root)
}

fn append_listing<W: ValueNodeReader + ValueNodeWriter + StatePageWriter, C: FnMut() -> bool>(
    descriptor: FieldDescriptor,
    value: &ModuleArtifactListingState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = empty_child()?;
    append_member(
        descriptor,
        &mut root,
        "order_id",
        MemberPayloadRefV1::U64(value.order_id),
        context,
        remaining,
    )?;
    append_member(
        descriptor,
        &mut root,
        "seller_agent_id",
        MemberPayloadRefV1::Text(&value.seller_agent_id),
        context,
        remaining,
    )?;
    append_member(
        descriptor,
        &mut root,
        "price_kind",
        MemberPayloadRefV1::U64(resource_kind(value.price_kind)),
        context,
        remaining,
    )?;
    append_member(
        descriptor,
        &mut root,
        "price_amount",
        MemberPayloadRefV1::I64(value.price_amount),
        context,
        remaining,
    )?;
    append_member(
        descriptor,
        &mut root,
        "listed_at",
        MemberPayloadRefV1::U64(value.listed_at),
        context,
        remaining,
    )?;
    Ok(root)
}

fn append_bids<W: ValueNodeReader + ValueNodeWriter + StatePageWriter, C: FnMut() -> bool>(
    descriptor: FieldDescriptor,
    values: &[ModuleArtifactBidState],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<StateRootV1, MemberRecordError> {
    let mut root = empty_child()?;
    let length = u64::try_from(values.len())
        .map_err(|_| MemberRecordError::Invalid("module bids length"))?;
    append_member(
        descriptor,
        &mut root,
        "length",
        MemberPayloadRefV1::U64(length),
        context,
        remaining,
    )?;
    append_member(
        descriptor,
        &mut root,
        "next_bid_id",
        MemberPayloadRefV1::U64(length),
        context,
        remaining,
    )?;
    for (ordinal, value) in values.iter().enumerate() {
        let ordinal =
            u64::try_from(ordinal).map_err(|_| MemberRecordError::Invalid("module bid ordinal"))?;
        // The initial stable ID is the source ordinal. Subsequent updates use
        // next_bid_id, which remains monotonic and is not the business order ID.
        let bid_id = ordinal;
        let bid = ("bid", bid_id);
        append_member(
            descriptor,
            &mut root,
            &(bid, "present"),
            MemberPayloadRefV1::Unit,
            context,
            remaining,
        )?;
        append_member(
            descriptor,
            &mut root,
            &(bid, "order_id"),
            MemberPayloadRefV1::U64(value.order_id),
            context,
            remaining,
        )?;
        append_member(
            descriptor,
            &mut root,
            &(bid, "bidder_agent_id"),
            MemberPayloadRefV1::Text(&value.bidder_agent_id),
            context,
            remaining,
        )?;
        append_member(
            descriptor,
            &mut root,
            &(bid, "price_kind"),
            MemberPayloadRefV1::U64(resource_kind(value.price_kind)),
            context,
            remaining,
        )?;
        append_member(
            descriptor,
            &mut root,
            &(bid, "price_amount"),
            MemberPayloadRefV1::I64(value.price_amount),
            context,
            remaining,
        )?;
        append_member(
            descriptor,
            &mut root,
            &(bid, "bid_at"),
            MemberPayloadRefV1::U64(value.bid_at),
            context,
            remaining,
        )?;
    }
    let (sequence_root_id, summary) =
        bid_index::append_balanced(descriptor, &mut root, 0, length, context, remaining)?;
    if summary.size != length {
        return Err(MemberRecordError::Invalid("module bid AVL genesis count"));
    }
    append_member(
        descriptor,
        &mut root,
        "sequence_root_id",
        MemberPayloadRefV1::U64(sequence_root_id),
        context,
        remaining,
    )?;
    Ok(root)
}

fn resource_kind(kind: ResourceKind) -> u64 {
    match kind {
        ResourceKind::Electricity => 0,
        ResourceKind::Data => 1,
    }
}

/// Build the provisional typed child-root members. The caller owns the single
/// global transaction and must discard every emitted write if any step fails.
pub fn prepare_module_marketplace_genesis_v1<
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let mut outer_count = 0u64;
    for field in ModuleMarketplaceFieldV1::ALL {
        let descriptor = field.descriptor()?;
        write_metadata(descriptor, context, remaining, sink)?;
        macro_rules! outer {
            ($key:expr, $child:expr) => {{
                let child = $child?;
                write_outer_root(
                    field,
                    descriptor,
                    $key.as_str(),
                    child,
                    context,
                    remaining,
                    sink,
                )?;
                outer_count = outer_count
                    .checked_add(1)
                    .ok_or(MemberRecordError::Invalid("module marketplace outer count"))?;
            }};
        }
        match field {
            ModuleMarketplaceFieldV1::VisualEntities => {
                for (key, value) in &source.module_visual_entities {
                    outer!(key, append_visual(descriptor, value, context, remaining));
                }
            }
            ModuleMarketplaceFieldV1::ArtifactListings => {
                for (key, value) in &source.module_artifact_listings {
                    outer!(key, append_listing(descriptor, value, context, remaining));
                }
            }
            ModuleMarketplaceFieldV1::ArtifactBids => {
                for (key, value) in &source.module_artifact_bids {
                    outer!(key, append_bids(descriptor, value, context, remaining));
                }
            }
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(outer_count)
}

#[cfg(test)]
#[path = "incremental_module_marketplace_tests.rs"]
mod tests;
