//! Typed changed-path validation. Parent cells must already be fully qualified.
//! Raw references deliberately avoid loading removed or replaced text payloads.
use crate::incremental_typed_record_codec::*;
use oasis7_formal_protocol::incremental_state::{
    STATE_MAX_PAGE_BYTES, StateEntryV1, StateNodeWriteV1, StatePageLinksV1,
};

pub(crate) enum TypedCellRefV1<'a> {
    Entry(&'a StateEntryV1),
    /// Only the owner may use this after authenticating the outer reference.
    Body(&'a StateRootV1),
}
impl<'a> TypedCellRefV1<'a> {
    fn entry(&self) -> Result<&'a StateEntryV1, ValueTreeError> {
        match self {
            Self::Entry(e) => Ok(e),
            Self::Body(_) => Err(invalid()),
        }
    }
}

pub(crate) struct ChangeDecoder<'a, 'v, M, R, E, C> {
    pub(crate) ctx: &'a mut WorldFieldReadContextV1<'v, M, R, E, C>,
    pub(crate) scan: &'a mut StateScanBudget,
    pub(crate) work: &'a mut TypedRecordReadWorkV1,
    pub(crate) profile: TypedRecordProfileV1,
    pub(crate) trees_remaining: usize,
    pub(crate) tree_depth: usize,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("closed typed changed record")
}
struct Frame {
    old: Option<String>,
    new: Option<String>,
    depth: u16,
    path: [u8; 32],
}
impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> ChangeDecoder<'_, '_, M, R, E, C>
{
    pub(crate) fn check(&mut self) -> Result<(), ValueTreeError> {
        self.ctx.check()?;
        if let Some(error) = (self.ctx.take_state_error)() {
            return Err(error);
        }
        Ok(())
    }
    fn page(
        &mut self,
        id: Option<&str>,
        depth: u16,
    ) -> Result<Option<StatePageLinksV1>, ValueTreeError> {
        let Some(id) = id else { return Ok(None) };
        self.check()?;
        self.scan.nodes_remaining = self
            .scan
            .nodes_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("typed diff nodes"))?;
        let _charge = self.ctx.values.memory.reserve(
            STATE_MAX_PAGE_BYTES
                .checked_mul(2)
                .and_then(|n| n.checked_add(1024))
                .ok_or_else(invalid)?,
            self.ctx.values.class,
        )?;
        let result = self.ctx.state.read_node(id, STATE_MAX_PAGE_BYTES);
        if let Some(error) = (self.ctx.take_state_error)() {
            return Err(error);
        }
        self.check()?;
        let node = result.map_err(|_| invalid())?.ok_or_else(invalid)?;
        self.scan.read_bytes_remaining = self
            .scan
            .read_bytes_remaining
            .checked_sub(node.as_ref().len() as u64)
            .ok_or(ValueTreeError::ResourceLimited("typed diff bytes"))?;
        let page = StateNodeWriteV1 {
            digest: id.to_owned(),
            bytes: node.as_ref().to_vec(),
        }
        .verified_links()
        .map_err(|_| invalid())?;
        match &page {
            StatePageLinksV1::Branch { depth: stored, .. } if *stored == depth && depth < 256 => {}
            StatePageLinksV1::Value(_) if depth == 256 => {}
            _ => return Err(invalid()),
        }
        Ok(Some(page))
    }
}
/// Owns only the fixed-depth traversal stack. Recursive validators run after
/// page-reader borrows end, so no uncharged delta staging buffer is needed.
fn visit_entries_inner<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &StateRootV1,
    after: &StateRootV1,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    mut visitor: impl FnMut(
        Option<&TypedCellRefV1<'_>>,
        Option<&TypedCellRefV1<'_>>,
        &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    before.validate().map_err(|_| invalid())?;
    after.validate().map_err(|_| invalid())?;
    if before.namespace != after.namespace || before.namespace != d.profile.namespace()? {
        return Err(invalid());
    }
    if before == after {
        return Ok(());
    }
    let _charge = d.ctx.values.memory.reserve(96 << 10, d.ctx.values.class)?;
    let empty = StateRootV1::empty(before.namespace).map_err(|_| invalid())?;
    let root = |r: &StateRootV1| {
        if r.node_root == empty.node_root {
            None
        } else {
            Some(r.node_root.clone())
        }
    };
    let mut stack = Vec::with_capacity(257);
    stack.push(Frame {
        old: root(before),
        new: root(after),
        depth: 0,
        path: [0; 32],
    });
    let (mut added, mut removed) = (0u64, 0u64);
    while let Some(frame) = stack.pop() {
        d.check()?;
        if frame.old == frame.new {
            continue;
        }
        let old = d.page(frame.old.as_deref(), frame.depth)?;
        let new = d.page(frame.new.as_deref(), frame.depth)?;
        if frame.depth == 256 {
            d.scan.leaves_remaining = d
                .scan
                .leaves_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("typed diff leaves"))?;
            let leaf = |p| match p {
                None => Ok(None),
                Some(StatePageLinksV1::Value(e)) => Ok(Some(e)),
                _ => Err(invalid()),
            };
            let old = leaf(old)?;
            let new = leaf(new)?;
            for e in old.iter().chain(new.iter()) {
                if e.namespace != before.namespace
                    || e.key_digest().map_err(|_| invalid())? != frame.path
                {
                    return Err(invalid());
                }
            }
            if let (Some(a), Some(b)) = (&old, &new) {
                if a.key != b.key {
                    return Err(invalid());
                }
                if a.value == b.value {
                    continue;
                }
            }
            if old.is_none() {
                added = added.checked_add(1).ok_or_else(invalid)?
            }
            if new.is_none() {
                removed = removed.checked_add(1).ok_or_else(invalid)?
            }
            let old = old.as_ref().map(|entry| TypedCellRefV1::Entry(entry));
            let new = new.as_ref().map(|entry| TypedCellRefV1::Entry(entry));
            visitor(old.as_ref(), new.as_ref(), d)?;
        } else {
            let children = |p| match p {
                None => Ok([None, None]),
                Some(StatePageLinksV1::Branch { children, .. }) => Ok(children),
                _ => Err(invalid()),
            };
            for (index, (old, new)) in children(old)?
                .into_iter()
                .zip(children(new)?)
                .enumerate()
                .rev()
            {
                if old == new {
                    continue;
                }
                let mut path = frame.path;
                if index == 1 {
                    path[usize::from(frame.depth) / 8] |= 0x80 >> (frame.depth % 8)
                }
                if stack.len() == 257 {
                    return Err(ValueTreeError::ResourceLimited("typed diff stack"));
                }
                stack.push(Frame {
                    old,
                    new,
                    depth: frame.depth + 1,
                    path,
                });
            }
        }
    }
    if before
        .leaf_count
        .checked_add(added)
        .and_then(|n| n.checked_sub(removed))
        != Some(after.leaf_count)
    {
        return Err(invalid());
    }
    d.check()
}

