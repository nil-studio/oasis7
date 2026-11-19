use super::*;
use crate::incremental_record_tree::{RecordTreeLabelsV1, Source};
use oasis7_formal_protocol::incremental_schedule::{
    SCHEDULE_MAX_PAGE_BYTES, SchedulePageKindV1 as Kind, SchedulePageLinksV1 as Links,
    SchedulePageWriteV1,
};
#[derive(Clone, Copy)]
struct Frame {
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
    depth: u16,
    path: [u8; 32],
}
fn hash(id: &str) -> Result<[u8; 32], ValueTreeError> {
    Ok(*blake3::Hash::from_hex(id)
        .map_err(|_| invalid())?
        .as_bytes())
}
fn root(index: &ScheduleIndexV1) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    if index.node_root == ScheduleIndexV1::empty()?.node_root {
        Ok(None)
    } else {
        Ok(Some(hash(&index.node_root)?))
    }
}
fn time_key(wake: u64) -> [u8; 32] {
    let mut out = [0; 32];
    out[..8].copy_from_slice(&wake.to_be_bytes());
    out
}
fn instance_key(instance: &str) -> Result<[u8; 32], ValueTreeError> {
    if instance.is_empty() || instance.len() > 1024 {
        return Err(invalid());
    }
    let mut h = blake3::Hasher::new();
    h.update(b"oasis7.iws.schedule-instance.v1\0");
    h.update(&(instance.len() as u16).to_be_bytes());
    h.update(instance.as_bytes());
    Ok(*h.finalize().as_bytes())
}
fn page<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: Option<[u8; 32]>,
    kind: Kind,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawScheduleWorkV1,
) -> Result<Option<Links>, WorldFieldOverlayErrorV1> {
    let Some(id) = id else { return Ok(None) };
    check(ctx)?;
    work.index.nodes_remaining = work
        .index
        .nodes_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("raw schedule index nodes"))?;
    ctx.values.io.nodes_remaining = ctx
        .values
        .io
        .nodes_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("raw schedule value nodes"))?;
    let admitted = SCHEDULE_MAX_PAGE_BYTES.min(
        work.index
            .read_bytes_remaining
            .min(ctx.values.io.read_bytes_remaining)
            .min(usize::MAX as u64) as usize,
    );
    if admitted == 0 {
        return Err(ValueTreeError::ResourceLimited("raw schedule index bytes").into());
    }
    work.index.read_bytes_remaining -= admitted as u64;
    ctx.values.io.read_bytes_remaining -= admitted as u64;
    let bytes = ctx
        .values
        .source
        .read_node(&id, admitted, ctx.values.memory, ctx.values.class)?;
    if bytes.len() > admitted
        || !bytes.uses_budget(ctx.values.memory)
        || bytes.reserved_bytes() < bytes.capacity()
    {
        return Err(invalid().into());
    }
    let refund = (admitted - bytes.len()) as u64;
    work.index.read_bytes_remaining += refund;
    ctx.values.io.read_bytes_remaining += refund;
    // The enclosing 128KiB phase bounds both fixed 257-frame stacks, source
    // copies, parsed page strings and this at-most-1200-byte canonical payload.
    let page = SchedulePageWriteV1 {
        kind,
        digest: blake3::Hash::from(id).to_hex().to_string(),
        bytes: bytes.as_slice().to_vec(),
    };
    let links = page.verified_links()?;
    check(ctx)?;
    Ok(Some(links))
}
fn children(node: &Option<Links>, depth: u16) -> Result<[Option<[u8; 32]>; 2], ValueTreeError> {
    match node {
        None => Ok([None; 2]),
        Some(Links::Branch {
            depth: actual,
            children,
            ..
        }) if *actual == depth && depth < 256 => Ok(*children),
        _ => Err(invalid()),
    }
}
fn walk<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    F: FnMut(
        Option<Links>,
        Option<Links>,
        [u8; 32],
        &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        &mut WorldRawScheduleWorkV1,
    ) -> Result<(), WorldFieldOverlayErrorV1>,
