//! Stable-ID removal keeps primary order and membership links in one update.
use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps a member deletion's read/write classes, limits, and cancellation boundaries explicit."
)]
pub(super) fn delete_bid_member<R, W, C, CW>(
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

#[expect(
    clippy::too_many_arguments,
    reason = "Deletes one authenticated stable ID while sharing the primary root and transaction budgets."
)]
pub(super) fn delete_stable_bid<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: FieldDescriptor,
    stable_id: u64,
    order_id: u64,
    bidder_agent_id: &str,
    sequence_root_id: u64,
    length: u64,
    length_root: [u8; 32],
    sequence_root_field_root: [u8; 32],
    values: &mut ValueReadContext<'_, R>,
    read_key_work: &mut SemanticKeyBudget,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining_records: &mut u64,
) -> Result<(u64, u64, [u8; 32], [u8; 32]), ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    let ordinal = bid_index::ordinal_of_id(
        root,
        descriptor,
        sequence_root_id,
        stable_id,
        values,
        read_key_work,
        write.limits.semantic_bytes,
        write.limits.record_bytes,
        cancelled,
    )?;
    let (actual_order_id, _) = bid_index::read_u64_member(
        root,
        descriptor,
        &(("bid", stable_id), "order_id"),
        values,
        read_key_work,
        write.limits.semantic_bytes,
        write.limits.record_bytes,
        cancelled,
    )?;
    if actual_order_id != order_id
        || !read_bid_text_equals(
            root,
            descriptor,
            stable_id,
            bidder_agent_id,
            values,
            read_key_work,
            write.limits.semantic_bytes,
            write.limits.record_bytes,
            cancelled,
        )?
    {
        return Err(ValueTreeError::Invalid(
            "marketplace bid membership source mismatch",
        ));
    }
    let marker = (("bid", stable_id), "present");
    let (marker_root, marker_key, marker_semantic) = require_member_root(
        root,
        descriptor,
        &marker,
        values,
        read_key_work,
        write.limits.semantic_bytes,
        write.limits.record_bytes,
        cancelled,
    )?;
    let marker_info = read_member_record_profile_v1(
        &marker_root,
        &marker_key,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Unit,
            semantic_bytes: marker_semantic,
            limits: write.limits,
        },
        &mut io::sink(),
        &mut io::sink(),
        &mut *cancelled,
    )?;
    if marker_info.payload_bytes != 0 {
        return Err(ValueTreeError::Invalid("marketplace bid marker profile"));
    }
    let (next_root, summary) = bid_index::delete_at(
        root,
        descriptor,
        sequence_root_id,
        ordinal,
        values,
        read_key_work,
        write.limits.semantic_bytes,
        write.limits.record_bytes,
        write,
        &mut *cancelled,
        remaining_records,
    )?;
    let next_length = length
        .checked_sub(1)
        .ok_or(ValueTreeError::Invalid("marketplace bid length underflow"))?;
    if summary.size != next_length || (next_length == 0) != (next_root == bid_index::NO_BID_ID) {
        return Err(ValueTreeError::Invalid("marketplace bid delete size"));
    }
    for name in BID_RECORD_FIELDS {
        delete_bid_member(
            root,
            descriptor,
            stable_id,
            name,
            values,
            write,
            cancelled,
            remaining_records,
        )?;
    }
    let next_length_root = update_scalar(
        root,
        descriptor,
        "length",
        length_root,
        next_length,
        values,
        write,
        cancelled,
        remaining_records,
    )?;
    let next_sequence_root_field = update_scalar(
        root,
        descriptor,
        "sequence_root_id",
        sequence_root_field_root,
        next_root,
        values,
        write,
        cancelled,
        remaining_records,
    )?;
    Ok((
        next_root,
        next_length,
        next_length_root,
        next_sequence_root_field,
    ))
}
