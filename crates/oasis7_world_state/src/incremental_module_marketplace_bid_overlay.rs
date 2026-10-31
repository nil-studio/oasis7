//! Single-position bid list mutations over the authenticated marketplace child.
//!
//! The persistent AVL maps source Vec ordinals to monotonic internal bid IDs.
//! A caller must supply the exact current RecordRootRef produced by the owner
//! qualification path. This module preserves Vec order and payload IDs; it does
//! not authorize a bid event or qualify the complete World.
use super::{
    ModuleMarketplaceFieldV1, RECORD_TREE_LABELS, SemanticKeyBuffer, append_member, bid_index,
    map_semantic_error, tree_policy,
};
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberPayloadRefV1, MemberReadProfileV1, MemberRecordError,
        MemberRecordLimitsV1, MemberRecordWriteContext, WorldFieldKeyV1,
        read_member_record_profile_v1, write_member_record_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::FieldDescriptor,
    incremental_record_tree,
    incremental_semantic_key::{
        CanonicalSemanticKeyRefV1, SemanticKeyBudget, SemanticKeyV1, hash_semantic_key_v1,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
    state::ModuleArtifactBidState,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_record_root::RecordRootRefV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::io::{self, Write};

const BID_RECORD_FIELDS: [&str; 6] = [
    "present",
    "order_id",
    "bidder_agent_id",
    "price_kind",
    "price_amount",
    "bid_at",
];

/// One ordinal insertion or deletion from a bid Vec.
#[derive(Debug, Clone, Copy)]
pub enum ModuleArtifactBidChangeV1<'a> {
    Insert {
        ordinal: u64,
        bid: &'a ModuleArtifactBidState,
    },
    Delete {
        ordinal: u64,
        expected_order_id: u64,
        expected_bidder_agent_id: &'a str,
    },
}

fn member_key<K: SemanticKeyV1 + ?Sized, R: ValueNodeReader, C: FnMut() -> bool>(
    descriptor: FieldDescriptor,
    semantic_key: &K,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    let buffer = SemanticKeyBuffer::encode(semantic_key)?;
    let semantic =
        CanonicalSemanticKeyRefV1::new(buffer.as_slice()).map_err(ValueTreeError::Invalid)?;
    let hashed = hash_semantic_key_v1(&semantic, values.memory, values.class, key_work, cancelled)
        .map_err(map_semantic_error)?;
    if hashed.encoded_bytes > semantic_limit || hashed.encoded_bytes > record_limit {
        return Err(ValueTreeError::ResourceLimited(
            "marketplace bid semantic key",
        ));
    }
    let key = WorldFieldKeyV1::new(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        Some(hashed.digest),
    )
    .map_err(ValueTreeError::Invalid)?;
    Ok((key, hashed.encoded_bytes))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Binds a typed field key to its root using the caller's shared classed read budget."
)]
fn require_member_root<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: FieldDescriptor,
    semantic_key: &impl SemanticKeyV1,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<([u8; 32], WorldFieldKeyV1, u64), ValueTreeError> {
    let (key, semantic_bytes) = member_key(
        descriptor,
        semantic_key,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let value_root =
        incremental_record_tree::get(root, &key, values, cancelled, RECORD_TREE_LABELS)?
            .ok_or(ValueTreeError::Invalid("marketplace bid field missing"))?;
    Ok((value_root, key, semantic_bytes))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Compares the original borrowed text under the same key, value, and cancellation budgets."
)]
fn read_bid_text_equals<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: FieldDescriptor,
    stable_id: u64,
    expected: &str,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<bool, ValueTreeError> {
    let semantic_key = (("bid", stable_id), "bidder_agent_id");
    let (value_root, key, semantic_bytes) = require_member_root(
        root,
        descriptor,
        &semantic_key,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let mut compare = ExactBytes {
        expected: expected.as_bytes(),
        offset: 0,
        equal: true,
    };
    let info = read_member_record_profile_v1(
        &value_root,
        &key,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: semantic_limit,
                payload_bytes: record_limit,
                record_bytes: record_limit,
            },
        },
        &mut io::sink(),
        &mut compare,
        cancelled,
    )?;
    Ok(compare.equal
        && compare.offset == expected.len()
        && info.payload_bytes == expected.len() as u64)
}

