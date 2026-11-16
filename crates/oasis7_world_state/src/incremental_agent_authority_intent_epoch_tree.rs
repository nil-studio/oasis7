//! Private paid path-copy metadata. Canonical outer binding key bytes are
//! retained independently of embedded intent.agent_id. This is never authority.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cmp::Ordering, sync::Arc};
pub(in crate::incremental_agent_authority_records) type Link = Option<Arc<ReservedValue<Node>>>;
pub(super) type Key = Arc<ReservedValue<KeyData>>;
#[derive(Debug)]
pub(super) struct KeyData {
    bytes: Vec<u8>,
    class: MemoryClass,
}
impl KeyData {
    fn len(&self) -> usize {
        self.bytes.len()
    }
    fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}
#[derive(Debug)]
pub(in crate::incremental_agent_authority_records) struct Node {
    key: Key,
    epoch: Option<u64>,
    left: Link,
    right: Link,
    height: u32,
    max: u64,
    heap: usize,
    recovery_heap: usize,
    class: MemoryClass,
    count: u64,
}
pub(super) struct Meter<'a, C> {
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub cells: &'a mut u64,
    pub bytes: &'a mut u64,
    pub stop: &'a mut C,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("intent epoch resident metadata")
}
impl<C: FnMut() -> bool> Meter<'_, C> {
    fn pay(&mut self, bytes: usize) -> Result<(), ValueTreeError> {
        if (self.stop)() {
            return Err(ValueTreeError::Cancelled);
        }
        *self.cells = self
            .cells
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "intent epoch metadata visits",
            ))?;
        *self.bytes =
            self.bytes
                .checked_sub(bytes as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "intent epoch metadata bytes",
                ))?;
        Ok(())
    }
}
pub(super) fn record_count(n: &Link) -> u64 {
    n.as_ref().map_or(0, |n| n.count)
}
fn height(n: &Link) -> u32 {
    n.as_ref().map_or(0, |n| n.height)
}
fn max(n: &Link) -> u64 {
    n.as_ref().map_or(0, |n| n.max)
}
fn heap(n: &Link) -> usize {
    n.as_ref().map_or(0, |n| n.heap)
}
fn recovery_heap(n: &Link) -> usize {
    n.as_ref().map_or(0, |n| n.recovery_heap)
}
pub(super) fn required_ordinary_cover(tree: &Link) -> usize {
    recovery_heap(tree)
}
fn share(n: &Link) -> Link {
    n.as_ref().map(Arc::clone)
}
pub(super) fn max_epoch(tree: &Link) -> u64 {
    max(tree)
}
pub(super) fn resident_heap(tree: &Link) -> usize {
    heap(tree)
}
pub(in crate::incremental_agent_authority_records) fn share_tree(tree: &Link) -> Link {
    share(tree)
}
fn make<C: FnMut() -> bool>(
    key: &Key,
    epoch: Option<u64>,
    left: Link,
    right: Link,
    m: &mut Meter<'_, C>,
) -> Result<Arc<ReservedValue<Node>>, ValueTreeError> {
    let size = std::mem::size_of::<Node>() + 64;
    m.pay(size)?;
    let fee = m.memory.reserve(size, m.class)?;
    let height = height(&left)
        .max(height(&right))
        .checked_add(1)
        .ok_or_else(bad)?;
    let max = epoch.unwrap_or(0).max(max(&left)).max(max(&right));
    let heap = size
        .checked_add(key.reserved_bytes())
        .and_then(|n| n.checked_add(heap(&left)))
        .and_then(|n| n.checked_add(heap(&right)))
        .ok_or_else(bad)?;
    let count = record_count(&left)
        .checked_add(record_count(&right))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(bad)?;
    let recovery_heap = (if m.class == MemoryClass::Recovery {
        size
    } else {
        0usize
    })
    .checked_add(if key.class == MemoryClass::Recovery {
        key.reserved_bytes()
    } else {
        0
    })
    .and_then(|n| n.checked_add(recovery_heap(&left)))
    .and_then(|n| n.checked_add(recovery_heap(&right)))
    .ok_or_else(bad)?;
    Ok(Arc::new(fee.retain(Node {
        key: Arc::clone(key),
        epoch,
        left,
        right,
        height,
        max,
        heap,
        recovery_heap,
        class: m.class,
        count,
    })))
}
fn cmp<C: FnMut() -> bool>(
    key: &[u8],
    node: &Arc<ReservedValue<Node>>,
    m: &mut Meter<'_, C>,
) -> Result<Ordering, ValueTreeError> {
    m.pay(key.len().min(node.key.len()) + 1)?;
    if !node.uses_budget(m.memory) || !node.key.uses_budget(m.memory) {
        return Err(bad());
    }
    Ok(key.cmp(node.key.as_slice()))
}
pub(super) fn get<C: FnMut() -> bool>(
    tree: &Link,
    key: &[u8],
    m: &mut Meter<'_, C>,
) -> Result<Option<Option<u64>>, ValueTreeError> {
    let mut n = tree.as_ref();
    while let Some(node) = n {
        match cmp(key, node, m)? {
            Ordering::Equal => return Ok(Some(node.epoch)),
            Ordering::Less => n = node.left.as_ref(),
            Ordering::Greater => n = node.right.as_ref(),
        }
    }
    Ok(None)
}
fn balance<C: FnMut() -> bool>(
    key: &Key,
    epoch: Option<u64>,
    left: Link,
    right: Link,
    m: &mut Meter<'_, C>,
) -> Result<Arc<ReservedValue<Node>>, ValueTreeError> {
    if height(&left) > height(&right).saturating_add(1) {
        let l = left.as_ref().ok_or_else(bad)?;
        if height(&l.left) >= height(&l.right) {
            let r = make(key, epoch, share(&l.right), right, m)?;
            return make(&l.key, l.epoch, share(&l.left), Some(r), m);
        }
        let mid = l.right.as_ref().ok_or_else(bad)?;
        let a = make(&l.key, l.epoch, share(&l.left), share(&mid.left), m)?;
        let b = make(key, epoch, share(&mid.right), right, m)?;
        return make(&mid.key, mid.epoch, Some(a), Some(b), m);
    }
    if height(&right) > height(&left).saturating_add(1) {
        let r = right.as_ref().ok_or_else(bad)?;
        if height(&r.right) >= height(&r.left) {
            let l = make(key, epoch, left, share(&r.left), m)?;
            return make(&r.key, r.epoch, Some(l), share(&r.right), m);
        }
        let mid = r.left.as_ref().ok_or_else(bad)?;
        let a = make(key, epoch, left, share(&mid.left), m)?;
        let b = make(&r.key, r.epoch, share(&mid.right), share(&r.right), m)?;
        return make(&mid.key, mid.epoch, Some(a), Some(b), m);
    }
    make(key, epoch, left, right, m)
}
fn put_key<C: FnMut() -> bool>(
    tree: &Link,
    key: &Key,
    epoch: Option<u64>,
    m: &mut Meter<'_, C>,
) -> Result<Link, ValueTreeError> {
    let Some(node) = tree else {
        return make(key, epoch, None, None, m).map(Some);
    };
    match cmp(key.as_slice(), node, m)? {
        Ordering::Equal => {
            make(&node.key, epoch, share(&node.left), share(&node.right), m).map(Some)
        }
        Ordering::Less => {
            let left = put_key(&node.left, key, epoch, m)?;
            balance(&node.key, node.epoch, left, share(&node.right), m).map(Some)
        }
        Ordering::Greater => {
            let right = put_key(&node.right, key, epoch, m)?;
            balance(&node.key, node.epoch, share(&node.left), right, m).map(Some)
        }
    }
}
pub(super) fn put<C: FnMut() -> bool>(
    tree: &Link,
    key: &[u8],
    epoch: Option<u64>,
    m: &mut Meter<'_, C>,
) -> Result<Link, ValueTreeError> {
    m.pay(key.len())?;
    let fee = m.memory.reserve(
        key.len()
            .checked_add(std::mem::size_of::<KeyData>() + 64)
            .ok_or_else(bad)?,
        m.class,
    )?;
    let mut value = Vec::new();
    value
        .try_reserve_exact(key.len())
        .map_err(|_| ValueTreeError::ResourceLimited("intent epoch key allocation"))?;
    if value.capacity() > key.len() {
        return Err(ValueTreeError::ResourceLimited("intent epoch key capacity"));
    }
    value.extend_from_slice(key);
    let key = Arc::new(fee.retain(KeyData {
        bytes: value,
        class: m.class,
    }));
    put_key(tree, &key, epoch, m)
}
fn pop_min<C: FnMut() -> bool>(
    node: &Arc<ReservedValue<Node>>,
    m: &mut Meter<'_, C>,
) -> Result<(Key, Option<u64>, Link), ValueTreeError> {
    m.pay(0)?;
    if let Some(left) = &node.left {
        let (key, epoch, next) = pop_min(left, m)?;
        let rest = balance(&node.key, node.epoch, next, share(&node.right), m)?;
        Ok((key, epoch, Some(rest)))
    } else {
        Ok((Arc::clone(&node.key), node.epoch, share(&node.right)))
    }
}
pub(super) fn remove<C: FnMut() -> bool>(
    tree: &Link,
    key: &[u8],
    m: &mut Meter<'_, C>,
) -> Result<Link, ValueTreeError> {
    let Some(node) = tree else { return Err(bad()) };
    match cmp(key, node, m)? {
        Ordering::Less => {
            let left = remove(&node.left, key, m)?;
            balance(&node.key, node.epoch, left, share(&node.right), m).map(Some)
        }
        Ordering::Greater => {
            let right = remove(&node.right, key, m)?;
            balance(&node.key, node.epoch, share(&node.left), right, m).map(Some)
        }
        Ordering::Equal => match (&node.left, &node.right) {
            (None, _) => Ok(share(&node.right)),
            (_, None) => Ok(share(&node.left)),
            (Some(_), Some(right)) => {
                let (key, epoch, next) = pop_min(right, m)?;
                balance(&key, epoch, share(&node.left), next, m).map(Some)
            }
        },
    }
}
