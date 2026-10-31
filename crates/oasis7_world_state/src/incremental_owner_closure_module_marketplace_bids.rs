use super::*;

#[derive(Clone, Copy)]
struct BidFieldRow {
    bid_id: u64,
    bit: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BidIndexRow {
    bid_id: u64,
    node: crate::incremental_module_marketplace::bid_index::BidIndexNodeV1,
}

#[derive(Clone, Copy)]
struct BidIndexSummary {
    size: u64,
    height: u64,
}

fn validate_bid_index_tree(
    id: u64,
    parent_id: u64,
    depth: usize,
    nodes: &[BidIndexRow],
    marks: &mut [u8],
) -> Result<BidIndexSummary, WorldStateOwnerError> {
    use crate::incremental_module_marketplace::bid_index::NO_BID_ID;
    if id == NO_BID_ID {
        return Ok(BidIndexSummary { size: 0, height: 0 });
    }
    if depth >= crate::incremental_module_marketplace::bid_index::MAX_BID_INDEX_DEPTH {
        return Err(WorldStateOwnerError::ResourceLimited(
            "module bid AVL depth",
        ));
    }
    let index = nodes
        .binary_search_by_key(&id, |node| node.bid_id)
        .map_err(|_| WorldStateOwnerError::Invalid("module bid AVL dangling link"))?;
    if marks[index] != 0 {
        return Err(WorldStateOwnerError::Invalid(
            "module bid AVL cycle or shared child",
        ));
    }
    marks[index] = 1;
    let node = nodes[index].node;
    if node.parent_id != parent_id {
        return Err(WorldStateOwnerError::Invalid("module bid AVL parent link"));
    }
    let left = validate_bid_index_tree(node.left_id, id, depth + 1, nodes, marks)?;
    let right = validate_bid_index_tree(node.right_id, id, depth + 1, nodes, marks)?;
    if (node.left_id == NO_BID_ID) != (node.left_size == 0 && node.left_height == 0)
        || (node.right_id == NO_BID_ID) != (node.right_size == 0 && node.right_height == 0)
        || node.left_size != left.size
        || node.left_height != left.height
        || node.right_size != right.size
        || node.right_height != right.height
        || left.height.abs_diff(right.height) > 1
    {
        return Err(WorldStateOwnerError::Invalid(
            "module bid AVL cached summary or balance",
        ));
    }
    marks[index] = 2;
    Ok(BidIndexSummary {
        size: 1u64
            .checked_add(left.size)
            .and_then(|size| size.checked_add(right.size))
            .ok_or(WorldStateOwnerError::Invalid(
                "module bid AVL size overflow",
            ))?,
        height: 1u64.checked_add(left.height.max(right.height)).ok_or(
            WorldStateOwnerError::Invalid("module bid AVL height overflow"),
        )?,
    })
}

pub(super) fn validate_bids<M, V, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    values: &mut ValueReadContext<'_, V>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    use crate::incremental_module_marketplace::bid_index::NO_BID_ID;
    let length = read_bid_length(
        child,
        reader,
        field,
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    let next_bid_id = read_bid_scalar(
        child,
        reader,
        field,
        "next_bid_id",
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    let sequence_root_id = read_bid_scalar(
        child,
        reader,
        field,
        "sequence_root_id",
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    if next_bid_id < length {
        return Err(WorldStateOwnerError::Invalid("module bids next ID"));
    }
    let expected_leaves = length
        .checked_mul(u64::from(CHILD_BID_FIELDS) + 1)
        .and_then(|count| count.checked_add(3))
        .ok_or(WorldStateOwnerError::Invalid(
            "module bids leaf count overflow",
        ))?;
    if expected_leaves != child.leaf_count {
        return Err(WorldStateOwnerError::Invalid("module bids leaf count"));
    }
    let count = usize::try_from(length)
        .map_err(|_| WorldStateOwnerError::ResourceLimited("module bids count"))?;
    let field_count = count
        .checked_mul(usize::from(CHILD_BID_FIELDS))
        .ok_or(WorldStateOwnerError::ResourceLimited("module bids fields"))?;
    let field_bytes = field_count
        .checked_mul(std::mem::size_of::<BidFieldRow>())
        .ok_or(WorldStateOwnerError::ResourceLimited("module bids fields"))?;
    let node_bytes = count
        .checked_mul(std::mem::size_of::<BidIndexRow>())
        .ok_or(WorldStateOwnerError::ResourceLimited(
            "module bid AVL nodes",
        ))?;
    let marks_bytes = count.checked_mul(std::mem::size_of::<u8>()).ok_or(
        WorldStateOwnerError::ResourceLimited("module bid AVL marks"),
    )?;
    let reservation_bytes = field_bytes
        .checked_add(node_bytes)
        .and_then(|bytes| bytes.checked_add(marks_bytes))
        .ok_or(WorldStateOwnerError::ResourceLimited("module bids closure"))?
        .max(1);
    let _rows_charge = values
        .memory
        .reserve(reservation_bytes, MemoryClass::Ordinary)
        .map_err(WorldStateOwnerError::Memory)?;
    let mut fields = Vec::new();
    fields
        .try_reserve_exact(field_count)
        .map_err(|_| WorldStateOwnerError::ResourceLimited("module bids fields"))?;
    let mut nodes = Vec::new();
    nodes
        .try_reserve_exact(count)
        .map_err(|_| WorldStateOwnerError::ResourceLimited("module bid AVL nodes"))?;
    let mut marks = Vec::new();
    marks
        .try_reserve_exact(count)
        .map_err(|_| WorldStateOwnerError::ResourceLimited("module bid AVL marks"))?;
    if fields.capacity() * std::mem::size_of::<BidFieldRow>() > field_bytes
        || nodes.capacity() * std::mem::size_of::<BidIndexRow>() > node_bytes
        || marks.capacity() * std::mem::size_of::<u8>() > marks_bytes
    {
        return Err(WorldStateOwnerError::Invalid("module bids row admission"));
    }
    marks.resize(count, 0);
    validate_child_entries(
        child,
        reader,
        field,
        values,
        max_value_bytes,
        cancelled,
        |semantic, value| match semantic {
            ChildSemantic::Field("length") if value == ChildValue::U64(length) => Ok(()),
            ChildSemantic::Field("length") => Err(WorldStateOwnerError::Invalid(
                "module bids length changed during scan",
            )),
            ChildSemantic::Field("next_bid_id") if value == ChildValue::U64(next_bid_id) => Ok(()),
            ChildSemantic::Field("next_bid_id") => Err(WorldStateOwnerError::Invalid(
                "module bids next ID changed during scan",
            )),
            ChildSemantic::Field("sequence_root_id")
                if value == ChildValue::U64(sequence_root_id) =>
            {
                Ok(())
            }
            ChildSemantic::Field("sequence_root_id") => Err(WorldStateOwnerError::Invalid(
                "module bids sequence root changed during scan",
            )),
            ChildSemantic::Field(_) => Err(WorldStateOwnerError::Invalid(
                "module bids unexpected scalar child",
            )),
            ChildSemantic::Bid {
                bid_id,
                field: "index_node",
            } => {
                if bid_id >= next_bid_id {
                    return Err(WorldStateOwnerError::Invalid("module bid ID range"));
                }
                let ChildValue::BidIndexNode(node) = value else {
                    return Err(WorldStateOwnerError::Invalid("module bid AVL field type"));
                };
                if nodes.len() == count {
                    return Err(WorldStateOwnerError::Invalid("module bid AVL node count"));
                }
                nodes.push(BidIndexRow { bid_id, node });
                Ok(())
            }
            ChildSemantic::Bid {
                bid_id,
                field: name,
            } => {
                if bid_id >= next_bid_id {
                    return Err(WorldStateOwnerError::Invalid("module bid ID range"));
                }
                let bit = match (name, value) {
                    ("present", ChildValue::Unit) => 1 << 0,
                    ("order_id" | "bid_at", ChildValue::U64(_)) => {
                        if name == "order_id" {
                            1 << 1
                        } else {
                            1 << 5
                        }
                    }
                    ("bidder_agent_id", ChildValue::Text) => 1 << 2,
                    ("price_kind", ChildValue::U64(kind)) if kind <= 1 => 1 << 3,
                    ("price_amount", ChildValue::I64(_)) => 1 << 4,
                    _ => return Err(WorldStateOwnerError::Invalid("module bid field profile")),
                };
                fields.push(BidFieldRow { bid_id, bit });
                Ok(())
            }
        },
    )?;
    if fields.len() != field_count || nodes.len() != count {
        return Err(WorldStateOwnerError::Invalid(
            "module bid membership counts",
        ));
    }
    fields.sort_unstable_by_key(|field| (field.bid_id, field.bit));
    let mut index = 0usize;
    while index < fields.len() {
        let bid_id = fields[index].bid_id;
        let mut mask = 0u8;
        while index < fields.len() && fields[index].bid_id == bid_id {
            let field = fields[index];
            if mask & field.bit != 0 {
                return Err(WorldStateOwnerError::Invalid("module bid duplicate field"));
            }
            mask |= field.bit;
            index += 1;
        }
        if mask != CHILD_BID_MASK {
            return Err(WorldStateOwnerError::Invalid("module bid record closure"));
        }
    }
    nodes.sort_unstable_by_key(|node| node.bid_id);
    fields.sort_unstable_by_key(|field| field.bid_id);
    if nodes
        .windows(2)
        .any(|pair| pair[0].bid_id == pair[1].bid_id)
        || fields
            .chunks_exact(usize::from(CHILD_BID_FIELDS))
            .zip(nodes.iter())
            .any(|(fields, node)| {
                fields
                    .first()
                    .is_none_or(|field| field.bid_id != node.bid_id)
            })
    {
        return Err(WorldStateOwnerError::Invalid(
            "module bid payload/index identity closure",
        ));
    }
    if length == 0 {
        if sequence_root_id != NO_BID_ID || !nodes.is_empty() {
            return Err(WorldStateOwnerError::Invalid("module bid empty AVL root"));
        }
    } else {
        if sequence_root_id == NO_BID_ID || sequence_root_id >= next_bid_id {
            return Err(WorldStateOwnerError::Invalid("module bid AVL root ID"));
        }
        let summary = validate_bid_index_tree(sequence_root_id, NO_BID_ID, 0, &nodes, &mut marks)?;
        if summary.size != length || marks.iter().any(|mark| *mark != 2) {
            return Err(WorldStateOwnerError::Invalid(
                "module bid AVL reachability/count",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental_module_marketplace::bid_index::{BidIndexNodeV1, NO_BID_ID};

    #[test]
    fn bid_index_closure_checks_parent_links() {
        let root = BidIndexNodeV1 {
            left_id: 0,
            right_id: NO_BID_ID,
            left_size: 1,
            left_height: 1,
            right_size: 0,
            right_height: 0,
            parent_id: NO_BID_ID,
        };
        let leaf = BidIndexNodeV1 {
            parent_id: 1,
            ..BidIndexNodeV1::LEAF
        };
        let valid = [
            BidIndexRow {
                bid_id: 0,
                node: leaf,
            },
            BidIndexRow {
                bid_id: 1,
                node: root,
            },
        ];
        let mut marks = [0; 2];
        let summary = validate_bid_index_tree(1, NO_BID_ID, 0, &valid, &mut marks).unwrap();
        assert_eq!(summary.size, 2);
        assert_eq!(summary.height, 2);
        assert_eq!(marks, [2, 2]);

        let invalid = [
            BidIndexRow {
                bid_id: 0,
                node: BidIndexNodeV1::LEAF,
            },
            BidIndexRow {
                bid_id: 1,
                node: root,
            },
        ];
        let mut marks = [0; 2];
        assert!(matches!(
            validate_bid_index_tree(1, NO_BID_ID, 0, &invalid, &mut marks),
            Err(WorldStateOwnerError::Invalid("module bid AVL parent link"))
        ));
    }
}