fn columns_profile(profile: &TypedRecordProfileV1) -> ColumnReadProfileV1 {
    ColumnReadProfileV1 {
        field: profile.field,
        schema: profile.schema,
        groups: &[],
        reference_schema: profile.reference_schema,
    }
}
pub(crate) fn unchanged(
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
) -> bool {
    match (before, after) {
        (Some(a), Some(b)) => match (a, b) {
            (TypedCellRefV1::Entry(a), TypedCellRefV1::Entry(b)) => {
                a.key == b.key && a.value == b.value
            }
            (TypedCellRefV1::Body(a), TypedCellRefV1::Body(b)) => a == b,
            _ => false,
        },
        (None, None) => true,
        _ => false,
    }
}
fn row<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<
    oasis7_proto::memory_budget::ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>,
    ValueTreeError,
> {
    let ctx = &mut *d.ctx;
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    crate::incremental_record_columns_read::read_column_row_v1(
        columns_profile(profile),
        cell.entry()?,
        &mut records,
        d.work.max_tree_heap,
    )
}
fn key<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<oasis7_proto::memory_budget::ReservedValue<ColumnKeyV1>, ValueTreeError> {
    let ctx = &mut *d.ctx;
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    crate::incremental_record_columns_read::read_column_key_v1(
        columns_profile(profile),
        cell.entry()?,
        &mut records,
        d.work.max_tree_heap,
    )
}
pub(crate) fn root<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<oasis7_proto::memory_budget::ReservedValue<StateRootV1>, ValueTreeError> {
    if let TypedCellRefV1::Body(root) = cell {
        root.validate().map_err(|_| invalid())?;
        if root.namespace != profile.namespace()? {
            return Err(invalid());
        }
        return Ok(d
            .ctx
            .values
            .memory
            .reserve(512, d.ctx.values.class)?
            .retain((*root).clone()));
    }
    let row = row(profile, cell, d)?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid());
    };
    Ok(d.ctx
        .values
        .memory
        .reserve(512, d.ctx.values.class)?
        .retain(root.clone()))
}
fn clone_cell(cell: &ColumnCellV1) -> ColumnCellV1 {
    match cell {
        ColumnCellV1::Unit => ColumnCellV1::Unit,
        ColumnCellV1::Bool(v) => ColumnCellV1::Bool(*v),
        ColumnCellV1::U64(v) => ColumnCellV1::U64(*v),
        ColumnCellV1::I64(v) => ColumnCellV1::I64(*v),
        ColumnCellV1::Text(v) => ColumnCellV1::Text(v.clone()),
        ColumnCellV1::Root(v) => ColumnCellV1::Root(v.clone()),
    }
}
/// Default validation loads only the new changed value. Old primitive payloads
/// inherit the accepted parent's qualification and are never materialized.
pub(crate) fn accept_fallback<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if unchanged(before, after) {
        return Ok(());
    }
    let Some(after) = after else { return Ok(()) };
    if let TypedCellRefV1::Body(root) = after {
        let _copy = d.ctx.values.memory.reserve(512, d.ctx.values.class)?;
        return crate::incremental_typed_record_codec::validate_typed_cell_v1::<T, _, _, _, _>(
            ColumnCellV1::Root((*root).clone()),
            profile,
            d.ctx,
            d.scan,
            d.work,
        );
    }
    let row = row(profile, after, d)?;
    let bytes = match &row.cell {
        ColumnCellV1::Text(v) => v
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(512))
            .ok_or_else(invalid)?,
        _ => 512,
    };
    let _copy = d.ctx.values.memory.reserve(bytes, d.ctx.values.class)?;
    crate::incremental_typed_record_codec::validate_typed_cell_v1::<T, _, _, _, _>(
        clone_cell(&row.cell),
        profile,
        d.ctx,
        d.scan,
        d.work,
    )
}
pub(crate) fn accept_projected<
    Owner,
    Field: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    _projection: fn(&Owner) -> &Field,
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    Field::accept_changed(profile, before, after, d)
}
/// Both roots exist and are qualified as the same fixed-schema DTO. Every
/// changed mandatory column remains present; unknown columns are rejected by
/// the exhaustive generated visitor. Unchanged siblings are not read.
pub(crate) fn accept_struct<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    names: &'static [&'static str],
    mut visitor: impl FnMut(
        &str,
        Option<&TypedCellRefV1<'_>>,
        Option<&TypedCellRefV1<'_>>,
        &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if unchanged(before, after) {
        return Ok(());
    }
    let Some(after) = after else { return Ok(()) };
    let Some(before) = before else {
        return full::fields(profile, after, d, names, visitor);
    };
    let before = root(profile, before, d)?;
    let after = root(profile, after, d)?;
    visit_entries(&before, &after, d, |old, new, d| {
        let old = old.ok_or_else(invalid)?;
        let new = new.ok_or_else(invalid)?;
        let name = key(profile, new, d)?;
        let ColumnKeyV1::Text(name) = &*name else {
            return Err(invalid());
        };
        visitor(name, Some(old), Some(new), d)
    })
}
/// Validate direct children without imposing DTO field-name restrictions.
fn accept_container<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    vector: bool,
    mut entry: impl FnMut(
        &ColumnKeyV1,
        Option<&TypedCellRefV1<'_>>,
        Option<&TypedCellRefV1<'_>>,
        &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if unchanged(before, after) {
        return Ok(());
    }
    let Some(after) = after else { return Ok(()) };
    let Some(before) = before else {
        return full::container(profile, after, d, vector, entry);
    };
    let before = root(profile, before, d)?;
    let after = root(profile, after, d)?;
    let old_length = before.leaf_count.checked_sub(2).ok_or_else(invalid)?;
    let new_length = after.leaf_count.checked_sub(2).ok_or_else(invalid)?;
    let mut length = old_length;
    visit_entries(&before, &after, d, |old, new, d| {
        let name = key(profile, new.or(old).ok_or_else(invalid)?, d)?;
        if matches!(&*name,ColumnKeyV1::Text(s) if s=="_length") {
            if old.is_none() {
                return Err(invalid());
            }
            let row = row(profile, new.ok_or_else(invalid)?, d)?;
            let ColumnCellV1::U64(value) = &row.cell else {
                return Err(invalid());
            };
            length = *value;
            return Ok(());
        }
        if vector {
            let ColumnKeyV1::Ordinal(index) = &*name else {
                return Err(invalid());
            };
            if (old.is_some() && *index >= old_length)
                || (new.is_some() && *index >= new_length)
                || (old.is_none() && *index < old_length)
                || (new.is_none() && *index < new_length)
            {
                return Err(invalid());
            }
        }
        entry(&name, old, new, d)
    })?;
    if length != new_length {
        return Err(invalid());
    }
    d.check()
}
pub(crate) fn accept_vec<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    accept_container::<Vec<T>, _, _, _, _>(profile, before, after, d, true, |_, old, new, d| {
        T::accept_changed(profile, old, new, d)
    })
}

