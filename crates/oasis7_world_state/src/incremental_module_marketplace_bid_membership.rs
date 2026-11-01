//! Authenticated secondary membership keys for bids grouped by their original
//! `(order_id, bidder_agent_id)` pair.
use super::{MemberRecordError, MemberRecordWriteContext, append_member, bid_index};
use super::{RECORD_TREE_LABELS, SemanticKeyBuffer, map_semantic_error};
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberPayloadRefV1, MemberReadProfileV1, MemberRecordLimitsV1,
        WorldFieldKeyV1, read_member_record_profile_v1,
    },
    incremental_record_tree,
    incremental_semantic_key::{
        CanonicalSemanticKeyRefV1, SemanticKeyBudget, hash_semantic_key_v1,
    },
    incremental_world_field_key::WorldFieldRecordKindV1,
    state::ModuleArtifactBidState,
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::io;

const GROUP_KEY_PREFIX: &[u8] = b"bid_group/";
const GROUP_KEY_HEX_BYTES: usize = 64;
pub(crate) const GROUP_KEY_BYTES: usize = GROUP_KEY_PREFIX.len() + GROUP_KEY_HEX_BYTES;
pub(crate) const GROUP_SCHEMA_DOMAIN: &[u8] = b"oasis7.iws.market.bid-membership-group.v1\0";
pub(crate) const PAIR_DIGEST_DOMAIN: &[u8] = b"oasis7.iws.market.bid-membership.v1\0";

#[derive(Clone, Copy)]
pub(crate) struct GroupKeyV1([u8; GROUP_KEY_BYTES]);

impl GroupKeyV1 {
    pub(crate) fn from_digest(digest: &[u8; 32]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut bytes = [0; GROUP_KEY_BYTES];
        bytes[..GROUP_KEY_PREFIX.len()].copy_from_slice(GROUP_KEY_PREFIX);
        for (index, byte) in digest.iter().copied().enumerate() {
            bytes[GROUP_KEY_PREFIX.len() + index * 2] = HEX[(byte >> 4) as usize];
            bytes[GROUP_KEY_PREFIX.len() + index * 2 + 1] = HEX[(byte & 0x0f) as usize];
        }
        Self(bytes)
    }

    pub(crate) fn from_text(text: &str) -> Option<([u8; 32], Self)> {
        let suffix = text.strip_prefix(std::str::from_utf8(GROUP_KEY_PREFIX).ok()?)?;
        if suffix.len() != GROUP_KEY_HEX_BYTES {
            return None;
        }
        let mut digest = [0; 32];
        for (index, pair) in suffix.as_bytes().chunks_exact(2).enumerate() {
            let high = lower_hex(pair[0])?;
            let low = lower_hex(pair[1])?;
            digest[index] = (high << 4) | low;
        }
        let key = Self::from_digest(&digest);
        (key.as_bytes() == text.as_bytes()).then_some((digest, key))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; GROUP_KEY_BYTES] {
        &self.0
    }

    pub(crate) fn as_str(&self) -> &str {
        // Prefix and hex digits are fixed ASCII bytes.
        std::str::from_utf8(&self.0).expect("fixed bid group key is ASCII")
    }
}

fn lower_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn pair_eq_with_charge<C: FnMut() -> bool>(
    left: &ModuleArtifactBidState,
    order_id: u64,
    bidder_agent_id: &str,
    work: &mut SemanticKeyBudget,
    cancelled: &mut C,
) -> Result<bool, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let charge = 8u64
        .checked_add(
            u64::try_from(bidder_agent_id.len())
                .map_err(|_| ValueTreeError::ResourceLimited("bid membership source comparison"))?,
        )
        .ok_or(ValueTreeError::ResourceLimited(
            "bid membership source comparison",
        ))?;
    if work.bytes_remaining < charge {
        return Err(ValueTreeError::ResourceLimited(
            "bid membership source comparison",
        ));
    }
    work.bytes_remaining -= charge;
    let matches = left.order_id == order_id && left.bidder_agent_id == bidder_agent_id;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(matches)
}