struct ExactBytes<'a> {
    expected: &'a [u8],
    offset: usize,
    equal: bool,
}
impl Write for ExactBytes<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(end) = self.offset.checked_add(bytes.len()) else {
            self.equal = false;
            return Err(io::Error::other("marketplace bidder comparison overflow"));
        };
        if end > self.expected.len() || self.expected[self.offset..end] != *bytes {
            self.equal = false;
            return Err(io::Error::other("marketplace bidder mismatch"));
        }
        self.offset = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps a member deletion's read/write classes, limits, and cancellation boundaries explicit."
)]
fn delete_bid_member<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: FieldDescriptor,
    stable_id: u64,
    name: &'static str,
    values: &mut ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining_records: &mut u64,
) -> Result<(), ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    *remaining_records = remaining_records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("marketplace bid records"))?;
    let key_semantic = (("bid", stable_id), name);
    let (value_root, key, _) = require_member_root(
        root,
        descriptor,
        &key_semantic,
        values,
        write.key_work,
        write.limits.semantic_bytes,
        write.limits.record_bytes,
        cancelled,
    )?;
    let mut page_context = MemberRecordWriteContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        key_work: &mut *write.key_work,
        limits: write.limits,
        cancelled: || false,
    };
    *root = incremental_record_tree::edit(
        root.clone(),
        &key,
        (Some(value_root), None),
        values,
        &mut page_context,
        cancelled,
        tree_policy(write.limits.record_bytes),
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}

fn encode_bid(price_kind: ResourceKind) -> u64 {
    match price_kind {
        ResourceKind::Electricity => 0,
        ResourceKind::Data => 1,
    }
}

fn append_bid<W, CW>(
    root: &mut StateRootV1,
    descriptor: FieldDescriptor,
    stable_id: u64,
    bid: &ModuleArtifactBidState,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    remaining_records: &mut u64,
) -> Result<(), MemberRecordError>
where
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
{
    let values = [
        ("present", MemberPayloadRefV1::Unit),
        ("order_id", MemberPayloadRefV1::U64(bid.order_id)),
        (
            "bidder_agent_id",
            MemberPayloadRefV1::Text(&bid.bidder_agent_id),
        ),
        (
            "price_kind",
            MemberPayloadRefV1::U64(encode_bid(bid.price_kind)),
        ),
        ("price_amount", MemberPayloadRefV1::I64(bid.price_amount)),
        ("bid_at", MemberPayloadRefV1::U64(bid.bid_at)),
    ];
    for (name, payload) in values {
        let key = (("bid", stable_id), name);
        append_member(descriptor, root, &key, payload, write, remaining_records)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
    }
    Ok(())
}

