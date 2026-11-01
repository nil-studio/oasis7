//! Fixed-depth authenticated child diffs. Equal qualified subtrees are skipped.
use crate::incremental_inventory_value_read::InventoryValueReader;
use crate::incremental_record_tree::{RecordTreeLabelsV1, Source};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, STATE_MAX_PAGE_BYTES, StateEntryV1, StateNodeWriteV1, StatePageLinksV1,
        StateRootV1,
    },
    incremental_value::ValueNodeReader,
};
use std::cell::RefCell;

#[derive(Clone, Copy)]
pub(crate) struct RecordDiffLabelsV1 {
    pub invalid: &'static str,
    pub cells: &'static str,
    pub stack: &'static str,
    pub tree: RecordTreeLabelsV1,
}
struct Frame {
    old: Option<String>,
    new: Option<String>,
    depth: u16,
    path: [u8; 32],
}
fn read<R: ValueNodeReader, C: FnMut() -> bool>(
    source: &Source<'_, '_, R, C>,
    id: Option<&String>,
    depth: u16,
    labels: RecordDiffLabelsV1,
) -> Result<Option<StatePageLinksV1>, ValueTreeError> {
    let Some(id) = id else { return Ok(None) };
    let page = source
        .finish(source.read_node(id, STATE_MAX_PAGE_BYTES))?
        .ok_or(ValueTreeError::Invalid(labels.invalid))?;
    let page = StateNodeWriteV1 {
        digest: id.clone(),
        bytes: page.as_ref().to_vec(),
    }
    .verified_links()
    .map_err(|_| ValueTreeError::Invalid(labels.invalid))?;
    match &page {
        StatePageLinksV1::Branch { depth: stored, .. } if *stored == depth && depth < 256 => {}
        StatePageLinksV1::Value(_) if depth == 256 => {}
        _ => return Err(ValueTreeError::Invalid(labels.invalid)),
    }
    Ok(Some(page))
}
/// The old root is already fully qualified. Only changed paths are decoded;
/// every changed leaf is checked against its actual 256-bit traversal position.
pub(crate) fn visit<R: ValueNodeReader, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    labels: RecordDiffLabelsV1,
    mut visitor: impl FnMut(
        Option<&StateEntryV1>,
        Option<&StateEntryV1>,
        &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    if before.namespace != after.namespace {
        return Err(ValueTreeError::Invalid(labels.invalid));
    }
    let _workspace = records
        .values
        .memory
        .reserve(96 << 10, records.values.class)?;
    before
        .validate()
        .map_err(|_| ValueTreeError::Invalid(labels.invalid))?;
    after
        .validate()
        .map_err(|_| ValueTreeError::Invalid(labels.invalid))?;
    let empty = StateRootV1::empty(before.namespace)
        .map_err(|_| ValueTreeError::Invalid(labels.invalid))?;
    let nonempty = |root: &StateRootV1| {
        if root.node_root == empty.node_root {
            None
        } else {
            Some(root.node_root.clone())
        }
    };
    let mut stack = Vec::with_capacity(257);
    stack.push(Frame {
        old: nonempty(before),
        new: nonempty(after),
        depth: 0,
        path: [0; 32],
    });
    let mut added = 0u64;
    let mut removed = 0u64;
    while let Some(frame) = stack.pop() {
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if frame.old == frame.new {
            continue;
        }
        *cells = cells
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(labels.cells))?;
        let (old, new) = {
            let source = Source {
                source: records.values.source,
                memory: records.values.memory,
                class: records.values.class,
                io: &RefCell::new(&mut *records.values.io),
                cancelled: &RefCell::new(&mut *records.cancelled),
                error: &RefCell::new(None),
                labels: labels.tree,
            };
            (
                read(&source, frame.old.as_ref(), frame.depth, labels)?,
                read(&source, frame.new.as_ref(), frame.depth, labels)?,
            )
        };
        if frame.depth == 256 {
            let leaf = |node| match node {
                None => Ok(None),
                Some(StatePageLinksV1::Value(entry)) => Ok(Some(entry)),
                _ => Err(ValueTreeError::Invalid(labels.invalid)),
            };
            let old = leaf(old)?;
            let new = leaf(new)?;
            for entry in old.iter().chain(new.iter()) {
                if entry.namespace != before.namespace
                    || entry
                        .key_digest()
                        .map_err(|_| ValueTreeError::Invalid(labels.invalid))?
                        != frame.path
                {
                    return Err(ValueTreeError::Invalid(labels.invalid));
                }
            }
            if let (Some(old), Some(new)) = (&old, &new)
                && old.key != new.key
            {
                return Err(ValueTreeError::Invalid(labels.invalid));
            }
            if old.is_none() {
                added = added
                    .checked_add(1)
                    .ok_or(ValueTreeError::Invalid(labels.invalid))?;
            }
            if new.is_none() {
                removed = removed
                    .checked_add(1)
                    .ok_or(ValueTreeError::Invalid(labels.invalid))?;
            }
            visitor(old.as_ref(), new.as_ref(), records)?;
        } else {
            let children = |node| match node {
                None => Ok([None, None]),
                Some(StatePageLinksV1::Branch { children, .. }) => Ok(children),
                _ => Err(ValueTreeError::Invalid(labels.invalid)),
            };
            let old = children(old)?;
            let new = children(new)?;
            for (index, (old, new)) in old.into_iter().zip(new).enumerate().rev() {
                if old == new {
                    continue;
                }
                let mut path = frame.path;
                if index == 1 {
                    path[usize::from(frame.depth) / 8] |= 0x80 >> (frame.depth % 8);
                }
                if stack.len() == 257 {
                    return Err(ValueTreeError::ResourceLimited(labels.stack));
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
        return Err(ValueTreeError::Invalid(labels.invalid));
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
