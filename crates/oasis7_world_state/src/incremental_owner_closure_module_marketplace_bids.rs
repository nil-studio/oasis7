use super::*;

use crate::incremental_module_marketplace::bid_index::NO_BID_ID;
use crate::incremental_module_marketplace::bid_membership::{group_schema_v1, load_group};

#[derive(Clone, Copy)]
pub(super) struct BidIndexSummary {
    size: u64,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Reads one typed AVL node using the caller's shared scan, value and cancellation budgets."
)]
fn read_index_node<M, R, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    bid_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<crate::incremental_module_marketplace::bid_index::BidIndexNodeV1, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    match super::read_bid_member(
        child,
        reader,
        field,
        bid_id,
        "index_node",
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )? {
        ChildValue::BidIndexNode(node) => Ok(node),
        _ => Err(WorldStateOwnerError::Invalid("module bid AVL field type")),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Checks linked AVL nodes with the same scan, value, key and cancellation budgets."
)]
pub(super) fn validate_index_node_links<M, R, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    bid_id: u64,
    node: crate::incremental_module_marketplace::bid_index::BidIndexNodeV1,
    root_id: u64,
    length: u64,
    next_bid_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<BidIndexSummary, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    use crate::incremental_module_marketplace::bid_index::{MAX_BID_INDEX_DEPTH, NO_BID_ID};

    if bid_id >= next_bid_id
        || node.left_id == bid_id
        || node.right_id == bid_id
        || (node.left_id != NO_BID_ID && node.left_id == node.right_id)
    {
        return Err(WorldStateOwnerError::Invalid("module bid AVL node IDs"));
    }
    let summary = node
        .summary()
        .map_err(|_| WorldStateOwnerError::Invalid("module bid AVL node summary"))?;
    if summary.height > MAX_BID_INDEX_DEPTH as u64 {
        return Err(WorldStateOwnerError::ResourceLimited(
            "module bid AVL depth",
        ));
    }

    if bid_id == root_id {
        if node.parent_id != NO_BID_ID || summary.size != length {
            return Err(WorldStateOwnerError::Invalid("module bid AVL root summary"));
        }
    } else {
        if node.parent_id == NO_BID_ID || node.parent_id >= next_bid_id {
            return Err(WorldStateOwnerError::Invalid("module bid AVL parent link"));
        }
        let parent = read_index_node(
            child,
            reader,
            field,
            node.parent_id,
            values,
            key_work,
            max_value_bytes,
            cancelled,
        )?;
        if parent.left_id == parent.right_id && parent.left_id != NO_BID_ID {
            return Err(WorldStateOwnerError::Invalid("module bid AVL shared child"));
        }
        if parent.left_id != bid_id && parent.right_id != bid_id {
            return Err(WorldStateOwnerError::Invalid("module bid AVL parent link"));
        }
    }

    for (child_id, child_size, child_height) in [
        (node.left_id, node.left_size, node.left_height),
        (node.right_id, node.right_size, node.right_height),
    ] {
        if child_id == NO_BID_ID {
            if child_size != 0 || child_height != 0 {
                return Err(WorldStateOwnerError::Invalid(
                    "module bid AVL empty child summary",
                ));
            }
            continue;
        }
        if child_id >= next_bid_id || child_height >= summary.height {
            return Err(WorldStateOwnerError::Invalid("module bid AVL child link"));
        }
        let child_node = read_index_node(
            child,
            reader,
            field,
            child_id,
            values,
            key_work,
            max_value_bytes,
            cancelled,
        )?;
        if child_node.parent_id != bid_id {
            return Err(WorldStateOwnerError::Invalid("module bid AVL parent link"));
        }
        let child_summary = child_node
            .summary()
            .map_err(|_| WorldStateOwnerError::Invalid("module bid AVL child summary"))?;
        if child_summary.size != child_size || child_summary.height != child_height {
            return Err(WorldStateOwnerError::Invalid(
                "module bid AVL cached summary",
            ));
        }
    }

    Ok(BidIndexSummary { size: summary.size })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Validates a primary bid node and its business payload under shared budgets."
)]
fn validate_bid_node_links<M, R, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    bid_id: u64,
    node: crate::incremental_module_marketplace::bid_index::BidIndexNodeV1,
    root_id: u64,
    length: u64,
    next_bid_id: u64,
    values: &mut ValueReadContext<'_, R>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<BidIndexSummary, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
{
    let summary = validate_index_node_links(
        child,
        reader,
        field,
        bid_id,
        node,
        root_id,
        length,
        next_bid_id,
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    for (name, expected) in [
        ("present", 0),
        ("order_id", 1),
        ("bidder_agent_id", 2),
        ("price_kind", 3),
        ("price_amount", 4),
        ("bid_at", 5),
    ] {
        let value = super::read_bid_member(
            child,
            reader,
            field,
            bid_id,
            name,
            values,
            key_work,
            max_value_bytes,
            cancelled,
        )?;
        let valid = match (expected, value) {
            (0, ChildValue::Unit)
            | (1, ChildValue::U64(_))
            | (2, ChildValue::Text)
            | (4, ChildValue::I64(_))
            | (5, ChildValue::U64(_)) => true,
            (3, ChildValue::U64(kind)) => kind <= 1,
            _ => false,
        };
        if !valid {
            return Err(WorldStateOwnerError::Invalid("module bid field profile"));
        }
    }
    Ok(summary)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Validates one pair membership child against the same parent scan and shared budgets."
)]
fn validate_bid_group<M, V, C>(
    primary: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    digest: [u8; 32],
    reference_bytes: [u8; RECORD_ROOT_BYTES],
    next_bid_id: u64,
    values: &mut ValueReadContext<'_, V>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<u64, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    let reference = RecordRootRefV1::from_canonical_bytes(&reference_bytes)
        .map_err(|_| WorldStateOwnerError::Invalid("module bid group reference framing"))?;
    let child = reference
        .validate_binding(&group_schema_v1(), &digest, StateNamespaceV1::Business)
        .map_err(|_| WorldStateOwnerError::Invalid("module bid group reference binding"))?
        .clone();
    let order_id = read_bid_scalar(
        &child,
        reader,
        field,
        "order_id",
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    let bidder_digest = super::read_bid_text_digest(
        &child,
        reader,
        field,
        &"bidder_agent_id",
        ChildSemantic::Field("bidder_agent_id"),
        order_id,
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    if bidder_digest != digest {
        return Err(WorldStateOwnerError::Invalid("module bid group identity"));
    }
    let length = read_bid_scalar(
        &child,
        reader,
        field,
        "length",
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    let sequence_root_id = read_bid_scalar(
        &child,
        reader,
        field,
        "sequence_root_id",
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    if length == 0
        || sequence_root_id == NO_BID_ID
        || sequence_root_id >= next_bid_id
        || length
            .checked_add(4)
            .is_none_or(|expected| expected != child.leaf_count)
    {
        return Err(WorldStateOwnerError::Invalid(
            "module bid group root profile",
        ));
    }
    let root_node = read_index_node(
        &child,
        reader,
        field,
        sequence_root_id,
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    if root_node.parent_id != NO_BID_ID
        || root_node
            .summary()
            .map_err(|_| WorldStateOwnerError::Invalid("module bid group root summary"))?
            .size
            != length
    {
        return Err(WorldStateOwnerError::Invalid(
            "module bid group root summary",
        ));
    }

    let mut scalar_mask = 0u8;
    let mut index_nodes = 0u64;
    let mut roots = 0u64;
    let scanned =
        reader
            .visit_entries(&child, |entry| {
                let (semantic, value) = super::read_child_member_with_pair(
                    entry,
                    field,
                    order_id,
                    key_work,
                    values,
                    max_value_bytes,
                    cancelled,
                )?;
                match semantic {
                    ChildSemantic::Field("order_id") if value == ChildValue::U64(order_id) => {
                        scalar_mask |= 1;
                        Ok(())
                    }
                    ChildSemantic::Field("bidder_agent_id")
                        if value == ChildValue::TextPairDigest(digest) =>
                    {
                        scalar_mask |= 2;
                        Ok(())
                    }
                    ChildSemantic::Field("length") if value == ChildValue::U64(length) => {
                        scalar_mask |= 4;
                        Ok(())
                    }
                    ChildSemantic::Field("sequence_root_id")
                        if value == ChildValue::U64(sequence_root_id) =>
                    {
                        scalar_mask |= 8;
                        Ok(())
                    }
                    ChildSemantic::Field(_) => Err(WorldStateOwnerError::Invalid(
                        "module bid group unexpected scalar child",
                    )),
                    ChildSemantic::Bid {
                        bid_id,
                        field: "index_node",
                    } => {
                        let ChildValue::BidIndexNode(node) = value else {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid group index type",
                            ));
                        };
                        if bid_id >= next_bid_id {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid group member ID range",
                            ));
                        }
                        let summary = validate_index_node_links(
                            &child,
                            reader,
                            field,
                            bid_id,
                            node,
                            sequence_root_id,
                            length,
                            next_bid_id,
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )?;
                        index_nodes = index_nodes.checked_add(1).ok_or(
                            WorldStateOwnerError::ResourceLimited("module bid group index count"),
                        )?;
                        if bid_id == sequence_root_id {
                            roots = roots.checked_add(1).ok_or(
                                WorldStateOwnerError::ResourceLimited("module bid group roots"),
                            )?;
                            if summary.size != length {
                                return Err(WorldStateOwnerError::Invalid(
                                    "module bid group root size",
                                ));
                            }
                        }
                        let (rank, found) =
                            crate::incremental_module_marketplace::bid_index::rank_of_id(
                                &child,
                                field,
                                sequence_root_id,
                                length,
                                bid_id,
                                values,
                                key_work,
                                max_value_bytes,
                                max_value_bytes,
                                cancelled,
                            )
                            .map_err(WorldStateOwnerError::Value)?;
                        if !found
                            || crate::incremental_module_marketplace::bid_index::select_ordinal(
                                &child,
                                field,
                                rank,
                                values,
                                key_work,
                                max_value_bytes,
                                max_value_bytes,
                                cancelled,
                            )
                            .map_err(WorldStateOwnerError::Value)?
                                != bid_id
                        {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid group order index",
                            ));
                        }
                        let primary_order = match super::read_bid_member(
                            primary,
                            reader,
                            field,
                            bid_id,
                            "order_id",
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )? {
                            ChildValue::U64(order_id) => order_id,
                            _ => {
                                return Err(WorldStateOwnerError::Invalid(
                                    "module bid group primary order type",
                                ));
                            }
                        };
                        if primary_order != order_id {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid group member order",
                            ));
                        }
                        let primary_digest = super::read_bid_text_digest(
                            primary,
                            reader,
                            field,
                            &(("bid", bid_id), "bidder_agent_id"),
                            ChildSemantic::Bid {
                                bid_id,
                                field: "bidder_agent_id",
                            },
                            order_id,
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )?;
                        if primary_digest != digest {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid group member pair",
                            ));
                        }
                        Ok(())
                    }
                    ChildSemantic::Bid { .. } => Err(WorldStateOwnerError::Invalid(
                        "module bid group unexpected member",
                    )),
                    ChildSemantic::BidGroup(_) => Err(WorldStateOwnerError::Invalid(
                        "module bid group nested reference",
                    )),
                }
            })
            .map_err(super::map_scan_error)?;
    if scanned != length + 4 || scalar_mask != 0b1111 || index_nodes != length || roots != 1 {
        return Err(WorldStateOwnerError::Invalid("module bid group closure"));
    }
    Ok(length)
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
    let base_expected_leaves = length
        .checked_mul(7)
        .and_then(|count| count.checked_add(3))
        .ok_or(WorldStateOwnerError::Invalid(
            "module bids leaf count overflow",
        ))?;
    if (length == 0) != (sequence_root_id == NO_BID_ID)
        || (sequence_root_id != NO_BID_ID && sequence_root_id >= next_bid_id)
    {
        return Err(WorldStateOwnerError::Invalid("module bid AVL root ID"));
    }

    let mut scalar_mask = 0u8;
    let mut index_nodes = 0u64;
    let mut bid_fields = 0u64;
    let mut primary_membership_checks = 0u64;
    let mut group_count = 0u64;
    let mut group_memberships = 0u64;
    let mut roots = 0u64;
    let mut root_summary = None;
    let scanned =
        reader
            .visit_entries(child, |entry| {
                let (semantic, value) =
                    super::read_child_member(entry, field, values, max_value_bytes, cancelled)?;
                match semantic {
                    ChildSemantic::Field("length") if value == ChildValue::U64(length) => {
                        scalar_mask |= 1;
                        Ok(())
                    }
                    ChildSemantic::Field("next_bid_id")
                        if value == ChildValue::U64(next_bid_id) =>
                    {
                        scalar_mask |= 2;
                        Ok(())
                    }
                    ChildSemantic::Field("sequence_root_id")
                        if value == ChildValue::U64(sequence_root_id) =>
                    {
                        scalar_mask |= 4;
                        Ok(())
                    }
                    ChildSemantic::BidGroup(digest) => {
                        let ChildValue::BidGroupRoot(reference_bytes) = value else {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid group reference type",
                            ));
                        };
                        let group_length = validate_bid_group(
                            child,
                            reader,
                            field,
                            digest,
                            reference_bytes,
                            next_bid_id,
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )?;
                        group_count = group_count.checked_add(1).ok_or(
                            WorldStateOwnerError::ResourceLimited("module bid group count"),
                        )?;
                        group_memberships = group_memberships.checked_add(group_length).ok_or(
                            WorldStateOwnerError::ResourceLimited(
                                "module bid group membership count",
                            ),
                        )?;
                        Ok(())
                    }
                    ChildSemantic::Field(_) => Err(WorldStateOwnerError::Invalid(
                        "module bids unexpected scalar child",
                    )),
                    ChildSemantic::Bid {
                        bid_id,
                        field: "index_node",
                    } => {
                        let ChildValue::BidIndexNode(node) = value else {
                            return Err(WorldStateOwnerError::Invalid("module bid AVL field type"));
                        };
                        index_nodes = index_nodes.checked_add(1).ok_or(
                            WorldStateOwnerError::ResourceLimited("module bid AVL node count"),
                        )?;
                        let summary = validate_bid_node_links(
                            child,
                            reader,
                            field,
                            bid_id,
                            node,
                            sequence_root_id,
                            length,
                            next_bid_id,
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )?;
                        if bid_id == sequence_root_id {
                            roots = roots.checked_add(1).ok_or(
                                WorldStateOwnerError::ResourceLimited("module bid AVL root count"),
                            )?;
                            root_summary = Some(summary);
                        } else if node.parent_id == NO_BID_ID {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid AVL unexpected root",
                            ));
                        }
                        Ok(())
                    }
                    ChildSemantic::Bid {
                        bid_id,
                        field: "present",
                    } => {
                        if bid_id >= next_bid_id || value != ChildValue::Unit {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid membership source marker",
                            ));
                        }
                        let order_id = match super::read_bid_member(
                            child,
                            reader,
                            field,
                            bid_id,
                            "order_id",
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )? {
                            ChildValue::U64(order_id) => order_id,
                            _ => {
                                return Err(WorldStateOwnerError::Invalid(
                                    "module bid membership order type",
                                ));
                            }
                        };
                        let digest = super::read_bid_text_digest(
                            child,
                            reader,
                            field,
                            &(("bid", bid_id), "bidder_agent_id"),
                            ChildSemantic::Bid {
                                bid_id,
                                field: "bidder_agent_id",
                            },
                            order_id,
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )?;
                        let group = load_group(
                            child,
                            field,
                            &digest,
                            values,
                            key_work,
                            max_value_bytes,
                            max_value_bytes,
                            cancelled,
                        )
                        .map_err(WorldStateOwnerError::Value)?
                        .ok_or(WorldStateOwnerError::Invalid(
                            "module bid group for primary member missing",
                        ))?;
                        let group_length = read_bid_scalar(
                            &group.child,
                            reader,
                            field,
                            "length",
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )?;
                        let group_root_id = read_bid_scalar(
                            &group.child,
                            reader,
                            field,
                            "sequence_root_id",
                            values,
                            key_work,
                            max_value_bytes,
                            cancelled,
                        )?;
                        let (rank, found) =
                            crate::incremental_module_marketplace::bid_index::rank_of_id(
                                &group.child,
                                field,
                                group_root_id,
                                group_length,
                                bid_id,
                                values,
                                key_work,
                                max_value_bytes,
                                max_value_bytes,
                                cancelled,
                            )
                            .map_err(WorldStateOwnerError::Value)?;
                        if !found
                            || crate::incremental_module_marketplace::bid_index::select_ordinal(
                                &group.child,
                                field,
                                rank,
                                values,
                                key_work,
                                max_value_bytes,
                                max_value_bytes,
                                cancelled,
                            )
                            .map_err(WorldStateOwnerError::Value)?
                                != bid_id
                        {
                            return Err(WorldStateOwnerError::Invalid(
                                "module bid missing from membership index",
                            ));
                        }
                        primary_membership_checks = primary_membership_checks
                            .checked_add(1)
                            .ok_or(WorldStateOwnerError::ResourceLimited(
                                "module bid membership checks",
                            ))?;
                        bid_fields = bid_fields.checked_add(1).ok_or(
                            WorldStateOwnerError::ResourceLimited("module bid field count"),
                        )?;
                        Ok(())
                    }
                    ChildSemantic::Bid {
                        bid_id,
                        field: name,
                    } => {
                        if bid_id >= next_bid_id {
                            return Err(WorldStateOwnerError::Invalid("module bid ID range"));
                        }
                        let valid = match (name, value) {
                            ("present", ChildValue::Unit)
                            | ("order_id" | "bid_at", ChildValue::U64(_))
                            | ("bidder_agent_id", ChildValue::Text)
                            | ("price_amount", ChildValue::I64(_)) => true,
                            ("price_kind", ChildValue::U64(kind)) => kind <= 1,
                            _ => false,
                        };
                        if !valid {
                            return Err(WorldStateOwnerError::Invalid("module bid field profile"));
                        }
                        bid_fields = bid_fields.checked_add(1).ok_or(
                            WorldStateOwnerError::ResourceLimited("module bid field count"),
                        )?;
                        Ok(())
                    }
                }
            })
            .map_err(super::map_scan_error)?;

    let expected_bid_fields =
        length
            .checked_mul(6)
            .ok_or(WorldStateOwnerError::ResourceLimited(
                "module bid field count",
            ))?;
    let expected_leaves =
        base_expected_leaves
            .checked_add(group_count)
            .ok_or(WorldStateOwnerError::Invalid(
                "module bids leaf count overflow",
            ))?;
    if scanned != expected_leaves
        || scalar_mask != 0b111
        || index_nodes != length
        || bid_fields != expected_bid_fields
        || primary_membership_checks != length
        || group_memberships != length
    {
        return Err(WorldStateOwnerError::Invalid("module bids record closure"));
    }
    if length == 0 {
        if roots != 0 || root_summary.is_some() {
            return Err(WorldStateOwnerError::Invalid("module bid empty AVL root"));
        }
    } else if roots != 1 || root_summary.is_none_or(|summary| summary.size != length) {
        return Err(WorldStateOwnerError::Invalid(
            "module bid AVL reachability/count",
        ));
    }
    Ok(())
}

fn read_bid_length<M, V, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    values: &mut ValueReadContext<'_, V>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<u64, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    let value = super::read_bid_field(
        child,
        reader,
        field,
        "length",
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )?;
    match value {
        ChildValue::U64(length) => Ok(length),
        _ => Err(WorldStateOwnerError::Invalid("module bids length value")),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Reads a bid scalar with the active scan, value, semantic-key and cancellation budgets."
)]
fn read_bid_scalar<M, V, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    name: &'static str,
    values: &mut ValueReadContext<'_, V>,
    key_work: &mut SemanticKeyBudget,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<u64, WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    match super::read_bid_field(
        child,
        reader,
        field,
        name,
        values,
        key_work,
        max_value_bytes,
        cancelled,
    )? {
        ChildValue::U64(value) => Ok(value),
        _ => Err(WorldStateOwnerError::Invalid("module bids scalar value")),
    }
}