fn nth_member_id<C: FnMut() -> bool>(
    values: &[ModuleArtifactBidState],
    order_id: u64,
    bidder_agent_id: &str,
    ordinal: u64,
    work: &mut SemanticKeyBudget,
    cancelled: &mut C,
) -> Result<u64, ValueTreeError> {
    let mut seen = 0u64;
    for (source_ordinal, bid) in values.iter().enumerate() {
        if pair_eq_with_charge(bid, order_id, bidder_agent_id, work, cancelled)? {
            if seen == ordinal {
                return u64::try_from(source_ordinal)
                    .map_err(|_| ValueTreeError::ResourceLimited("bid membership source ordinal"));
            }
            seen = seen.checked_add(1).ok_or(ValueTreeError::ResourceLimited(
                "bid membership source ordinal",
            ))?;
        }
    }
    Err(ValueTreeError::Invalid(
        "bid membership source ordinal missing",
    ))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Builds one typed membership node with the caller's shared writer, source values, work and cancellation budgets."
)]
fn append_balanced_group<W, C>(
    descriptor: crate::incremental_projection::FieldDescriptor,
    root: &mut StateRootV1,
    values: &[ModuleArtifactBidState],
    order_id: u64,
    bidder_agent_id: &str,
    start: u64,
    end: u64,
    parent_id: u64,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<(u64, bid_index::BidIndexSummaryV1), MemberRecordError>
where
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
{
    if start == end {
        return Ok((
            bid_index::NO_BID_ID,
            bid_index::BidIndexSummaryV1 { size: 0, height: 0 },
        ));
    }
    let middle = start + (end - start) / 2;
    let bid_id = nth_member_id(
        values,
        order_id,
        bidder_agent_id,
        middle,
        context.key_work,
        &mut context.cancelled,
    )?;
    if bid_id == bid_index::NO_BID_ID {
        return Err(MemberRecordError::Invalid("bid membership ID sentinel"));
    }
    let (left_id, left) = append_balanced_group(
        descriptor,
        root,
        values,
        order_id,
        bidder_agent_id,
        start,
        middle,
        bid_id,
        context,
        remaining,
    )?;
    let (right_id, right) = append_balanced_group(
        descriptor,
        root,
        values,
        order_id,
        bidder_agent_id,
        middle + 1,
        end,
        bid_id,
        context,
        remaining,
    )?;
    let node = bid_index::BidIndexNodeV1 {
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
        &(("bid", bid_id), "index_node"),
        MemberPayloadRefV1::FixedU64x7(node.fields()),
        context,
        remaining,
    )?;
    Ok((bid_id, summary))
}

/// Append one group per distinct original pair without building an O(S)
/// temporary map. The bounded key-work budget also limits the source scans
/// used to find first occurrences and stable IDs in each group.
pub(crate) fn append_groups<W, C>(
    descriptor: crate::incremental_projection::FieldDescriptor,
    parent: &mut StateRootV1,
    values: &[ModuleArtifactBidState],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
) -> Result<(), MemberRecordError>
where
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
{
    for (ordinal, bid) in values.iter().enumerate() {
        let mut first = true;
        for earlier in &values[..ordinal] {
            if pair_eq_with_charge(
                earlier,
                bid.order_id,
                &bid.bidder_agent_id,
                context.key_work,
                &mut context.cancelled,
            )? {
                first = false;
                break;
            }
        }
        if !first {
            continue;
        }
        let digest = hash_pair_v1(
            bid.order_id,
            &bid.bidder_agent_id,
            context.memory,
            MemoryClass::Ordinary,
            context.key_work,
            &mut context.cancelled,
        )?;
        for earlier in &values[..ordinal] {
            if earlier.order_id == bid.order_id
                && earlier.bidder_agent_id.as_bytes() == bid.bidder_agent_id.as_bytes()
            {
                continue;
            }
            let earlier_digest = hash_pair_v1(
                earlier.order_id,
                &earlier.bidder_agent_id,
                context.memory,
                MemoryClass::Ordinary,
                context.key_work,
                &mut context.cancelled,
            )?;
            if earlier_digest == digest {
                return Err(MemberRecordError::Invalid(
                    "bid membership pair digest collision",
                ));
            }
        }
        let mut group = StateRootV1::empty(StateNamespaceV1::Business)
            .map_err(|_| MemberRecordError::Invalid("bid membership child namespace"))?;
        let mut group_length = 0u64;
        for member in values {
            if pair_eq_with_charge(
                member,
                bid.order_id,
                &bid.bidder_agent_id,
                context.key_work,
                &mut context.cancelled,
            )? {
                group_length = group_length
                    .checked_add(1)
                    .ok_or(MemberRecordError::Invalid("bid membership length overflow"))?;
            }
        }
        if group_length == 0 {
            return Err(MemberRecordError::Invalid("empty bid membership group"));
        }
        let (root_id, summary) = append_balanced_group(
            descriptor,
            &mut group,
            values,
            bid.order_id,
            &bid.bidder_agent_id,
            0,
            group_length,
            bid_index::NO_BID_ID,
            context,
            remaining,
        )?;
        if summary.size != group_length {
            return Err(MemberRecordError::Invalid("bid membership group size"));
        }
        append_member(
            descriptor,
            &mut group,
            "order_id",
            MemberPayloadRefV1::U64(bid.order_id),
            context,
            remaining,
        )?;
        append_member(
            descriptor,
            &mut group,
            "bidder_agent_id",
            MemberPayloadRefV1::Text(&bid.bidder_agent_id),
            context,
            remaining,
        )?;
        append_member(
            descriptor,
            &mut group,
            "length",
            MemberPayloadRefV1::U64(group_length),
            context,
            remaining,
        )?;
        append_member(
            descriptor,
            &mut group,
            "sequence_root_id",
            MemberPayloadRefV1::U64(root_id),
            context,
            remaining,
        )?;
        let reference = RecordRootRefV1::new(group_schema_v1(), digest, group)
            .map_err(|_| MemberRecordError::Invalid("bid membership root"))?;
        let key = GroupKeyV1::from_digest(&digest);
        append_member(
            descriptor,
            parent,
            key.as_str(),
            MemberPayloadRefV1::RecordRoot(&reference),
            context,
            remaining,
        )?;
    }
    Ok(())
}

/// Hash the exact original pair without allocating or retaining either key.
/// The byte work is charged before hashing and never refunded on cancellation.
pub(crate) fn hash_pair_v1<C: FnMut() -> bool>(
    order_id: u64,
    bidder_agent_id: &str,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut SemanticKeyBudget,
    cancelled: &mut C,
) -> Result<[u8; 32], ValueTreeError> {
    let _workspace = memory
        .reserve(4096, class)
        .map_err(ValueTreeError::Memory)?;
    let bidder_bytes = u64::try_from(bidder_agent_id.len())
        .map_err(|_| ValueTreeError::ResourceLimited("bid membership key bytes"))?;
    let charge = u64::try_from(PAIR_DIGEST_DOMAIN.len() + 8)
        .ok()
        .and_then(|fixed| fixed.checked_add(bidder_bytes))
        .ok_or(ValueTreeError::ResourceLimited("bid membership key bytes"))?;
    if work.bytes_remaining < charge {
        return Err(ValueTreeError::ResourceLimited("bid membership key bytes"));
    }
    work.bytes_remaining -= charge;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(PAIR_DIGEST_DOMAIN);
    hasher.update(&order_id.to_be_bytes());
    for chunk in bidder_agent_id.as_bytes().chunks(64 * 1024) {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        hasher.update(chunk);
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(*hasher.finalize().as_bytes())
}

pub(crate) fn group_schema_v1() -> [u8; 32] {
    *blake3::hash(GROUP_SCHEMA_DOMAIN).as_bytes()
}

pub(crate) struct LoadedGroupV1 {
    pub(crate) field_key: WorldFieldKeyV1,
    pub(crate) value_root: [u8; 32],
    pub(crate) child: StateRootV1,
}

pub(crate) fn group_field_key<R: ValueNodeReader, C: FnMut() -> bool>(
    descriptor: crate::incremental_projection::FieldDescriptor,
    digest: &[u8; 32],
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<(GroupKeyV1, WorldFieldKeyV1, u64), ValueTreeError> {
    let key = GroupKeyV1::from_digest(digest);
    let semantic_buffer = SemanticKeyBuffer::encode(key.as_str())?;
    let semantic = CanonicalSemanticKeyRefV1::new(semantic_buffer.as_slice())
        .map_err(ValueTreeError::Invalid)?;
    let hashed = hash_semantic_key_v1(
        &semantic,
        values.memory,
        values.class,
        key_work,
        &mut *cancelled,
    )
    .map_err(map_semantic_error)?;
    if hashed.encoded_bytes > semantic_limit || hashed.encoded_bytes > record_limit {
        return Err(ValueTreeError::ResourceLimited("bid membership group key"));
    }
    let field_key = WorldFieldKeyV1::new(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        Some(hashed.digest),
    )
    .map_err(ValueTreeError::Invalid)?;
    Ok((key, field_key, hashed.encoded_bytes))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Loads a group reference through the caller's shared key, value, class and cancellation budgets."
)]
pub(crate) fn load_group<R: ValueNodeReader, C: FnMut() -> bool>(
    parent: &StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    digest: &[u8; 32],
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<Option<LoadedGroupV1>, ValueTreeError> {
    let (_key, field_key, semantic_bytes) = group_field_key(
        descriptor,
        digest,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let Some(value_root) =
        incremental_record_tree::get(parent, &field_key, values, cancelled, RECORD_TREE_LABELS)?
    else {
        return Ok(None);
    };
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_profile_v1(
        &value_root,
        &field_key,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: semantic_limit,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: record_limit,
            },
        },
        &mut io::sink(),
        &mut &mut bytes[..],
        cancelled,
    )?;
    if info.payload_bytes != RECORD_ROOT_BYTES as u64 {
        return Err(ValueTreeError::Invalid("bid membership record-root length"));
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("bid membership record-root framing"))?;
    let child = reference
        .validate_binding(&group_schema_v1(), digest, StateNamespaceV1::Business)
        .map_err(|_| ValueTreeError::Invalid("bid membership record-root binding"))?
        .clone();
    Ok(Some(LoadedGroupV1 {
        field_key,
        value_root,
        child,
    }))
}
