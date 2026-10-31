//! Persistent order-statistic AVL index for the marketplace bid Vec.
//!
//! Bid payloads remain keyed by stable internal IDs. This tree maps Vec
//! ordinals to those IDs without shifting payload records when the sequence
//! changes. A node stores child IDs and cached child summaries in six U64
//! leaves; path reads and structural updates are bounded by the AVL height.
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
pub(crate) use mutation::{LoadedBidIndexNodeV1, delete_at, insert_at, write_u64_member};

const BID_INDEX_FIELDS: [&str; 6] = [
    "index_left_id",
    "index_right_id",
    "index_left_size",
    "index_left_height",
    "index_right_size",
    "index_right_height",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BidIndexNodeV1 {
    pub left_id: u64,
    pub right_id: u64,
    pub left_size: u64,
    pub left_height: u64,
    pub right_size: u64,
    pub right_height: u64,
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
    };

    pub fn summary(self) -> Result<BidIndexSummaryV1, ValueTreeError> {
        if (self.left_id == NO_BID_ID) != (self.left_size == 0 && self.left_height == 0)
            || (self.right_id == NO_BID_ID) != (self.right_size == 0 && self.right_height == 0)
            || self.left_id != NO_BID_ID && (self.left_size == 0 || self.left_height == 0)
            || self.right_id != NO_BID_ID && (self.right_size == 0 || self.right_height == 0)
            || self.left_height.abs_diff(self.right_height) > 1
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

    fn fields(self) -> [u64; 6] {
        [
            self.left_id,
            self.right_id,
            self.left_size,
            self.left_height,
            self.right_size,
            self.right_height,
        ]
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
    let (left_id, left) = append_balanced(descriptor, root, start, middle, context, remaining)?;
    let (right_id, right) = append_balanced(descriptor, root, middle + 1, end, context, remaining)?;
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
    };
    let summary = node.summary()?;
    for (name, value) in BID_INDEX_FIELDS.into_iter().zip(node.fields()) {
        append_member(
            descriptor,
            root,
            &(("bid", middle), name),
            MemberPayloadRefV1::U64(value),
            context,
            remaining,
        )?;
    }
    Ok((middle, summary))
}

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
    if bid_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid index sentinel"));
    }
    let mut fields = [0; 6];
    for (index, name) in BID_INDEX_FIELDS.into_iter().enumerate() {
        fields[index] = read_u64_member(
            root,
            descriptor,
            &(("bid", bid_id), name),
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?
        .0;
    }
    let node = BidIndexNodeV1 {
        left_id: fields[0],
        right_id: fields[1],
        left_size: fields[2],
        left_height: fields[3],
        right_size: fields[4],
        right_height: fields[5],
    };
    node.summary()?;
    Ok(node)
}

/// Select the bid ID at the original Vec ordinal by following the persistent
/// order-statistic tree. The depth ceiling also makes malformed cycles fail
/// closed without allowing unbounded source reads.
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
    for depth in 0..MAX_BID_INDEX_DEPTH {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
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
        let summary = node.summary()?;
        if (depth == 0 && summary.size != length) || summary.size > length {
            return Err(ValueTreeError::Invalid("marketplace bid index size"));
        }
        if rank < node.left_size {
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
