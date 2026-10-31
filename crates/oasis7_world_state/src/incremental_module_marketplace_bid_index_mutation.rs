use super::*;

pub(crate) struct LoadedBidIndexNodeV1 {
    pub id: u64,
    pub node: BidIndexNodeV1,
    roots: [[u8; 32]; 6],
}

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
    let mut fields = [0; 6];
    let mut roots = [[0; 32]; 6];
    for (index, name) in BID_INDEX_FIELDS.into_iter().enumerate() {
        let (value, root) = read_u64_member(
            root,
            descriptor,
            &(("bid", bid_id), name),
            values,
            key_work,
            semantic_limit,
            record_limit,
            &mut *cancelled,
        )?;
        fields[index] = value;
        roots[index] = root;
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
    Ok(LoadedBidIndexNodeV1 {
        id: bid_id,
        node,
        roots,
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
    let old_values = loaded.node.fields();
    let new_values = next.fields();
    for (index, name) in BID_INDEX_FIELDS.into_iter().enumerate() {
        if old_values[index] == new_values[index] {
            continue;
        }
        let value_root = write_u64_member(
            root,
            descriptor,
            &(("bid", loaded.id), name),
            Some(loaded.roots[index]),
            Some(new_values[index]),
            values,
            write,
            cancelled,
            remaining,
        )?
        .ok_or(ValueTreeError::Invalid("marketplace bid index update root"))?;
        loaded.roots[index] = value_root;
    }
    loaded.node = next;
    Ok(loaded)
}

fn create_node<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    id: u64,
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
    let mut roots = [[0; 32]; 6];
    for (index, name) in BID_INDEX_FIELDS.into_iter().enumerate() {
        roots[index] = write_u64_member(
            root,
            descriptor,
            &(("bid", id), name),
            None,
            Some(node.fields()[index]),
            values,
            write,
            cancelled,
            remaining,
        )?
        .ok_or(ValueTreeError::Invalid("marketplace bid index create root"))?;
    }
    Ok(LoadedBidIndexNodeV1 { id, node, roots })
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
    for (index, name) in BID_INDEX_FIELDS.into_iter().enumerate() {
        write_u64_member(
            root,
            descriptor,
            &(("bid", node.id), name),
            Some(node.roots[index]),
            None,
            values,
            write,
            cancelled,
            remaining,
        )?;
    }
    Ok(())
}

fn rotate_left<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    mut x: LoadedBidIndexNodeV1,
    mut y: LoadedBidIndexNodeV1,
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
    if x.node.right_id != y.id {
        return Err(ValueTreeError::Invalid("marketplace bid AVL rotation link"));
    }
    x.node.right_id = y.node.left_id;
    x.node.right_size = y.node.left_size;
    x.node.right_height = y.node.left_height;
    let x_node = x.node;
    let x = store_node(
        root, descriptor, x, x_node, values, write, cancelled, remaining,
    )?;
    let x_summary = x.node.summary()?;
    y.node.left_id = x.id;
    y.node.left_size = x_summary.size;
    y.node.left_height = x_summary.height;
    let y_node = y.node;
    store_node(
        root, descriptor, y, y_node, values, write, cancelled, remaining,
    )
}

fn rotate_right<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    mut x: LoadedBidIndexNodeV1,
    mut y: LoadedBidIndexNodeV1,
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
    if x.node.left_id != y.id {
        return Err(ValueTreeError::Invalid("marketplace bid AVL rotation link"));
    }
    x.node.left_id = y.node.right_id;
    x.node.left_size = y.node.right_size;
    x.node.left_height = y.node.right_height;
    let x_node = x.node;
    let x = store_node(
        root, descriptor, x, x_node, values, write, cancelled, remaining,
    )?;
    let x_summary = x.node.summary()?;
    y.node.right_id = x.id;
    y.node.right_size = x_summary.size;
    y.node.right_height = x_summary.height;
    let y_node = y.node;
    store_node(
        root, descriptor, y, y_node, values, write, cancelled, remaining,
    )
}

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
    node.node.summary()?;
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
                root, descriptor, node, left, values, write, cancelled, remaining,
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
            root, descriptor, left, pivot, values, write, cancelled, remaining,
        )?;
        let left_summary = left.node.summary()?;
        node.node.left_id = left.id;
        node.node.left_size = left_summary.size;
        node.node.left_height = left_summary.height;
        rotate_right(
            root, descriptor, node, left, values, write, cancelled, remaining,
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
                root, descriptor, node, right, values, write, cancelled, remaining,
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
            root, descriptor, right, pivot, values, write, cancelled, remaining,
        )?;
        let right_summary = right.node.summary()?;
        node.node.right_id = right.id;
        node.node.right_size = right_summary.size;
        node.node.right_height = right_summary.height;
        rotate_left(
            root, descriptor, node, right, values, write, cancelled, remaining,
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

fn insert_at_inner<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
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
            BidIndexNodeV1::LEAF,
            values,
            write,
            cancelled,
            remaining,
        );
    }
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
    let size = current.node.summary()?.size;
    if rank > size {
        return Err(ValueTreeError::Invalid("marketplace bid insert rank"));
    }
    if rank <= current.node.left_size {
        let child = insert_at_inner(
            root,
            descriptor,
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
    insert_at_inner(
        root,
        descriptor,
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
        0,
    )
}

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

fn remove_min_inner<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    current_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
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
    if current.node.left_id == NO_BID_ID {
        let right = cached_child_summary(
            current.node.right_id,
            current.node.right_size,
            current.node.right_height,
        )?;
        return Ok((current.node.right_id, right, current));
    }
    let (left_id, left, removed) = remove_min_inner(
        root,
        descriptor,
        current.node.left_id,
        values,
        key_work,
        semantic_limit,
        record_limit,
        write,
        cancelled,
        remaining,
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

fn delete_at_inner<R, W, C, CW>(
    root: &mut StateRootV1,
    descriptor: crate::incremental_projection::FieldDescriptor,
    current_id: u64,
    rank: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    semantic_limit: u64,
    record_limit: u64,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancelled: &mut C,
    remaining: &mut u64,
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
    let current_summary = current.node.summary()?;
    if rank >= current_summary.size {
        return Err(ValueTreeError::Invalid("marketplace bid delete rank"));
    }
    if rank < current.node.left_size {
        let (left_id, left) = delete_at_inner(
            root,
            descriptor,
            current.node.left_id,
            rank,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
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
            current.node.right_id,
            rank - current.node.left_size - 1,
            values,
            key_work,
            semantic_limit,
            record_limit,
            write,
            cancelled,
            remaining,
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
            delete_node(
                root, descriptor, current, values, write, cancelled, remaining,
            )?;
            Ok((right_id, right))
        }
        (false, true) => {
            delete_node(
                root, descriptor, current, values, write, cancelled, remaining,
            )?;
            Ok((left_id, left))
        }
        (false, false) => {
            let (right_id, right, mut successor) = remove_min_inner(
                root,
                descriptor,
                current.node.right_id,
                values,
                key_work,
                semantic_limit,
                record_limit,
                write,
                cancelled,
                remaining,
                depth + 1,
            )?;
            successor.node.left_id = current.node.left_id;
            successor.node.left_size = left.size;
            successor.node.left_height = left.height;
            successor.node.right_id = right_id;
            successor.node.right_size = right.size;
            successor.node.right_height = right.height;
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
    delete_at_inner(
        root,
        descriptor,
        current_root_id,
        rank,
        values,
        key_work,
        semantic_limit,
        record_limit,
        write,
        cancelled,
        remaining,
        0,
    )
}
