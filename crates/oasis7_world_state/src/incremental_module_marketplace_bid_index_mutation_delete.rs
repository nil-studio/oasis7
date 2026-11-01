use super::*;

fn cached_child_summary(
    id: u64,
    size: u64,
    height: u64,
) -> Result<BidIndexSummaryV1, ValueTreeError> {
    if id == NO_BID_ID {
        if size != 0 || height != 0 {
            return Err(ValueTreeError::Invalid("marketplace bid AVL empty child"));
        }
        Ok(BidIndexSummaryV1 { size: 0, height: 0 })
    } else if size == 0 || height == 0 {
        Err(ValueTreeError::Invalid("marketplace bid AVL child summary"))
    } else {
        Ok(BidIndexSummaryV1 { size, height })
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Carries recursive parent identity and the caller's shared read/write budgets through deletion."
)]
fn remove_min_inner<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    parent_id: u64,
    current_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
    path: &mut [u64; MAX_BID_INDEX_DEPTH],
    depth: usize,
) -> Result<(u64, BidIndexSummaryV1, LoadedBidIndexNodeV1), ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if depth >= MAX_BID_INDEX_DEPTH {
        return Err(ValueTreeError::ResourceLimited(
            "marketplace bid index depth",
        ));
    }
    if current_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid remove minimum"));
    }
    if path[..depth].contains(&current_id) {
        return Err(ValueTreeError::Invalid("marketplace bid index cycle"));
    }
    path[depth] = current_id;
    let mut current = load_node(
        root,
        descriptor,
        current_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    if current.node.parent_id != parent_id {
        return Err(ValueTreeError::Invalid(
            "marketplace bid AVL parent mismatch",
        ));
    }
    if current.node.left_id == NO_BID_ID {
        let right = cached_child_summary(
            current.node.right_id,
            current.node.right_size,
            current.node.right_height,
        )?;
        set_parent(
            root,
            descriptor,
            current.node.right_id,
            current.id,
            parent_id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )?;
        return Ok((current.node.right_id, right, current));
    }
    let (left_id, left, removed) = remove_min_inner(
        root,
        descriptor,
        current.id,
        current.node.left_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        write,
        cancelled,
        remaining,
        path,
        depth + 1,
    )?;
    current.node.left_id = left_id;
    current.node.left_size = left.size;
    current.node.left_height = left.height;
    let current = rebalance(
        root,
        descriptor,
        current,
        values,
        key_work,
        semantic_limit,
        record_limit,
        write,
        cancelled,
        remaining,
    )?;
    let summary = current.node.summary()?;
    Ok((current.id, summary, removed))
}

#[expect(
    clippy::too_many_arguments,
    reason = "Carries recursive rank, parent identity, and shared read/write cancellation budgets without reset."
)]
fn delete_at_inner<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    parent_id: u64,
    current_id: u64,
    rank: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
    path: &mut [u64; MAX_BID_INDEX_DEPTH],
    depth: usize,
) -> Result<(u64, BidIndexSummaryV1), ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if depth >= MAX_BID_INDEX_DEPTH {
        return Err(ValueTreeError::ResourceLimited(
            "marketplace bid index depth",
        ));
    }
    if current_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid delete rank"));
    }
    if path[..depth].contains(&current_id) {
        return Err(ValueTreeError::Invalid("marketplace bid index cycle"));
    }
    path[depth] = current_id;
    let mut current = load_node(
        root,
        descriptor,
        current_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    if current.node.parent_id != parent_id {
        return Err(ValueTreeError::Invalid(
            "marketplace bid AVL parent mismatch",
        ));
    }
    let current_summary = current.node.summary()?;
    if rank >= current_summary.size {
        return Err(ValueTreeError::Invalid("marketplace bid delete rank"));
    }
    if rank < current.node.left_size {
        let (left_id, left) = delete_at_inner(
            root,
            descriptor,
            current.id,
            current.node.left_id,
            rank,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
            path,
            depth + 1,
        )?;
        current.node.left_id = left_id;
        current.node.left_size = left.size;
        current.node.left_height = left.height;
        let current = rebalance(
            root,
            descriptor,
            current,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )?;
        return Ok((current.id, current.node.summary()?));
    }
    if rank > current.node.left_size {
        let (right_id, right) = delete_at_inner(
            root,
            descriptor,
            current.id,
            current.node.right_id,
            rank - current.node.left_size - 1,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
            path,
            depth + 1,
        )?;
        current.node.right_id = right_id;
        current.node.right_size = right.size;
        current.node.right_height = right.height;
        let current = rebalance(
            root,
            descriptor,
            current,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )?;
        return Ok((current.id, current.node.summary()?));
    }

    let left = cached_child_summary(
        current.node.left_id,
        current.node.left_size,
        current.node.left_height,
    )?;
    let right = cached_child_summary(
        current.node.right_id,
        current.node.right_size,
        current.node.right_height,
    )?;
    let left_id = current.node.left_id;
    let right_id = current.node.right_id;
    match (
        current.node.left_id == NO_BID_ID,
        current.node.right_id == NO_BID_ID,
    ) {
        (true, true) => {
            delete_node(
                root, descriptor, current, values, write, cancelled, remaining,
            )?;
            Ok((NO_BID_ID, BidIndexSummaryV1 { size: 0, height: 0 }))
        }
        (true, false) => {
            set_parent(
                root,
                descriptor,
                right_id,
                current.id,
                current.node.parent_id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                write,
                cancelled,
                remaining,
            )?;
            delete_node(
                root, descriptor, current, values, write, cancelled, remaining,
            )?;
            Ok((right_id, right))
        }
        (false, true) => {
            set_parent(
                root,
                descriptor,
                left_id,
                current.id,
                current.node.parent_id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                write,
                cancelled,
                remaining,
            )?;
            delete_node(
                root, descriptor, current, values, write, cancelled, remaining,
            )?;
            Ok((left_id, left))
        }
        (false, false) => {
            let (right_id, right, mut successor) = remove_min_inner(
                root,
                descriptor,
                current.id,
                current.node.right_id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                write,
                cancelled,
                remaining,
                path,
                depth + 1,
            )?;
            successor.node.left_id = current.node.left_id;
            successor.node.left_size = left.size;
            successor.node.left_height = left.height;
            successor.node.right_id = right_id;
            successor.node.right_size = right.size;
            successor.node.right_height = right.height;
            successor.node.parent_id = current.node.parent_id;
            set_parent(
                root,
                descriptor,
                current.node.left_id,
                current.id,
                successor.id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                write,
                cancelled,
                remaining,
            )?;
            set_parent(
                root,
                descriptor,
                right_id,
                current.id,
                successor.id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                write,
                cancelled,
                remaining,
            )?;
            let successor_fields = successor.node;
            let successor = store_node(
                root,
                descriptor,
                successor,
                successor_fields,
                values,
                write,
                cancelled,
                remaining,
            )?;
            delete_node(
                root, descriptor, current, values, write, cancelled, remaining,
            )?;
            Ok((successor.id, successor.node.summary()?))
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps the public deletion boundary's root, rank, typed resources, and shared budgets explicit."
)]
pub(crate) fn delete_at<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    current_root_id: u64,
    rank: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<(u64, BidIndexSummaryV1), ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if current_root_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid delete empty"));
    }
    let mut path = [NO_BID_ID; MAX_BID_INDEX_DEPTH];
    delete_at_inner(
        root,
        descriptor,
        NO_BID_ID,
        current_root_id,
        rank,
        values,
        key_work,
        semantic_limit,
        record_limit,
        write,
        cancelled,
        remaining,
        &mut path,
        0,
    )
}