/// Inserts or deletes exactly one Vec ordinal under a qualified current record.
/// The caller repeats deletes for all matching business IDs if the legacy
/// reducer's `retain` semantics require removing duplicates. Every call reads
/// the current outer record in the transaction and binds it to `expected`.
#[expect(
    clippy::too_many_arguments,
    reason = "The transaction, authority guard, live fence, stores, and resource budgets are independent admission inputs."
)]
pub fn change_module_bid_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
>(
    bids_key: &str,
    expected: &RecordRootRefV1,
    change: ModuleArtifactBidChangeV1<'_>,
    remaining_records: &mut u64,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<RecordRootRefV1, WorldFieldOverlayErrorV1> {
    let cancelled = &mut context.cancelled;
    let state_error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
            || !context.values.memory.same_pool(write.memory)
        {
            return Err(ValueTreeError::Invalid("marketplace bid overlay pool/class").into());
        }
        let owner = ModuleMarketplaceFieldV1::ArtifactBids;
        let descriptor = owner.descriptor()?;
        let write_sink = &mut *write.sink;
        let write_memory = write.memory;
        let write_io = &mut *write.io;
        let write_key_work = &mut *write.key_work;
        let write_limits = write.limits;
        let write_cancelled = &mut write.cancelled;
        let mut combined_cancelled = || cancelled() || (write_cancelled)();
        let (outer_key, outer_semantic_bytes, outer_value_root) = {
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
            let metadata_root = lookup(transaction, &metadata_key, &mut records, state_error)?
                .ok_or(ValueTreeError::Invalid("marketplace bid metadata missing"))?;
            records.presence(&metadata_root, &metadata_key, 0)?;
            let (outer_key, semantic_bytes) =
                records.key(descriptor, WorldFieldRecordKindV1::Entry, bids_key)?;
            let outer_value_root = lookup(transaction, &outer_key, &mut records, state_error)?
                .ok_or(ValueTreeError::Invalid("marketplace bids record missing"))?;
            (outer_key, semantic_bytes, outer_value_root)
        };

        let binding =
            WorldFieldKeyV1::from_bytes(outer_key.as_bytes()).map_err(ValueTreeError::Invalid)?;
        let owner_digest = binding
            .semantic_key
            .ok_or(ValueTreeError::Invalid("marketplace bids owner digest"))?;
        let mut outer_bytes =
            [0; oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES];
        let outer_info = read_member_record_profile_v1(
            &outer_value_root,
            &outer_key,
            &mut context.values,
            MemberReadProfileV1 {
                codec: MemberPayloadCodecV1::RecordRoot,
                semantic_bytes: outer_semantic_bytes,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: context.semantic_bytes,
                    payload_bytes: outer_bytes.len() as u64,
                    record_bytes: context.record_bytes,
                },
            },
            &mut io::sink(),
            &mut &mut outer_bytes[..],
            &mut combined_cancelled,
        )?;
        if outer_info.payload_bytes != outer_bytes.len() as u64 {
            return Err(ValueTreeError::Invalid("marketplace bids root length").into());
        }
        let actual = RecordRootRefV1::from_canonical_bytes(&outer_bytes)
            .map_err(|_| ValueTreeError::Invalid("marketplace bids root framing"))?;
        let child = actual
            .validate_binding(&owner.schema(), &owner_digest, StateNamespaceV1::Business)
            .map_err(|_| ValueTreeError::Invalid("marketplace bids root binding"))?
            .clone();
        if actual != *expected {
            return Err(ValueTreeError::Invalid("marketplace bids expected root").into());
        }
        if expected
            .validate_binding(&owner.schema(), &owner_digest, StateNamespaceV1::Business)
            .is_err()
        {
            return Err(ValueTreeError::Invalid("marketplace bids expected binding").into());
        }

        let (length, length_root) = bid_index::read_u64_member(
            &child,
            descriptor,
            "length",
            &mut context.values,
            context.key_work,
            context.semantic_bytes,
            context.record_bytes,
            &mut combined_cancelled,
        )?;
        let (next_bid_id, next_bid_id_root) = bid_index::read_u64_member(
            &child,
            descriptor,
            "next_bid_id",
            &mut context.values,
            context.key_work,
            context.semantic_bytes,
            context.record_bytes,
            &mut combined_cancelled,
        )?;
        let (sequence_root_id, sequence_root_field_root) = bid_index::read_u64_member(
            &child,
            descriptor,
            "sequence_root_id",
            &mut context.values,
            context.key_work,
            context.semantic_bytes,
            context.record_bytes,
            &mut combined_cancelled,
        )?;
        if length == 0 {
            if sequence_root_id != bid_index::NO_BID_ID {
                return Err(ValueTreeError::Invalid("marketplace empty bids root").into());
            }
        } else {
            if sequence_root_id == bid_index::NO_BID_ID {
                return Err(ValueTreeError::Invalid("marketplace nonempty bids root").into());
            }
            let summary = bid_index::read_node(
                &child,
                descriptor,
                sequence_root_id,
                &mut context.values,
                context.key_work,
                context.semantic_bytes,
                context.record_bytes,
                &mut combined_cancelled,
            )?
            .summary()?;
            if summary.size != length {
                return Err(ValueTreeError::Invalid("marketplace bids root size").into());
            }
        }

        let mut updated_child = child.clone();
        let new_sequence_root = match change {
            ModuleArtifactBidChangeV1::Insert { ordinal, bid } => {
                if ordinal > length {
                    return Err(ValueTreeError::Invalid("marketplace bid insert ordinal").into());
                }
                if next_bid_id == bid_index::NO_BID_ID {
                    return Err(ValueTreeError::ResourceLimited("marketplace bid IDs").into());
                }
                let next_length = length
                    .checked_add(1)
                    .ok_or(ValueTreeError::ResourceLimited("marketplace bid length"))?;
                let next_id = next_bid_id
                    .checked_add(1)
                    .ok_or(ValueTreeError::ResourceLimited("marketplace bid IDs"))?;
                let present_key = (("bid", next_bid_id), "present");
                let (present_key, _) = member_key(
                    descriptor,
                    &present_key,
                    &mut context.values,
                    context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut combined_cancelled,
                )?;
                if incremental_record_tree::get(
                    &child,
                    &present_key,
                    &mut context.values,
                    &mut combined_cancelled,
                    RECORD_TREE_LABELS,
                )?
                .is_some()
                {
                    return Err(
                        ValueTreeError::Invalid("marketplace bid ID already present").into(),
                    );
                }
                {
                    let mut bid_output = MemberRecordWriteContext {
                        sink: write_sink,
                        memory: write_memory,
                        io: write_io,
                        key_work: write_key_work,
                        limits: write_limits,
                        cancelled: || combined_cancelled(),
                    };
                    append_bid(
                        &mut updated_child,
                        descriptor,
                        next_bid_id,
                        bid,
                        &mut bid_output,
                        remaining_records,
                    )?;
                }
                let mut index_output = MemberRecordWriteContext {
                    sink: write_sink,
                    memory: write_memory,
                    io: write_io,
                    key_work: write_key_work,
                    limits: write_limits,
                    cancelled: || false,
                };
                let index_root = bid_index::insert_at(
                    &mut updated_child,
                    descriptor,
                    sequence_root_id,
                    ordinal,
                    next_bid_id,
                    &mut context.values,
                    context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut index_output,
                    &mut combined_cancelled,
                    remaining_records,
                )?;
                let actual_size = index_root.node.summary()?.size;
                if actual_size != next_length {
                    return Err(ValueTreeError::Invalid("marketplace bid insert size").into());
                }
                update_scalar(
                    &mut updated_child,
                    descriptor,
                    "length",
                    length_root,
                    next_length,
                    &mut context.values,
                    &mut index_output,
                    &mut combined_cancelled,
                    remaining_records,
                )?;
                update_scalar(
                    &mut updated_child,
                    descriptor,
                    "next_bid_id",
                    next_bid_id_root,
                    next_id,
                    &mut context.values,
                    &mut index_output,
                    &mut combined_cancelled,
                    remaining_records,
                )?;
                update_scalar(
                    &mut updated_child,
                    descriptor,
                    "sequence_root_id",
                    sequence_root_field_root,
                    index_root.id,
                    &mut context.values,
                    &mut index_output,
                    &mut combined_cancelled,
                    remaining_records,
                )?;
                index_root.id
            }
            ModuleArtifactBidChangeV1::Delete {
                ordinal,
                expected_order_id,
                expected_bidder_agent_id,
            } => {
                if ordinal >= length {
                    return Err(ValueTreeError::Invalid("marketplace bid delete ordinal").into());
                }
                let stable_id = bid_index::select_ordinal(
                    &child,
                    descriptor,
                    ordinal,
                    &mut context.values,
                    context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut combined_cancelled,
                )?;
                let (order_id, _) = bid_index::read_u64_member(
                    &child,
                    descriptor,
                    &(("bid", stable_id), "order_id"),
                    &mut context.values,
                    context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut combined_cancelled,
                )?;
                if order_id != expected_order_id
                    || !read_bid_text_equals(
                        &child,
                        descriptor,
                        stable_id,
                        expected_bidder_agent_id,
                        &mut context.values,
                        context.key_work,
                        context.semantic_bytes,
                        context.record_bytes,
                        &mut combined_cancelled,
                    )?
                {
                    return Err(
                        ValueTreeError::Invalid("marketplace bid delete expected value").into(),
                    );
                }
                let mut delete_output = MemberRecordWriteContext {
                    sink: write_sink,
                    memory: write_memory,
                    io: write_io,
                    key_work: write_key_work,
                    limits: write_limits,
                    cancelled: || false,
                };
                let marker = (("bid", stable_id), "present");
                let (marker_root, marker_key, marker_semantic) = require_member_root(
                    &child,
                    descriptor,
                    &marker,
                    &mut context.values,
                    context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut combined_cancelled,
                )?;
                let marker_info = read_member_record_profile_v1(
                    &marker_root,
                    &marker_key,
                    &mut context.values,
                    MemberReadProfileV1 {
                        codec: MemberPayloadCodecV1::Unit,
                        semantic_bytes: marker_semantic,
                        limits: write_limits,
                    },
                    &mut io::sink(),
                    &mut io::sink(),
                    &mut combined_cancelled,
                )?;
                if marker_info.payload_bytes != 0 {
                    return Err(ValueTreeError::Invalid("marketplace bid marker profile").into());
                }
                let (next_root, summary) = bid_index::delete_at(
                    &mut updated_child,
                    descriptor,
                    sequence_root_id,
                    ordinal,
                    &mut context.values,
                    context.key_work,
                    context.semantic_bytes,
                    context.record_bytes,
                    &mut delete_output,
                    &mut combined_cancelled,
                    remaining_records,
                )?;
                let next_length = length
                    .checked_sub(1)
                    .ok_or(ValueTreeError::Invalid("marketplace bid length underflow"))?;
                if summary.size != next_length
                    || (next_length == 0) != (next_root == bid_index::NO_BID_ID)
                {
                    return Err(ValueTreeError::Invalid("marketplace bid delete size").into());
                }
                for name in BID_RECORD_FIELDS {
                    delete_bid_member(
                        &mut updated_child,
                        descriptor,
                        stable_id,
                        name,
                        &mut context.values,
                        &mut delete_output,
                        &mut combined_cancelled,
                        remaining_records,
                    )?;
                }
                update_scalar(
                    &mut updated_child,
                    descriptor,
                    "length",
                    length_root,
                    next_length,
                    &mut context.values,
                    &mut delete_output,
                    &mut combined_cancelled,
                    remaining_records,
                )?;
                update_scalar(
                    &mut updated_child,
                    descriptor,
                    "sequence_root_id",
                    sequence_root_field_root,
                    next_root,
                    &mut context.values,
                    &mut delete_output,
                    &mut combined_cancelled,
                    remaining_records,
                )?;
                next_root
            }
        };
        if combined_cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let next_reference = RecordRootRefV1::new(owner.schema(), owner_digest, updated_child)
            .map_err(|_| ValueTreeError::Invalid("marketplace updated record root"))?;
        let outer = {
            let mut outer_output = MemberRecordWriteContext {
                sink: write_sink,
                memory: write_memory,
                io: write_io,
                key_work: write_key_work,
                limits: write_limits,
                cancelled: || combined_cancelled(),
            };
            write_member_record_v1(
                descriptor,
                WorldFieldRecordKindV1::Entry,
                bids_key,
                MemberPayloadRefV1::RecordRoot(&next_reference),
                &mut outer_output,
            )?
        };
        if outer.field_key != outer_key {
            return Err(ValueTreeError::Invalid("marketplace bids outer writer key").into());
        }
        transaction.write(
            StateNamespaceV1::Business,
            outer_key.as_bytes(),
            Some(outer.value_root),
        )?;
        if combined_cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let _ = new_sequence_root;
        Ok(next_reference)
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preserves independent typed before/after values, stores, budgets, and cancellation checks for one scalar write."
)]
fn update_scalar<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: FieldDescriptor,
    name: &'static str,
    current: [u8; 32],
    next: u64,
    values: &mut ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining_records: &mut u64,
) -> Result<(), ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    let member = bid_index::write_u64_member(
        root,
        descriptor,
        &name,
        Some(current),
        Some(next),
        values,
        write,
        cancelled,
        remaining_records,
    )?
    .ok_or(ValueTreeError::Invalid("marketplace scalar update root"))?;
    let _ = member;
    Ok(())
}