fn is_unit<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<bool, ValueTreeError> {
    if matches!(cell, TypedCellRefV1::Body(_)) {
        return Ok(false);
    }
    let _key = key(profile, cell, d)?;
    let parsed =
        WorldFieldKeyV1::from_bytes(&cell.entry()?.key).map_err(ValueTreeError::Invalid)?;
    let expected = WorldFieldKeyV1::new(profile.field, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let header = read_member_record_header_v1(
        &cell.entry()?.value,
        &expected,
        &mut d.ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: d.ctx.semantic_bytes,
            payload_bytes: d.ctx.record_bytes,
            record_bytes: d.ctx.record_bytes,
        },
        &mut d.ctx.cancelled,
    )?;
    Ok(header.codec == MemberPayloadCodecV1::Unit)
}
pub(crate) fn accept_option<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if unchanged(before, after) {
        return Ok(());
    }
    let Some(after) = after else { return Ok(()) };
    if is_unit(profile, after, d)? {
        return accept_fallback::<Option<T>, _, _, _, _>(profile, before, Some(after), d);
    }
    let before = match before {
        Some(old) if !is_unit(profile, old, d)? => Some(old),
        _ => None,
    };
    T::accept_changed(profile, before, Some(after), d)
}
pub(crate) fn accept_box<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    T::accept_changed(profile, before, after, d)
}

