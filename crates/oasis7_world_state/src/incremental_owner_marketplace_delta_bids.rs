//! Parent-qualified primary/secondary bid graphs advance through actual Δ only.
use super::*;
use crate::incremental_module_marketplace::{bid_index as avl, bid_membership};
#[derive(Clone, Copy)]
struct Meta {
    len: u64,
    root: u64,
    next: u64,
}
struct Read<'a, 'r, 'v, 'g, M, R, G, C> {
    guard: &'a StateScanReader<'g, M, G>,
    records: &'a mut InventoryValueReader<'r, 'v, R, C>,
    field: FieldDescriptor,
    max: u64,
}
impl<M: MerkleNodeReader, R: ValueNodeReader, G: FnMut() -> bool, C: FnMut() -> bool>
    Read<'_, '_, '_, '_, M, R, G, C>
{
    fn scalar(
        &mut self,
        child: &StateRootV1,
        name: &'static str,
    ) -> Result<u64, WorldStateOwnerError> {
        let key = make_text_key(
            self.field,
            name,
            self.records.values,
            self.records.key_work,
            self.max,
            self.records.cancelled,
        )?;
        match read_value_at_key(
            child,
            self.guard,
            self.field,
            &key,
            ChildSemantic::Field(name),
            None,
            self.records.key_work,
            self.records.values,
            self.max,
            self.records.cancelled,
        )? {
            ChildValue::U64(v) => Ok(v),
            _ => Err(bad()),
        }
    }
    fn bid(
        &mut self,
        child: &StateRootV1,
        id: u64,
        name: &'static str,
    ) -> Result<ChildValue, WorldStateOwnerError> {
        read_bid_member(
            child,
            self.guard,
            self.field,
            id,
            name,
            self.records.values,
            self.records.key_work,
            self.max,
            self.records.cancelled,
        )
    }
    fn point<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        child: &StateRootV1,
        key: &K,
    ) -> Result<Option<[u8; 32]>, WorldStateOwnerError> {
        let key = make_semantic_key(
            self.field,
            key,
            self.records.values,
            self.records.key_work,
            self.max,
            self.records.cancelled,
        )?;
        self.guard
            .get(child, key.as_bytes())
            .map_err(map_scan_lookup_error)
    }
    fn index(
        &mut self,
        child: &StateRootV1,
        id: u64,
    ) -> Result<avl::BidIndexNodeV1, WorldStateOwnerError> {
        match self.bid(child, id, "index_node")? {
            ChildValue::BidIndexNode(v) => Ok(v),
            _ => Err(bad()),
        }
    }
    fn meta(
        &mut self,
        child: &StateRootV1,
        next: Option<u64>,
    ) -> Result<Meta, WorldStateOwnerError> {
        let len = self.scalar(child, "length")?;
        let root = self.scalar(child, "sequence_root_id")?;
        let next = match next {
            Some(n) => n,
            None => self.scalar(child, "next_bid_id")?,
        };
        if next < len
            || (len == 0) != (root == avl::NO_BID_ID)
            || (root != avl::NO_BID_ID && root >= next)
        {
            return Err(bad());
        }
        Ok(Meta { len, root, next })
    }
    fn roundtrip(
        &mut self,
        child: &StateRootV1,
        id: u64,
        m: Meta,
        group: bool,
    ) -> Result<(), WorldStateOwnerError> {
        let rank = if group {
            let (rank, found) = avl::rank_of_id(
                child,
                self.field,
                m.root,
                m.len,
                id,
                self.records.values,
                self.records.key_work,
                self.max,
                self.max,
                self.records.cancelled,
            )
            .map_err(WorldStateOwnerError::Value)?;
            if !found {
                return Err(bad());
            }
            rank
        } else {
            avl::ordinal_of_id(
                child,
                self.field,
                m.root,
                id,
                self.records.values,
                self.records.key_work,
                self.max,
                self.max,
                self.records.cancelled,
            )
            .map_err(WorldStateOwnerError::Value)?
        };
        if avl::select_ordinal(
            child,
            self.field,
            rank,
            self.records.values,
            self.records.key_work,
            self.max,
            self.max,
            self.records.cancelled,
        )
        .map_err(WorldStateOwnerError::Value)?
            != id
        {
            return Err(bad());
        }
        Ok(())
    }
    fn edge(
        &mut self,
        child: &StateRootV1,
        mut id: u64,
        last: bool,
    ) -> Result<u64, WorldStateOwnerError> {
        for _ in 0..avl::MAX_BID_INDEX_DEPTH {
            let node = self.index(child, id)?;
            let next = if last { node.right_id } else { node.left_id };
            if next == avl::NO_BID_ID {
                return Ok(id);
            }
            id = next;
        }
        Err(bad())
    }
    fn links(
        &mut self,
        child: &StateRootV1,
        id: u64,
        node: avl::BidIndexNodeV1,
        m: Meta,
        group: bool,
    ) -> Result<(), WorldStateOwnerError> {
        bids::validate_index_node_links(
            child,
            self.guard,
            self.field,
            id,
            node,
            m.root,
            m.len,
            m.next,
            self.records.values,
            self.records.key_work,
            self.max,
            self.records.cancelled,
        )?;
        self.roundtrip(child, id, m, group)?;
        if group {
            if node.left_id != avl::NO_BID_ID && self.edge(child, node.left_id, true)? >= id {
                return Err(bad());
            }
            if node.right_id != avl::NO_BID_ID && self.edge(child, node.right_id, false)? <= id {
                return Err(bad());
            }
        }
        Ok(())
    }
    fn shape(&mut self, child: &StateRootV1, id: u64) -> Result<bool, WorldStateOwnerError> {
        let mut n = 0;
        for field in [
            "present",
            "order_id",
            "bidder_agent_id",
            "price_kind",
            "price_amount",
            "bid_at",
            "index_node",
        ] {
            if self.point(child, &(("bid", id), field))?.is_some() {
                n += 1;
            }
        }
        if n != 0 && n != 7 {
            return Err(bad());
        }
        Ok(n == 7)
    }
    fn identity_same(
        &mut self,
        before: &StateRootV1,
        after: &StateRootV1,
        id: u64,
    ) -> Result<bool, WorldStateOwnerError> {
        for field in ["order_id", "bidder_agent_id"] {
            let key = (("bid", id), field);
            if self.point(before, &key)? != self.point(after, &key)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn pair(&mut self, child: &StateRootV1, id: u64) -> Result<[u8; 32], WorldStateOwnerError> {
        let ChildValue::U64(order) = self.bid(child, id, "order_id")? else {
            return Err(bad());
        };
        read_bid_text_digest(
            child,
            self.guard,
            self.field,
            &(("bid", id), "bidder_agent_id"),
            ChildSemantic::Bid {
                bid_id: id,
                field: "bidder_agent_id",
            },
            order,
            self.records.values,
            self.records.key_work,
            self.max,
            self.records.cancelled,
        )
    }
    fn membership(
        &mut self,
        primary: &StateRootV1,
        digest: [u8; 32],
        id: u64,
        expected: bool,
        next: u64,
    ) -> Result<(), WorldStateOwnerError> {
        let loaded = bid_membership::load_group(
            primary,
            self.field,
            &digest,
            self.records.values,
            self.records.key_work,
            self.max,
            self.max,
            self.records.cancelled,
        )
        .map_err(WorldStateOwnerError::Value)?;
        let Some(group) = loaded else {
            return if expected { Err(bad()) } else { Ok(()) };
        };
        let present = self
            .point(&group.child, &(("bid", id), "index_node"))?
            .is_some();
        if present != expected {
            return Err(bad());
        }
        if present {
            let meta = self.meta(&group.child, Some(next))?;
            self.roundtrip(&group.child, id, meta, true)?;
        }
        Ok(())
    }
    fn primary_pair(
        &mut self,
        before: &StateRootV1,
        after: &StateRootV1,
        id: u64,
        old_exists: bool,
        new_exists: bool,
        next: u64,
    ) -> Result<(), WorldStateOwnerError> {
        let old = if old_exists {
            Some(self.pair(before, id)?)
        } else {
            None
        };
        let new = if new_exists {
            Some(self.pair(after, id)?)
        } else {
            None
        };
        if let Some(old) = old
            && Some(old) != new
        {
            self.membership(after, old, id, false, next)?;
        }
        if let Some(new) = new {
            self.membership(after, new, id, true, next)?;
        }
        Ok(())
    }
}
fn semantic<R: ValueNodeReader, C: FnMut() -> bool>(
    entry: &StateEntryV1,
    field: FieldDescriptor,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
) -> Result<ChildSemantic, WorldStateOwnerError> {
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(WorldStateOwnerError::Invalid)?;
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if key.as_bytes() != entry.key {
        return Err(bad());
    }
    let mut bytes = [0; CHILD_SEMANTIC_CAPACITY];
    let mut out = &mut bytes[..];
    let info = crate::incremental_field_member::read_member_record_key_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: CHILD_SEMANTIC_CAPACITY as u64,
            payload_bytes: max,
            record_bytes: max,
        },
        &mut out,
        &mut *records.cancelled,
    )
    .map_err(WorldStateOwnerError::Value)?;
    classify_semantic(&bytes[..info.semantic_bytes as usize]).ok_or_else(bad)
}
#[derive(Clone, Copy)]
struct Totals {
    nodes: i128,
    fields: i128,
    groups: i128,
    members: i128,
}
fn valid_business(name: &str, value: ChildValue) -> bool {
    matches!(
        (name, value),
        ("present", ChildValue::Unit)
            | ("order_id" | "bid_at", ChildValue::U64(_))
            | ("bidder_agent_id", ChildValue::Text)
            | ("price_amount", ChildValue::I64(_))
    ) || matches!((name, value), ("price_kind", ChildValue::U64(0 | 1)))
}
#[allow(
    clippy::too_many_arguments,
    reason = "Before/after roots, group identity, structural/value readers, descriptor, member-cell budget, and sequence bounds are distinct delta-proof inputs."
)]
fn group_delta<M: MerkleNodeReader, R: ValueNodeReader, G: FnMut() -> bool, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    digest: [u8; 32],
    guard: &StateScanReader<'_, M, G>,
    field: FieldDescriptor,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
    old_next: u64,
    next: u64,
) -> Result<(u64, u64), WorldStateOwnerError> {
    let old = bid_membership::load_group(
        before,
        field,
        &digest,
        records.values,
        records.key_work,
        max,
        max,
        records.cancelled,
    )
    .map_err(WorldStateOwnerError::Value)?;
    let new = bid_membership::load_group(
        after,
        field,
        &digest,
        records.values,
        records.key_work,
        max,
        max,
        records.cancelled,
    )
    .map_err(WorldStateOwnerError::Value)?;
    let mut r = Read {
        guard,
        records,
        field,
        max,
    };
    match (old, new) {
        (None, None) => Err(bad()),
        (Some(old), None) => {
            let m = r.meta(&old.child, Some(old_next))?;
            Ok((m.len, 0))
        }
        (None, Some(new)) => {
            let mut bytes = [0; RECORD_ROOT_BYTES];
            let reference =
                RecordRootRefV1::new(bid_membership::group_schema_v1(), digest, new.child)
                    .map_err(|_| bad())?;
            bytes.copy_from_slice(&reference.canonical_bytes().map_err(|_| bad())?);
            let count = bids::validate_bid_group(
                after,
                guard,
                field,
                digest,
                bytes,
                next,
                r.records.values,
                r.records.key_work,
                max,
                r.records.cancelled,
            )?;
            Ok((0, count))
        }
        (Some(old), Some(new)) => {
            let om = r.meta(&old.child, Some(old_next))?;
            let nm = r.meta(&new.child, Some(next))?;
            if om.len == 0
                || nm.len == 0
                || new.child.leaf_count != nm.len.checked_add(4).ok_or_else(bad)?
            {
                return Err(bad());
            }
            for name in ["order_id", "bidder_agent_id"] {
                if r.point(&old.child, name)? != r.point(&new.child, name)? {
                    return Err(bad());
                }
            }
            let node = r.index(&new.child, nm.root)?;
            r.links(&new.child, nm.root, node, nm, true)?;
            let mut nodes = i128::from(om.len);
            let mut error = None;
            let result = record_diff::visit(
                &old.child,
                &new.child,
                r.records,
                cells,
                LABELS,
                |old_entry, new_entry, records| {
                    let outcome = (|| {
                        let source = new_entry.or(old_entry).ok_or_else(bad)?;
                        let sem = semantic(source, field, records, max)?;
                        let mut r = Read {
                            guard,
                            records,
                            field,
                            max,
                        };
                        match sem {
                            ChildSemantic::Field("length" | "sequence_root_id") => {
                                let Some(new_entry) = new_entry else {
                                    return Err(bad());
                                };
                                let (actual, value) = read_child_member(
                                    new_entry,
                                    field,
                                    r.records.values,
                                    max,
                                    r.records.cancelled,
                                )?;
                                if actual != sem {
                                    return Err(bad());
                                }
                                match (actual, value) {
                                    (ChildSemantic::Field("length"), ChildValue::U64(n))
                                        if n == nm.len => {}
                                    (
                                        ChildSemantic::Field("sequence_root_id"),
                                        ChildValue::U64(n),
                                    ) if n == nm.root => (),
                                    _ => return Err(bad()),
                                }
                            }
                            ChildSemantic::Bid {
                                bid_id,
                                field: "index_node",
                            } => {
                                nodes += i128::from(new_entry.is_some())
                                    - i128::from(old_entry.is_some());
                                if let Some(new_entry) = new_entry {
                                    let (actual, value) = read_child_member(
                                        new_entry,
                                        field,
                                        r.records.values,
                                        max,
                                        r.records.cancelled,
                                    )?;
                                    if actual != sem {
                                        return Err(bad());
                                    }
                                    let ChildValue::BidIndexNode(node) = value else {
                                        return Err(bad());
                                    };
                                    r.links(&new.child, bid_id, node, nm, true)?;
                                    if !r.shape(after, bid_id)? {
                                        return Err(bad());
                                    }
                                    if (old_entry.is_none()
                                        || !r.identity_same(before, after, bid_id)?)
                                        && r.pair(after, bid_id)? != digest
                                    {
                                        return Err(bad());
                                    }
                                }
                            }
                            _ => return Err(bad()),
                        }
                        Ok(())
                    })();
                    if let Err(e) = outcome {
                        error = Some(e);
                        return Err(ValueTreeError::Invalid("market changed membership group"));
                    }
                    Ok(())
                },
            );
            if (r.records.cancelled)() {
                return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
            }
            if let Some(e) = error {
                return Err(e);
            }
            result.map_err(WorldStateOwnerError::Value)?;
            if nodes != i128::from(nm.len) {
                return Err(bad());
            }
            Ok((om.len, nm.len))
        }
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "The marketplace bid delta requires distinct before/after roots, structural/value contexts, and per-group/member work budgets."
)]
pub(crate) fn bids_delta<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    G: FnMut() -> bool,
    C: FnMut() -> bool,
