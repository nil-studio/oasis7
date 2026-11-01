use super::*;

#[path = "incremental_module_marketplace_bid_index_mutation_delete.rs"]
mod delete;
pub(crate) use delete::delete_at;

pub(crate) struct LoadedBidIndexNodeV1 {
    pub id: u64,
    pub node: BidIndexNodeV1,
    root: [u8; 32],
    stored_node: BidIndexNodeV1,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps the independent value reader, semantic-key budget, and cancellation boundary explicit."
)]
pub(crate) fn load_node<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    bid_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    cancelled: &mut C,
) -> Result<LoadedBidIndexNodeV1, ValueTreeError> {
    if bid_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid index sentinel"));
    }
    let (node, node_root) = super::read_node_with_root(
        root,
        descriptor,
        bid_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    Ok(LoadedBidIndexNodeV1 {
        id: bid_id,
        node,
        root: node_root,
        stored_node: node,
    })
}

fn member_error(error: MemberRecordError) -> ValueTreeError {
    match error {
        MemberRecordError::Key(error) => super::super::map_semantic_error(error),
        MemberRecordError::Value(error) => error,
        MemberRecordError::Invalid(reason) => ValueTreeError::Invalid(reason),
        MemberRecordError::Io(_) => ValueTreeError::Invalid("marketplace bid index write"),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Carries the read/write stores, budgets, and cancellation callbacks independently through one member edit."
)]
pub(crate) fn write_u64_member<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    key: &impl crate::incremental_semantic_key::SemanticKeyV1,
    before: Option<[u8; 32]>,
    after: Option<u64>,
    values: &mut ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<Option<[u8; 32]>, ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if cancelled() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "marketplace bid index writes",
        ))?;
    let value_root = if let Some(value) = after {
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || cancelled(),
        };
        let record = write_member_record_v1(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            key,
            MemberPayloadRefV1::U64(value),
            &mut output,
        )
        .map_err(member_error)?;
        Some((record.field_key, record.value_root))
    } else {
        None
    };
    let field_key = if let Some((field_key, _)) = &value_root {
        field_key.clone()
    } else {
        let buffer = SemanticKeyBuffer::encode(key)?;
        let semantic =
            CanonicalSemanticKeyRefV1::new(buffer.as_slice()).map_err(ValueTreeError::Invalid)?;
        let hash = hash_semantic_key_v1(
            &semantic,
            write.memory,
            MemoryClass::Ordinary,
            write.key_work,
            &mut *cancelled,
        )
        .map_err(super::super::map_semantic_error)?;
        WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Entry, Some(hash.digest))
            .map_err(ValueTreeError::Invalid)?
    };
    let next_root = value_root.as_ref().map(|(_, value)| *value);
    let mut page_context = MemberRecordWriteContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        key_work: &mut *write.key_work,
        limits: write.limits,
        cancelled: || false,
    };
    *root = crate::incremental_record_tree::edit(
        root.clone(),
        &field_key,
        (before, next_root),
        values,
        &mut page_context,
        &mut *cancelled,
        super::super::tree_policy(write.limits.record_bytes),
    )?;
    if cancelled() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(next_root)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Carries the read/write stores, budgets, and cancellation callbacks independently through one node edit."
)]
fn write_node_member<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    bid_id: u64,
    before: Option<[u8; 32]>,
    after: Option<BidIndexNodeV1>,
    values: &mut ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<Option<[u8; 32]>, ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if cancelled() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if bid_id == super::NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid index ID sentinel"));
    }
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "marketplace bid index writes",
        ))?;
    let value_root = if let Some(node) = after {
        node.summary()?;
        let semantic_key = (("bid", bid_id), super::BID_INDEX_NODE_FIELD);
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || cancelled(),
        };
        let record = write_member_record_v1(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            &semantic_key,
            MemberPayloadRefV1::FixedU64x7(node.fields()),
            &mut output,
        )
        .map_err(member_error)?;
        Some((record.field_key, record.value_root))
    } else {
        None
    };
    let field_key = if let Some((key, _)) = &value_root {
        key.clone()
    } else {
        super::node_field_key(
            descriptor,
            bid_id,
            values,
            write.key_work,
            write.limits.semantic_bytes,
            write.limits.record_bytes,
            &mut *cancelled,
        )?
        .0
    };
    let next_root = value_root.map(|(_, value)| value);
    let mut page_context = MemberRecordWriteContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        key_work: &mut *write.key_work,
        limits: write.limits,
        cancelled: || false,
    };
    *root = crate::incremental_record_tree::edit(
        root.clone(),
        &field_key,
        (before, next_root),
        values,
        &mut page_context,
        &mut *cancelled,
        super::super::tree_policy(write.limits.record_bytes),
    )?;
    if cancelled() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(next_root)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preserves distinct read, write, record, and cancellation limits while storing one tree node."
)]
fn store_node<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    mut loaded: LoadedBidIndexNodeV1,
    next: BidIndexNodeV1,
    values: &mut ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<LoadedBidIndexNodeV1, ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    next.summary()?;
    if loaded.stored_node != next {
        loaded.root = write_node_member(
            root,
            descriptor,
            loaded.id,
            Some(loaded.root),
            Some(next),
            values,
            write,
            cancelled,
            remaining,
        )?
        .ok_or(ValueTreeError::Invalid("marketplace bid index update root"))?;
    }
    loaded.node = next;
    loaded.stored_node = next;
    Ok(loaded)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preserves distinct read, write, record, and cancellation limits while creating one tree node."
)]
fn create_node<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    id: u64,
    parent_id: u64,
    node: BidIndexNodeV1,
    values: &mut ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<LoadedBidIndexNodeV1, ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid index ID sentinel"));
    }
    node.summary()?;
    let node = BidIndexNodeV1 { parent_id, ..node };
    let node_root = write_node_member(
        root,
        descriptor,
        id,
        None,
        Some(node),
        values,
        write,
        cancelled,
        remaining,
    )?
    .ok_or(ValueTreeError::Invalid("marketplace bid index create root"))?;
    Ok(LoadedBidIndexNodeV1 {
        id,
        node,
        root: node_root,
        stored_node: node,
    })
}