/// Bound recursive typed nesting separately from the aggregate tree-work quota.
pub(crate) fn visit_entries<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &StateRootV1,
    after: &StateRootV1,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    visitor: impl FnMut(
        Option<&TypedCellRefV1<'_>>,
        Option<&TypedCellRefV1<'_>>,
        &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if before == after {
        return Ok(());
    }
    if d.tree_depth >= 64 {
        return Err(ValueTreeError::ResourceLimited("typed diff nesting"));
    }
    d.trees_remaining = d
        .trees_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("typed diff trees"))?;
    d.tree_depth += 1;
    let result = visit_entries_inner(before, after, d, visitor);
    d.tree_depth -= 1;
    result
}

fn check_tuple_size<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    n: u64,
) -> Result<(), ValueTreeError> {
    if let Some(after) = after {
        if root(profile, after, d)?.leaf_count != n + 2 {
            return Err(invalid());
        }
    }
    Ok(())
}
pub(crate) fn accept_tuple2<
    A: Codec,
    B: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    if unchanged(before, after) {
        return d.check();
    }
    check_tuple_size(profile, after, d, 2)?;
    let inherited = before.is_some();
    accept_container::<(A, B), _, _, _, _>(profile, before, after, d, true, |name, old, new, d| {
        if (inherited && old.is_none()) || new.is_none() {
            return Err(invalid());
        }
        match name {
            ColumnKeyV1::Ordinal(0) => A::accept_changed(profile, old, new, d),
            ColumnKeyV1::Ordinal(1) => B::accept_changed(profile, old, new, d),
            _ => Err(invalid()),
        }
    })
}
pub(crate) fn accept_tuple3<
    A: Codec,
    B: Codec,
    D: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    if unchanged(before, after) {
        return d.check();
    }
    check_tuple_size(profile, after, d, 3)?;
    let inherited = before.is_some();
    accept_container::<(A, B, D), _, _, _, _>(
        profile,
        before,
        after,
        d,
        true,
        |name, old, new, d| {
            if (inherited && old.is_none()) || new.is_none() {
                return Err(invalid());
            }
            match name {
                ColumnKeyV1::Ordinal(0) => A::accept_changed(profile, old, new, d),
                ColumnKeyV1::Ordinal(1) => B::accept_changed(profile, old, new, d),
                ColumnKeyV1::Ordinal(2) => D::accept_changed(profile, old, new, d),
                _ => Err(invalid()),
            }
        },
    )
}