>(
    field: FieldDescriptor,
    before: &StateRootV1,
    after: &StateRootV1,
    guard: &StateScanReader<'_, M, G>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    group_cells: &mut u64,
    max: u64,
) -> Result<(), WorldStateOwnerError> {
    if field_from_descriptor(field) != Some(ModuleMarketplaceFieldV1::ArtifactBids) {
        return Err(bad());
    }
    let mut r = Read {
        guard,
        records,
        field,
        max,
    };
    let old = r.meta(before, None)?;
    let new = r.meta(after, None)?;
    if new.next < old.next {
        return Err(bad());
    }
    if new.len != 0 {
        let node = r.index(after, new.root)?;
        r.links(after, new.root, node, new, false)?;
    }
    let groups = i128::from(before.leaf_count) - i128::from(old.len) * 7 - 3;
    if groups < 0 {
        return Err(bad());
    }
    let mut total = Totals {
        nodes: i128::from(old.len),
        fields: i128::from(old.len) * 6,
        groups,
        members: i128::from(old.len),
    };
    let mut error = None;
    let result = record_diff::visit(
        before,
        after,
        r.records,
        cells,
        LABELS,
        |old_entry, new_entry, records| {
            let outcome = (|| {
                let source = new_entry.or(old_entry).ok_or_else(bad)?;
                let sem = semantic(source, field, records, max)?;
                let value = if let Some(new_entry) = new_entry {
                    let (actual, value) = read_child_member(
                        new_entry,
                        field,
                        records.values,
                        max,
                        records.cancelled,
                    )?;
                    if actual != sem {
                        return Err(bad());
                    }
                    Some(value)
                } else {
                    None
                };
                let mut r = Read {
                    guard,
                    records,
                    field,
                    max,
                };
                match sem {
                    ChildSemantic::Field(name) => {
                        let Some(ChildValue::U64(value)) = value else {
                            return Err(bad());
                        };
                        if !matches!((name,value),("length",n) if n==new.len)
                            && !matches!((name,value),("next_bid_id",n) if n==new.next)
                            && !matches!((name,value),("sequence_root_id",n) if n==new.root)
                        {
                            return Err(bad());
                        }
                    }
                    ChildSemantic::Bid {
                        bid_id,
                        field: name,
                    } => {
                        if new_entry.is_some() && bid_id >= new.next {
                            return Err(bad());
                        }
                        let alive = r.shape(after, bid_id)?;
                        if name == "index_node" {
                            total.nodes +=
                                i128::from(new_entry.is_some()) - i128::from(old_entry.is_some());
                            if let Some(ChildValue::BidIndexNode(node)) = value {
                                r.links(after, bid_id, node, new, false)?;
                            } else if new_entry.is_some() {
                                return Err(bad());
                            }
                        } else {
                            if !matches!(
                                name,
                                "present"
                                    | "order_id"
                                    | "bidder_agent_id"
                                    | "price_kind"
                                    | "price_amount"
                                    | "bid_at"
                            ) {
                                return Err(bad());
                            }
                            total.fields +=
                                i128::from(new_entry.is_some()) - i128::from(old_entry.is_some());
                            if value.is_some_and(|value| !valid_business(name, value)) {
                                return Err(bad());
                            }
                            if matches!(name, "present" | "order_id" | "bidder_agent_id") {
                                let old_alive =
                                    r.point(before, &(("bid", bid_id), "present"))?.is_some();
                                r.primary_pair(before, after, bid_id, old_alive, alive, new.next)?;
                            }
                        }
                    }
                    ChildSemantic::BidGroup(digest) => {
                        total.groups +=
                            i128::from(new_entry.is_some()) - i128::from(old_entry.is_some());
                        let (old_len, new_len) = group_delta(
                            before,
                            after,
                            digest,
                            guard,
                            field,
                            r.records,
                            group_cells,
                            max,
                            old.next,
                            new.next,
                        )?;
                        total.members += i128::from(new_len) - i128::from(old_len);
                    }
                }
                Ok(())
            })();
            if let Err(e) = outcome {
                error = Some(e);
                return Err(ValueTreeError::Invalid("market changed bid graph"));
            }
            Ok(())
        },
    );
    if (r.records.cancelled)() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    if let Some(e) = error {
        return Err(e);
    }
    result.map_err(WorldStateOwnerError::Value)?;
    let len = i128::from(new.len);
    if total.nodes != len
        || total.fields != len * 6
        || total.members != len
        || total.groups < 0
        || i128::from(after.leaf_count) != len * 7 + 3 + total.groups
    {
        return Err(bad());
    }
    Ok(())
}
