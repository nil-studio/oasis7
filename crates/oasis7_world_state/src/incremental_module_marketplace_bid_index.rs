//! Persistent order-statistic AVL index for the marketplace bid Vec.
//!
//! Bid payloads remain keyed by stable internal IDs. This tree maps Vec
//! ordinals to those IDs without shifting payload records when the sequence
//! changes. A node stores parent/child IDs and cached summaries in one fixed
//! Entry leaf with the FixedU64x7 payload; path reads and updates are bounded
//! by the AVL height.
use super::{RECORD_TREE_LABELS, SemanticKeyBuffer, append_member};
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberPayloadRefV1, MemberReadProfileV1, MemberRecordError,
        MemberRecordLimitsV1, MemberRecordWriteContext, WorldFieldKeyV1,
        read_member_record_profile_v1, write_member_record_v1,
    },
    incremental_semantic_key::{
        CanonicalSemanticKeyRefV1, SemanticKeyBudget, hash_semantic_key_v1,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_state::{StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;

pub(crate) const NO_BID_ID: u64 = u64::MAX;
pub(crate) const MAX_BID_INDEX_DEPTH: usize = 128;
#[path = "incremental_module_marketplace_bid_index_mutation.rs"]
mod mutation;
pub(crate) use mutation::{delete_at, insert_at, write_u64_member};

const BID_INDEX_NODE_FIELD: &str = "index_node";
pub(crate) const BID_INDEX_NODE_BYTES: usize = 7 * 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BidIndexNodeV1 {
    pub left_id: u64,
    pub right_id: u64,
    pub left_size: u64,
    pub left_height: u64,
    pub right_size: u64,
    pub right_height: u64,
    pub parent_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BidIndexSummaryV1 {
    pub size: u64,
    pub height: u64,
}

impl BidIndexNodeV1 {
    pub const LEAF: Self = Self {
        left_id: NO_BID_ID,
        right_id: NO_BID_ID,
        left_size: 0,
        left_height: 0,
        right_size: 0,
        right_height: 0,
        parent_id: NO_BID_ID,
    };

    fn structural_summary(self) -> Result<BidIndexSummaryV1, ValueTreeError> {
        if (self.left_id == NO_BID_ID) != (self.left_size == 0 && self.left_height == 0)
            || (self.right_id == NO_BID_ID) != (self.right_size == 0 && self.right_height == 0)
            || self.left_id != NO_BID_ID && (self.left_size == 0 || self.left_height == 0)
            || self.right_id != NO_BID_ID && (self.right_size == 0 || self.right_height == 0)
        {
            return Err(ValueTreeError::Invalid("marketplace bid AVL node profile"));
        }
        Ok(BidIndexSummaryV1 {
            size: 1u64
                .checked_add(self.left_size)
                .and_then(|size| size.checked_add(self.right_size))
                .ok_or(ValueTreeError::Invalid("marketplace bid AVL size overflow"))?,
            height: 1u64
                .checked_add(self.left_height.max(self.right_height))
                .ok_or(ValueTreeError::Invalid(
                    "marketplace bid AVL height overflow",
                ))?,
        })
    }

    pub fn summary(self) -> Result<BidIndexSummaryV1, ValueTreeError> {
        let summary = self.structural_summary()?;
        if self.left_height.abs_diff(self.right_height) > 1 {
            return Err(ValueTreeError::Invalid("marketplace bid AVL node profile"));
        }
        Ok(summary)
    }

    pub(crate) fn fields(self) -> [u64; 7] {
        [
            self.left_id,
            self.right_id,
            self.left_size,
            self.left_height,
            self.right_size,
            self.right_height,
            self.parent_id,
        ]
    }

    pub(crate) fn decode(bytes: &[u8; BID_INDEX_NODE_BYTES]) -> Self {
        let mut fields = [0; 7];
        for (index, field) in fields.iter_mut().enumerate() {
            let start = index * 8;
            *field = u64::from_be_bytes(bytes[start..start + 8].try_into().unwrap());
        }
        Self {
            left_id: fields[0],
            right_id: fields[1],
            left_size: fields[2],
            left_height: fields[3],
            right_size: fields[4],
            right_height: fields[5],
            parent_id: fields[6],
        }
    }
}

/// Build an AVL sequence from the source Vec's exact order without collecting
/// a second ID list. Genesis IDs are the original ordinals; later IDs are
/// monotonic and are never reused after deletion.
pub(crate) fn append_balanced<W, C>(
    descriptor: crate::incremental_projection::FieldDescriptor,
    root: &mut StateRootV1,
    start: u64,
    end: u64,
    parent_id: u64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<(u64, BidIndexSummaryV1), MemberRecordError>
where
    W: ValueNodeReader
        + oasis7_formal_protocol::incremental_value::ValueNodeWriter
        + StatePageWriter,
    C: FnMut() -> bool,
{
    if start == end {
        return Ok((NO_BID_ID, BidIndexSummaryV1 { size: 0, height: 0 }));
    }
    let middle = start + (end - start) / 2;
    let (left_id, left) =
        append_balanced(descriptor, root, start, middle, middle, context, remaining)?;
    let (right_id, right) = append_balanced(
        descriptor,
        root,
        middle + 1,
        end,
        middle,
        context,
        remaining,
    )?;
    if middle == NO_BID_ID {
        return Err(MemberRecordError::Invalid("marketplace bid ID sentinel"));
    }
    let node = BidIndexNodeV1 {
        left_id,
        right_id,
        left_size: left.size,
        left_height: left.height,
        right_size: right.size,
        right_height: right.height,
        parent_id,
    };
    let summary = node.summary()?;
    append_member(
        descriptor,
        root,
        &(("bid", middle), BID_INDEX_NODE_FIELD),
        MemberPayloadRefV1::FixedU64x7(node.fields()),
        context,
        remaining,
    )?;
    Ok((middle, summary))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps the field profile, classed read context, key budget, and cancellation callback explicit."
)]
pub(crate) fn read_u64_member<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    key: &(impl crate::incremental_semantic_key::SemanticKeyV1 + ?Sized),
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<(u64, [u8; 32]), ValueTreeError> {
    let buffer = SemanticKeyBuffer::encode(key)?;
    let semantic =
        CanonicalSemanticKeyRefV1::new(buffer.as_slice()).map_err(ValueTreeError::Invalid)?;
    let hashed = hash_semantic_key_v1(
        &semantic,
        values.memory,
        values.class,
        key_work,
        &mut *cancelled,
    )
    .map_err(super::map_semantic_error)?;
    let key = WorldFieldKeyV1::new(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        Some(hashed.digest),
    )
    .map_err(ValueTreeError::Invalid)?;
    let value_root =
        crate::incremental_record_tree::get(root, &key, values, cancelled, RECORD_TREE_LABELS)?
            .ok_or(ValueTreeError::Invalid(
                "marketplace bid index field missing",
            ))?;
    let mut bytes = [0; 8];
    let info = read_member_record_profile_v1(
        &value_root,
        &key,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::U64,
            semantic_bytes: hashed.encoded_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: semantic_limit,
                payload_bytes: 8,
                record_bytes: record_limit,
            },
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::U64 || info.payload_bytes != 8 {
        return Err(ValueTreeError::Invalid(
            "marketplace bid index scalar profile",
        ));
    }
    Ok((u64::from_be_bytes(bytes), value_root))
}

fn node_field_key<R: ValueNodeReader, C: FnMut() -> bool>(
    descriptor: crate::incremental_projection::FieldDescriptor,
    bid_id: u64,
    values: &ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    let semantic_key = (("bid", bid_id), BID_INDEX_NODE_FIELD);
    let buffer = SemanticKeyBuffer::encode(&semantic_key)?;
    let semantic =
        CanonicalSemanticKeyRefV1::new(buffer.as_slice()).map_err(ValueTreeError::Invalid)?;
    let hashed = hash_semantic_key_v1(
        &semantic,
        values.memory,
        values.class,
        key_work,
        &mut *cancelled,
    )
    .map_err(super::map_semantic_error)?;
    if hashed.encoded_bytes > semantic_limit || hashed.encoded_bytes > record_limit {
        return Err(ValueTreeError::ResourceLimited(
            "marketplace bid index semantic key",
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
    reason = "Keeps the field profile, classed reader, key budget, and cancellation callback explicit."
)]
pub(crate) fn read_node_with_root<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    bid_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<(BidIndexNodeV1, [u8; 32]), ValueTreeError> {
    if bid_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid index sentinel"));
    }
    let (key, semantic_bytes) = node_field_key(
        descriptor,
        bid_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let value_root =
        crate::incremental_record_tree::get(root, &key, values, cancelled, RECORD_TREE_LABELS)?
            .ok_or(ValueTreeError::Invalid(
                "marketplace bid index node missing",
            ))?;
    let mut bytes = [0; BID_INDEX_NODE_BYTES];
    let info = read_member_record_profile_v1(
        &value_root,
        &key,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::FixedU64x7,
            semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: semantic_limit,
                payload_bytes: BID_INDEX_NODE_BYTES as u64,
                record_bytes: record_limit,
            },
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        cancelled,
    )?;
    if info.payload_bytes != BID_INDEX_NODE_BYTES as u64 {
        return Err(ValueTreeError::Invalid("marketplace bid index node length"));
    }
    let node = BidIndexNodeV1::decode(&bytes);
    node.summary()?;
    Ok((node, value_root))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps the field profile, classed reader, key budget, and cancellation callback explicit."
)]
pub(crate) fn read_node<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    bid_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<BidIndexNodeV1, ValueTreeError> {
    read_node_with_root(
        root,
        descriptor,
        bid_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )
    .map(|(node, _)| node)
}

/// Select the bid ID at the original Vec ordinal by following the persistent
/// order-statistic tree. The depth ceiling also makes malformed cycles fail
/// closed without allowing unbounded source reads.
#[expect(
    clippy::too_many_arguments,
    reason = "Keeps one shared tree traversal's root, classed reader, bounds, and cancellation callback explicit."
)]
pub(crate) fn select_ordinal<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    ordinal: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<u64, ValueTreeError> {
    let (length, _) = read_u64_member(
        root,
        descriptor,
        "length",
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    if ordinal >= length {
        return Err(ValueTreeError::Invalid("marketplace bid ordinal range"));
    }
    let (mut node_id, _) = read_u64_member(
        root,
        descriptor,
        "sequence_root_id",
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    if node_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid sequence root"));
    }
    let mut rank = ordinal;
    let mut parent_id = NO_BID_ID;
    let mut path = [NO_BID_ID; MAX_BID_INDEX_DEPTH];
    for depth in 0..MAX_BID_INDEX_DEPTH {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if path[..depth].contains(&node_id) {
            return Err(ValueTreeError::Invalid("marketplace bid index cycle"));
        }
        path[depth] = node_id;
        let node = read_node(
            root,
            descriptor,
            node_id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        if node.parent_id != parent_id {
            return Err(ValueTreeError::Invalid("marketplace bid index parent link"));
        }
        let summary = node.summary()?;
        if (depth == 0 && summary.size != length) || summary.size > length {
            return Err(ValueTreeError::Invalid("marketplace bid index size"));
        }
        if rank < node.left_size {
            parent_id = node_id;
            node_id = node.left_id;
            if node_id == NO_BID_ID {
                return Err(ValueTreeError::Invalid("marketplace bid index left link"));
            }
        } else if rank == node.left_size {
            return Ok(node_id);
        } else {
            rank = rank
                .checked_sub(node.left_size + 1)
                .ok_or(ValueTreeError::Invalid("marketplace bid index rank"))?;
            parent_id = node_id;
            node_id = node.right_id;
            if node_id == NO_BID_ID {
                return Err(ValueTreeError::Invalid("marketplace bid index right link"));
            }
        }
    }
    Err(ValueTreeError::ResourceLimited(
        "marketplace bid index depth",
    ))
}

/// Resolve a stable bid ID to its current Vec ordinal by following the
/// authenticated parent links. The traversal uses a fixed path buffer and
/// shares the caller's value/key budgets with the enclosing owner operation.
#[cfg(test)]
#[expect(
    clippy::too_many_arguments,
    reason = "Keeps the root, classed reader, bounds, and cancellation callback explicit for one shared traversal."
)]
pub(crate) fn ordinal_of_id<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    root_id: u64,
    bid_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<u64, ValueTreeError> {
    if root_id == NO_BID_ID || bid_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid index sentinel"));
    }
    let mut current_id = bid_id;
    let mut current = read_node(
        root,
        descriptor,
        current_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let mut ordinal = current.left_size;
    let mut path = [NO_BID_ID; MAX_BID_INDEX_DEPTH];

    for depth in 0..MAX_BID_INDEX_DEPTH {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if path[..depth].contains(&current_id) {
            return Err(ValueTreeError::Invalid("marketplace bid index cycle"));
        }
        path[depth] = current_id;
        if current_id == root_id {
            if current.parent_id != NO_BID_ID || ordinal >= current.summary()?.size {
                return Err(ValueTreeError::Invalid("marketplace bid index root/rank"));
            }
            return Ok(ordinal);
        }
        if current.parent_id == NO_BID_ID {
            return Err(ValueTreeError::Invalid("marketplace bid index parent link"));
        }
        let parent_id = current.parent_id;
        let parent = read_node(
            root,
            descriptor,
            parent_id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        if parent.left_id == current_id {
            // The current subtree precedes the parent, so its local rank is
            // unchanged while walking upward.
        } else if parent.right_id == current_id {
            ordinal = ordinal
                .checked_add(parent.left_size)
                .and_then(|value| value.checked_add(1))
                .ok_or(ValueTreeError::Invalid(
                    "marketplace bid index rank overflow",
                ))?;
        } else {
            return Err(ValueTreeError::Invalid("marketplace bid index parent link"));
        }
        current_id = parent_id;
        current = parent;
    }
    Err(ValueTreeError::ResourceLimited(
        "marketplace bid index depth",
    ))
}