pub(crate) fn accept_map_key<
    K: CollectionKey,
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    accept_container::<std::collections::BTreeMap<K, T>, _, _, _, _>(
        profile,
        before,
        after,
        d,
        false,
        |name, old, new, d| {
            match name {
                ColumnKeyV1::Pair(tag, k)
                    if matches!(&**tag,ColumnKeyV1::Text(s) if s=="entry")
                        && K::accepts_column(k) => {}
                _ => return Err(invalid()),
            }
            T::accept_changed(profile, old, new, d)
        },
    )
}
pub(crate) fn accept_set_key<
    K: CollectionKey,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    accept_container::<std::collections::BTreeSet<K>, _, _, _, _>(
        profile,
        before,
        after,
        d,
        false,
        |name, _, new, d| {
            match name {
                ColumnKeyV1::Pair(tag, k)
                    if matches!(&**tag,ColumnKeyV1::Text(s) if s=="entry")
                        && K::accepts_column(k) => {}
                _ => return Err(invalid()),
            }
            if let Some(new) = new {
                let row = row(profile, new, d)?;
                if !matches!(row.cell, ColumnCellV1::Bool(true)) {
                    return Err(invalid());
                }
            }
            Ok(())
        },
    )
}

/// The owner has already authenticated both outer reference bindings. This
/// validates the body delta without interpreting that outer key as a column.
pub(crate) fn accept_body<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    let before = before.map(TypedCellRefV1::Body);
    let after = after.map(TypedCellRefV1::Body);
    T::accept_changed(profile, before.as_ref(), after.as_ref(), d)
}

#[path = "incremental_typed_record_full.rs"]
mod full;

/// Text validation streams UTF8 to a sink; full-family qualification does not
/// allocate a String merely to prove its primitive codec.
pub(crate) fn accept_text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if unchanged(before, after) {
        return Ok(());
    }
    let Some(after) = after else { return Ok(()) };
    let _key = key(profile, after, d)?;
    let entry = after.entry()?;
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let expected = WorldFieldKeyV1::new(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    let header = read_member_record_header_v1(
        &entry.value,
        &expected,
        &mut d.ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: d.ctx.semantic_bytes,
            payload_bytes: d.ctx.record_bytes,
            record_bytes: d.ctx.record_bytes,
        },
        &mut d.ctx.cancelled,
    )?;
    use crate::incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, read_record_value_v1,
    };
    let ctx = &mut *d.ctx;
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let limit = records.record_bytes;
    read_record_value_v1(
        &entry.value,
        &(expected, header.semantic_bytes),
        &mut records,
        limit,
        &mut std::io::sink(),
        RecordReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            nullable: false,
            integer_max: u64::MAX,
            labels: RecordReadLabelsV1 {
                utf8: "typed text UTF8",
                ledger_utf8: "typed text ledger",
                ledger_frame: "typed text ledger",
                boolean: "typed text bool",
                fixed: "typed text fixed",
                profile: "typed text codec",
                integer: "typed text integer",
            },
        },
    )?;
    d.check()
}

#[path = "incremental_typed_record_tagged_diff.rs"]
mod tagged;
pub(crate) use tagged::tagged_choice_v1;

#[path = "incremental_typed_record_point.rs"]
mod point;
pub(crate) use point::point_typed_column_v1;