fn delete_node<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    node: LoadedBidIndexNodeV1,
    values: &mut ValueReadContext<'_, R>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<(), ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    write_node_member(
        root,
        descriptor,
        node.id,
        Some(node.root),
        None,
        values,
        write,
        cancelled,
        remaining,
    )?;
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Carries the parent-link read/write proof and independent cancellation budgets through rebalancing."
)]
fn set_parent<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    bid_id: u64,
    expected_parent: u64,
    next_parent: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<(), ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if bid_id == super::NO_BID_ID {
        return Ok(());
    }
    let mut node = load_node(
        root,
        descriptor,
        bid_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        cancelled,
    )?;
    if node.node.parent_id != expected_parent {
        return Err(ValueTreeError::Invalid(
            "marketplace bid AVL parent mismatch",
        ));
    }
    node.node.parent_id = next_parent;
    let next = node.node;
    store_node(
        root, descriptor, node, next, values, write, cancelled, remaining,
    )?;
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps both tree readers, the writer, record budget, and cancellation boundaries explicit for a rotation."
)]
fn rotate_left<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    mut x: LoadedBidIndexNodeV1,
    mut y: LoadedBidIndexNodeV1,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<LoadedBidIndexNodeV1, ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if x.node.right_id != y.id {
        return Err(ValueTreeError::Invalid("marketplace bid AVL rotation link"));
    }
    let parent_id = x.node.parent_id;
    if y.node.parent_id != x.id {
        return Err(ValueTreeError::Invalid("marketplace bid AVL parent link"));
    }
    let beta_id = y.node.left_id;
    x.node.right_id = y.node.left_id;
    x.node.right_size = y.node.left_size;
    x.node.right_height = y.node.left_height;
    x.node.parent_id = y.id;
    let x_node = x.node;
    let x = store_node(
        root, descriptor, x, x_node, values, write, cancelled, remaining,
    )?;
    let x_summary = x.node.summary()?;
    y.node.left_id = x.id;
    y.node.left_size = x_summary.size;
    y.node.left_height = x_summary.height;
    y.node.parent_id = parent_id;
    if beta_id != super::NO_BID_ID {
        set_parent(
            root,
            descriptor,
            beta_id,
            y.id,
            x.id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )?;
    }
    let y_node = y.node;
    store_node(
        root, descriptor, y, y_node, values, write, cancelled, remaining,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps both tree readers, the writer, record budget, and cancellation boundaries explicit for a rotation."
)]
fn rotate_right<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    mut x: LoadedBidIndexNodeV1,
    mut y: LoadedBidIndexNodeV1,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<LoadedBidIndexNodeV1, ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if x.node.left_id != y.id {
        return Err(ValueTreeError::Invalid("marketplace bid AVL rotation link"));
    }
    let parent_id = x.node.parent_id;
    if y.node.parent_id != x.id {
        return Err(ValueTreeError::Invalid("marketplace bid AVL parent link"));
    }
    let beta_id = y.node.right_id;
    x.node.left_id = y.node.right_id;
    x.node.left_size = y.node.right_size;
    x.node.left_height = y.node.right_height;
    x.node.parent_id = y.id;
    let x_node = x.node;
    let x = store_node(
        root, descriptor, x, x_node, values, write, cancelled, remaining,
    )?;
    let x_summary = x.node.summary()?;
    y.node.right_id = x.id;
    y.node.right_size = x_summary.size;
    y.node.right_height = x_summary.height;
    y.node.parent_id = parent_id;
    if beta_id != super::NO_BID_ID {
        set_parent(
            root,
            descriptor,
            beta_id,
            y.id,
            x.id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )?;
    }
    let y_node = y.node;
    store_node(
        root, descriptor, y, y_node, values, write, cancelled, remaining,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps the read/write resource boundaries explicit while applying bounded AVL rotations."
)]
fn rebalance<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    mut node: LoadedBidIndexNodeV1,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<LoadedBidIndexNodeV1, ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    node.node.structural_summary()?;
    if node.node.left_height > node.node.right_height
        && node.node.left_height - node.node.right_height > 1
    {
        let left = load_node(
            root,
            descriptor,
            node.node.left_id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        if left.node.left_height >= left.node.right_height {
            return rotate_right(
                root,
                descriptor,
                node,
                left,
                values,
                key_work,
                semantic_limit,
                record_limit,
                write,
                cancelled,
                remaining,
            );
        }
        let pivot = load_node(
            root,
            descriptor,
            left.node.right_id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        let left = rotate_left(
            root,
            descriptor,
            left,
            pivot,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )?;
        let left_summary = left.node.summary()?;
        node.node.left_id = left.id;
        node.node.left_size = left_summary.size;
        node.node.left_height = left_summary.height;
        rotate_right(
            root,
            descriptor,
            node,
            left,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )
    } else if node.node.right_height > node.node.left_height
        && node.node.right_height - node.node.left_height > 1
    {
        let right = load_node(
            root,
            descriptor,
            node.node.right_id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        if right.node.right_height >= right.node.left_height {
            return rotate_left(
                root,
                descriptor,
                node,
                right,
                values,
                key_work,
                semantic_limit,
                record_limit,
                write,
                cancelled,
                remaining,
            );
        }
        let pivot = load_node(
            root,
            descriptor,
            right.node.left_id,
            values,
            key_work,
            semantic_limit,
            record_limit,
            cancelled,
        )?;
        let right = rotate_right(
            root,
            descriptor,
            right,
            pivot,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )?;
        let right_summary = right.node.summary()?;
        node.node.right_id = right.id;
        node.node.right_size = right_summary.size;
        node.node.right_height = right_summary.height;
        rotate_left(
            root,
            descriptor,
            node,
            right,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
        )
    } else {
        let node_fields = node.node;
        store_node(
            root,
            descriptor,
            node,
            node_fields,
            values,
            write,
            cancelled,
            remaining,
        )
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Carries recursive rank, parent, read/write resources, and cancellation limits without hidden resets."
)]
fn insert_at_inner<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    parent_id: u64,
    current_id: u64,
    rank: u64,
    new_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
    path: &mut [u64; MAX_BID_INDEX_DEPTH],
    depth: usize,
) -> Result<LoadedBidIndexNodeV1, ValueTreeError>
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
        if rank != 0 {
            return Err(ValueTreeError::Invalid("marketplace bid insert rank"));
        }
        return create_node(
            root,
            descriptor,
            new_id,
            parent_id,
            BidIndexNodeV1::LEAF,
            values,
            write,
            cancelled,
            remaining,
        );
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
    let size = current.node.summary()?.size;
    if rank > size {
        return Err(ValueTreeError::Invalid("marketplace bid insert rank"));
    }
    if rank <= current.node.left_size {
        let child = insert_at_inner(
            root,
            descriptor,
            current.id,
            current.node.left_id,
            rank,
            new_id,
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
        let summary = child.node.summary()?;
        current.node.left_id = child.id;
        current.node.left_size = summary.size;
        current.node.left_height = summary.height;
    } else {
        let child = insert_at_inner(
            root,
            descriptor,
            current.id,
            current.node.right_id,
            rank - current.node.left_size - 1,
            new_id,
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
        let summary = child.node.summary()?;
        current.node.right_id = child.id;
        current.node.right_size = summary.size;
        current.node.right_height = summary.height;
    }
    rebalance(
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
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Keeps the public insertion boundary's parent root, rank, typed resources, and shared budgets explicit."
)]
pub(crate) fn insert_at<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    current_root_id: u64,
    rank: u64,
    new_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
) -> Result<LoadedBidIndexNodeV1, ValueTreeError>
where
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if new_id == NO_BID_ID {
        return Err(ValueTreeError::Invalid("marketplace bid ID sentinel"));
    }
    let mut path = [NO_BID_ID; MAX_BID_INDEX_DEPTH];
    insert_at_inner(
        root,
        descriptor,
        NO_BID_ID,
        current_root_id,
        rank,
        new_id,
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
