use super::*;
use crate::incremental_module_marketplace::bid_membership::{
    LoadedGroupV1, group_field_key, group_schema_v1, hash_pair_v1, load_group,
};

pub(super) fn member_error(error: MemberRecordError) -> ValueTreeError {
    match error {
        MemberRecordError::Key(error) => super::super::map_semantic_error(error),
        MemberRecordError::Value(error) => error,
        MemberRecordError::Invalid(reason) => ValueTreeError::Invalid(reason),
        MemberRecordError::Io(_) => ValueTreeError::Invalid("marketplace membership write"),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Updates one original text payload under the same root, read/write budgets, and cancellation pair."
)]
pub(super) fn update_bidder_text<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: FieldDescriptor,
    stable_id: u64,
    bidder_agent_id: &str,
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
    if cancelled() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    *remaining_records = remaining_records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("marketplace bid records"))?;
    let semantic = (("bid", stable_id), "bidder_agent_id");
    let (before, key, _) = require_member_root(
        root,
        descriptor,
        &semantic,
        values,
        write.key_work,
        write.limits.semantic_bytes,
        write.limits.record_bytes,
        cancelled,
    )?;
    let record = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        &semantic,
        MemberPayloadRefV1::Text(bidder_agent_id),
        write,
    )
    .map_err(member_error)?;
    if record.field_key != key {
        return Err(ValueTreeError::Invalid("marketplace bidder writer key"));
    }
    *root = incremental_record_tree::edit(
        root.clone(),
        &key,
        (Some(before), Some(record.value_root)),
        values,
        write,
        cancelled,
        super::tree_policy(write.limits.record_bytes),
    )?;
    if cancelled() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Reads an owner-bound text member using the caller's shared field and value budgets."
)]
fn text_member_equals<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: FieldDescriptor,
    semantic: &impl SemanticKeyV1,
    expected: &str,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<bool, ValueTreeError> {
    let (value_root, key, semantic_bytes) = super::require_member_root(
        root,
        descriptor,
        semantic,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let mut compare = ExactText {
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

struct ExactText<'a> {
    expected: &'a [u8],
    offset: usize,
    equal: bool,
}

impl Write for ExactText<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(end) = self.offset.checked_add(bytes.len()) else {
            self.equal = false;
            return Err(io::Error::other(
                "marketplace group text comparison overflow",
            ));
        };
        if end > self.expected.len() || self.expected[self.offset..end] != *bytes {
            self.equal = false;
            return Err(io::Error::other(
                "marketplace group text comparison mismatch",
            ));
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
    reason = "Loads and verifies the exact original pair under the active shared key, value and cancellation budgets."
)]
fn load_exact_group<R: ValueNodeReader, C: FnMut() -> bool>(
    parent: &StateRootV1,
    descriptor: FieldDescriptor,
    order_id: u64,
    bidder_agent_id: &str,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<([u8; 32], Option<LoadedGroupV1>), ValueTreeError> {
    let digest = hash_pair_v1(
        order_id,
        bidder_agent_id,
        values.memory,
        values.class,
        key_work,
        cancelled,
    )?;
    let group = load_group(
        parent,
        descriptor,
        &digest,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    if let Some(group) = &group {
        let (stored_order, _) = bid_index::read_u64_member(
            &group.child,
            descriptor,
            "order_id",
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        if stored_order != order_id
            || !text_member_equals(
                &group.child,
                descriptor,
                &"bidder_agent_id",
                bidder_agent_id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                cancelled,
            )?
        {
            return Err(ValueTreeError::Invalid(
                "marketplace bid group pair binding",
            ));
        }
    }
    Ok((digest, group))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Maintains one secondary pair group with the caller's same typed read/write budgets and cancel state."
)]
pub(super) fn mutate_pair_member<R, W, C, CW>(
    parent: &mut StateRootV1,
    descriptor: FieldDescriptor,
    order_id: u64,
    bidder_agent_id: &str,
    bid_id: u64,
    insert: bool,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
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
    if bid_id == bid_index::NO_BID_ID {
        return Err(ValueTreeError::Invalid(
            "marketplace membership ID sentinel",
        ));
    }
    let (digest, loaded) = load_exact_group(
        parent,
        descriptor,
        order_id,
        bidder_agent_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let mut child = if let Some(group) = &loaded {
        let (length, _) = bid_index::read_u64_member(
            &group.child,
            descriptor,
            "length",
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        let (root_id, _) = bid_index::read_u64_member(
            &group.child,
            descriptor,
            "sequence_root_id",
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        if length == 0 || root_id == bid_index::NO_BID_ID {
            return Err(ValueTreeError::Invalid(
                "marketplace membership empty group",
            ));
        }
        group.child.clone()
    } else if insert {
        StateRootV1::empty(StateNamespaceV1::Business)
            .map_err(|_| ValueTreeError::Invalid("marketplace membership namespace"))?
    } else {
        return Err(ValueTreeError::Invalid(
            "marketplace membership group missing",
        ));
    };

    if let Some(group) = &loaded {
        let (length, length_root) = bid_index::read_u64_member(
            &group.child,
            descriptor,
            "length",
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        let (root_id, root_value) = bid_index::read_u64_member(
            &group.child,
            descriptor,
            "sequence_root_id",
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        if insert {
            let (rank, found) = bid_index::rank_of_id(
                &child,
                descriptor,
                root_id,
                length,
                bid_id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                cancelled,
            )?;
            if found {
                return Err(ValueTreeError::Invalid(
                    "marketplace membership ID already present",
                ));
            }
            let next_length = length
                .checked_add(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "marketplace membership length",
                ))?;
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || (write.cancelled)(),
            };
            let new_root = bid_index::insert_at(
                &mut child,
                descriptor,
                root_id,
                rank,
                bid_id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                &mut output,
                cancelled,
                remaining_records,
            )?;
            if new_root.node.summary()?.size != next_length {
                return Err(ValueTreeError::Invalid(
                    "marketplace membership insert size",
                ));
            }
            super::update_scalar(
                &mut child,
                descriptor,
                "length",
                length_root,
                next_length,
                values,
                &mut output,
                cancelled,
                remaining_records,
            )?;
            super::update_scalar(
                &mut child,
                descriptor,
                "sequence_root_id",
                root_value,
                new_root.id,
                values,
                &mut output,
                cancelled,
                remaining_records,
            )?;
        } else {
            let (rank, found) = bid_index::rank_of_id(
                &child,
                descriptor,
                root_id,
                length,
                bid_id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                cancelled,
            )?;
            if !found
                || bid_index::select_ordinal(
                    &child,
                    descriptor,
                    rank,
                    values,
                    key_work,
                    semantic_limit,
                    record_limit,
                    cancelled,
                )? != bid_id
            {
                return Err(ValueTreeError::Invalid("marketplace membership ID missing"));
            }
            let next_length = length.checked_sub(1).ok_or(ValueTreeError::Invalid(
                "marketplace membership length underflow",
            ))?;
            if next_length == 0 {
                return store_group_reference(
                    parent,
                    descriptor,
                    &digest,
                    group.field_key.clone(),
                    Some(group.value_root),
                    None,
                    values,
                    key_work,
                    semantic_limit,
                    record_limit,
                    write,
                    cancelled,
                    remaining_records,
                );
            }
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || (write.cancelled)(),
            };
            let (new_root, summary) = bid_index::delete_at(
                &mut child,
                descriptor,
                root_id,
                rank,
                values,
                key_work,
                semantic_limit,
                record_limit,
                &mut output,
                cancelled,
                remaining_records,
            )?;
            if summary.size != next_length || new_root == bid_index::NO_BID_ID {
                return Err(ValueTreeError::Invalid(
                    "marketplace membership delete size",
                ));
            }
            super::update_scalar(
                &mut child,
                descriptor,
                "length",
                length_root,
                next_length,
                values,
                &mut output,
                cancelled,
                remaining_records,
            )?;
            super::update_scalar(
                &mut child,
                descriptor,
                "sequence_root_id",
                root_value,
                new_root,
                values,
                &mut output,
                cancelled,
                remaining_records,
            )?;
        }
    } else {
        let node = bid_index::BidIndexNodeV1::LEAF;
        super::append_member(
            descriptor,
            &mut child,
            &(("bid", bid_id), "index_node"),
            MemberPayloadRefV1::FixedU64x7(node.fields()),
            write,
            remaining_records,
        )
        .map_err(member_error)?;
        super::append_member(
            descriptor,
            &mut child,
            "order_id",
            MemberPayloadRefV1::U64(order_id),
            write,
            remaining_records,
        )
        .map_err(member_error)?;
        super::append_member(
            descriptor,
            &mut child,
            "bidder_agent_id",
            MemberPayloadRefV1::Text(bidder_agent_id),
            write,
            remaining_records,
        )
        .map_err(member_error)?;
        super::append_member(
            descriptor,
            &mut child,
            "length",
            MemberPayloadRefV1::U64(1),
            write,
            remaining_records,
        )
        .map_err(member_error)?;
        super::append_member(
            descriptor,
            &mut child,
            "sequence_root_id",
            MemberPayloadRefV1::U64(bid_id),
            write,
            remaining_records,
        )
        .map_err(member_error)?;
    }

    let (_, field_key, _) = group_field_key(
        descriptor,
        &digest,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let before = loaded.as_ref().map(|group| group.value_root);
    store_group_reference(
        parent,
        descriptor,
        &digest,
        field_key,
        before,
        Some(child),
        values,
        key_work,
        semantic_limit,
        record_limit,
        write,
        cancelled,
        remaining_records,
    )?;
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Reads one indexed pair's first stable ID under the caller's shared resource budgets."
)]
pub(super) fn first_pair_member_id<R: ValueNodeReader, C: FnMut() -> bool>(
    parent: &StateRootV1,
    descriptor: FieldDescriptor,
    order_id: u64,
    bidder_agent_id: &str,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<Option<(u64, u64)>, ValueTreeError> {
    let (_, Some(group)) = load_exact_group(
        parent,
        descriptor,
        order_id,
        bidder_agent_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?
    else {
        return Ok(None);
    };
    let (length, _) = bid_index::read_u64_member(
        &group.child,
        descriptor,
        "length",
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    let (root_id, _) = bid_index::read_u64_member(
        &group.child,
        descriptor,
        "sequence_root_id",
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    if length == 0 || root_id == bid_index::NO_BID_ID {
        return Err(ValueTreeError::Invalid(
            "marketplace membership empty group",
        ));
    }
    let id = bid_index::select_ordinal(
        &group.child,
        descriptor,
        0,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    Ok(Some((length, id)))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stores a group-root transition with the caller's field key, source and writer budgets."
)]
fn store_group_reference<R, W, C, CW>(
    parent: &mut StateRootV1,
    descriptor: FieldDescriptor,
    digest: &[u8; 32],
    field_key: WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    after: Option<StateRootV1>,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
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
    if cancelled() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    *remaining_records =
        remaining_records
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "marketplace membership records",
            ))?;
    let (expected_key, actual_key, _) = group_field_key(
        descriptor,
        digest,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    if actual_key != field_key {
        return Err(ValueTreeError::Invalid("marketplace group reference key"));
    }
    let after_value = if let Some(child) = after {
        let reference = RecordRootRefV1::new(group_schema_v1(), *digest, child)
            .map_err(|_| ValueTreeError::Invalid("marketplace group reference root"))?;
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || (write.cancelled)(),
        };
        let record = write_member_record_v1(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            expected_key.as_str(),
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut output,
        )
        .map_err(member_error)?;
        if record.field_key != field_key {
            return Err(ValueTreeError::Invalid("marketplace group writer key"));
        }
        Some(record.value_root)
    } else {
        None
    };
    let tree_limits = write.limits;
    let mut page_context = MemberRecordWriteContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        key_work: &mut *write.key_work,
        limits: write.limits,
        cancelled: || (write.cancelled)(),
    };
    *parent = incremental_record_tree::edit(
        parent.clone(),
        &field_key,
        (before, after_value),
        values,
        &mut page_context,
        cancelled,
        super::tree_policy(tree_limits.record_bytes),
    )?;
    if cancelled() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