>(
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
    kind: Kind,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawScheduleWorkV1,
    mut leaf: F,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let mut stack = Vec::with_capacity(257);
    stack.push(Frame {
        before,
        after,
        depth: 0,
        path: [0; 32],
    });
    while let Some(frame) = stack.pop() {
        check(ctx)?;
        if frame.before == frame.after {
            continue;
        }
        let old = page(frame.before, kind, ctx, work)?;
        let new = page(frame.after, kind, ctx, work)?;
        if frame.depth == 256 {
            if old
                .as_ref()
                .is_some_and(|n| matches!(n, Links::Branch { .. }))
                || new
                    .as_ref()
                    .is_some_and(|n| matches!(n, Links::Branch { .. }))
            {
                return Err(invalid().into());
            }
            match kind {
                Kind::Times => {
                    work.index.buckets_remaining =
                        work.index.buckets_remaining.checked_sub(1).ok_or(
                            ValueTreeError::ResourceLimited("raw schedule changed buckets"),
                        )?;
                }
                Kind::Instances => {
                    work.index.instances_remaining =
                        work.index.instances_remaining.checked_sub(1).ok_or(
                            ValueTreeError::ResourceLimited("raw schedule changed instances"),
                        )?;
                }
            }
            leaf(old, new, frame.path, ctx, work)?;
        } else {
            let old = children(&old, frame.depth)?;
            let new = children(&new, frame.depth)?;
            for side in (0..2).rev() {
                if old[side] == new[side] {
                    continue;
                }
                if stack.len() == 257 {
                    return Err(invalid().into());
                }
                let mut path = frame.path;
                if side == 1 {
                    path[frame.depth as usize / 8] |= 1 << (7 - frame.depth % 8);
                }
                stack.push(Frame {
                    before: old[side],
                    after: new[side],
                    depth: frame.depth + 1,
                    path,
                });
            }
        }
    }
    check(ctx)?;
    Ok(())
}
fn bucket(
    links: Option<Links>,
    path: [u8; 32],
) -> Result<Option<(u64, u64, [u8; 32])>, ValueTreeError> {
    match links {
        None => Ok(None),
        Some(Links::Bucket {
            wake,
            count,
            instances,
        }) if path == time_key(wake) => Ok(Some((wake, count, instances))),
        _ => Err(invalid()),
    }
}
fn instance(links: Option<Links>, path: [u8; 32]) -> Result<Option<String>, ValueTreeError> {
    match links {
        None => Ok(None),
        Some(Links::Instance(id)) if instance_key(&id)? == path => Ok(Some(id)),
        _ => Err(invalid()),
    }
}
fn scalar<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    instance: &str,
    roots: &RootSetV1,
    record: Option<&TransitionRecordV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<u64>, ValueTreeError> {
    let key = ModuleWakeKeyV1::new(instance).map_err(ValueTreeError::Invalid)?;
    after_value(roots, record, key.as_bytes(), ctx)?
        .map(|id| wake(&id, ctx))
        .transpose()
}
pub(super) fn advance<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &ScheduleIndexV1,
    after: &ScheduleIndexV1,
    roots: &RootSetV1,
    record: Option<&TransitionRecordV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawScheduleWorkV1,
) -> Result<(), WorldFieldOverlayErrorV1> {
    before.validate()?;
    after.validate()?;
    let mut entries = i128::from(before.entries);
    let mut buckets = i128::from(before.buckets);
    walk(
        root(before)?,
        root(after)?,
        Kind::Times,
        ctx,
        work,
        |old, new, path, ctx, work| {
            let old = bucket(old, path)?;
            let new = bucket(new, path)?;
            let wake = old.as_ref().or(new.as_ref()).ok_or_else(invalid)?.0;
            let mut count = i128::from(old.as_ref().map_or(0, |v| v.1));
            walk(
                old.as_ref().map(|v| v.2),
                new.as_ref().map(|v| v.2),
                Kind::Instances,
                ctx,
                work,
                |old, new, path, ctx, _work| {
                    let old = instance(old, path)?;
                    let new = instance(new, path)?;
                    if let (Some(a), Some(b)) = (&old, &new)
                        && a != b
                    {
                        return Err(invalid().into());
                    }
                    if let Some(id) = old.as_ref()
                        && new.is_none()
                        && scalar(id, roots, record, ctx)? == Some(wake)
                    {
                        return Err(invalid().into());
                    }
                    if let Some(id) = new.as_ref()
                        && scalar(id, roots, record, ctx)? != Some(wake)
                    {
                        return Err(invalid().into());
                    }
                    count += i128::from(new.is_some()) - i128::from(old.is_some());
                    Ok(())
                },
            )?;
            if u64::try_from(count).map_err(|_| invalid())? != new.as_ref().map_or(0, |v| v.1) {
                return Err(invalid().into());
            }
            entries += i128::from(new.as_ref().map_or(0, |v| v.1))
                - i128::from(old.as_ref().map_or(0, |v| v.1));
            buckets += i128::from(new.is_some()) - i128::from(old.is_some());
            Ok(())
        },
    )?;
    if u64::try_from(entries).map_err(|_| invalid())? != after.entries
        || u64::try_from(buckets).map_err(|_| invalid())? != after.buckets
    {
        return Err(invalid().into());
    }
    check(ctx)?;
    Ok(())
}
// Point membership consumes the same index scan allowance as delta pages,
// in addition to the caller's original value-I/O allowance in Source.
struct PointReader<'a, T> {
    source: &'a T,
    nodes: &'a std::cell::Cell<u64>,
    bytes: &'a std::cell::Cell<u64>,
    error: &'a RefCell<Option<ValueTreeError>>,
}
impl<T: MerkleNodeReader> MerkleNodeReader for PointReader<'_, T> {
    type Node = T::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let result = (|| {
            let nodes = self
                .nodes
                .get()
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("raw schedule point nodes"))?;
            let max = max.min(self.bytes.get().min(usize::MAX as u64) as usize);
            if max == 0 {
                return Err(ValueTreeError::ResourceLimited("raw schedule point bytes"));
            }
            self.nodes.set(nodes);
            self.bytes.set(self.bytes.get() - max as u64);
            Ok(max)
        })();
        let max = match result {
            Ok(n) => n,
            Err(e) => {
                if self.error.borrow().is_none() {
                    *self.error.borrow_mut() = Some(e);
                }
                return Err("raw schedule point allowance".into());
            }
        };
        let node = self.source.read_node(id, max)?;
        if let Some(node) = &node {
            if node.as_ref().len() > max {
                if self.error.borrow().is_none() {
                    *self.error.borrow_mut() = Some(invalid());
                }
                return Err("raw schedule point source bound".into());
            }
            self.bytes
                .set(self.bytes.get() + (max - node.as_ref().len()) as u64);
        } else {
            self.bytes.set(self.bytes.get() + max as u64);
        }
        Ok(node)
    }
}
const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "raw schedule page read",
    pending: "raw schedule pending",
    tree: "raw schedule tree",
    digest: "raw schedule digest",
    read_allowance: "raw schedule reads",
    admission: "raw schedule admission",
    bound: "raw schedule bound",
    producer_pool: "raw schedule pool",
    page: "raw schedule page",
    write_allowance: "raw schedule writes",
};
pub(super) fn check_changed_scalars<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    index: &ScheduleIndexV1,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRawScheduleWorkV1,
) -> Result<(), WorldFieldOverlayErrorV1> {
    for write in &record.writes {
        check(ctx)?;
        if !ModuleWakeKeyV1::is_wake_key(&write.mutation.key)
            || write.mutation.before == write.mutation.after
        {
            continue;
        }
        spend(work)?;
        let instance = ModuleWakeKeyV1::instance_from_bytes(&write.mutation.key)
            .map_err(ValueTreeError::Invalid)?;
        let Some(id) = write.mutation.after else {
            continue;
        };
        let time = wake(&id, ctx)?;
        let io = RefCell::new(&mut *ctx.values.io);
        let cancel = RefCell::new(&mut ctx.cancelled);
        let error = RefCell::new(None);
        let source = Source {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &io,
            cancelled: &cancel,
            error: &error,
            labels: LABELS,
        };
        work.index.instances_remaining = work.index.instances_remaining.checked_sub(1).ok_or(
            ValueTreeError::ResourceLimited("raw schedule point instances"),
        )?;
        let point = PointReader {
            source: &source,
            nodes: std::cell::Cell::from_mut(&mut work.index.nodes_remaining),
            bytes: std::cell::Cell::from_mut(&mut work.index.read_bytes_remaining),
            error: &error,
        };
        let result = index.contains(time, instance, &point);
        if let Some(e) = error.borrow_mut().take() {
            return Err(e.into());
        }
        if !result? {
            return Err(invalid().into());
        }
    }
    check(ctx)?;
    Ok(())
}
