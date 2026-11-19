use super::*;
use std::cmp::Ordering;
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) struct Key {
    pub bytes: [u8; 64],
    pub len: u8,
    pub position: u64,
}
impl Key {
    pub fn new(bytes: &[u8], position: u64) -> Self {
        let mut key = Self {
            bytes: [0; 64],
            len: bytes.len().min(64) as u8,
            position,
        };
        key.bytes[..key.len as usize].copy_from_slice(&bytes[..key.len as usize]);
        key
    }
    pub fn data(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}
fn cmp(
    a: &Key,
    b: &Key,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Ordering, DeliveryDataErrorV1> {
    common.charge(1, a.len.min(b.len) as u64)?;
    Ok(a.data()
        .cmp(b.data())
        .then_with(|| a.position.cmp(&b.position)))
}
pub(super) type NodeRef = Arc<ReservedValue<Node>>;
pub(super) struct Node {
    pub key: Key,
    pub event: EventRef,
    pub left: Option<NodeRef>,
    pub right: Option<NodeRef>,
    height: u32,
}
fn height(n: &Option<NodeRef>) -> u32 {
    n.as_ref().map_or(0, |n| n.height)
}
fn node(
    key: Key,
    event: EventRef,
    left: Option<NodeRef>,
    right: Option<NodeRef>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<NodeRef, DeliveryDataErrorV1> {
    common.consume_steps(1)?;
    if !event.uses_budget(common.memory())
        || left
            .as_ref()
            .is_some_and(|n| !n.uses_budget(common.memory()))
        || right
            .as_ref()
            .is_some_and(|n| !n.uses_budget(common.memory()))
    {
        return Err(invalid("delivery AVL pool"));
    }
    let height = height(&left)
        .max(height(&right))
        .checked_add(1)
        .filter(|n| *n <= 128)
        .ok_or_else(limited)?;
    let fee = common
        .memory()
        .reserve(arc_heap::<Node>()?, MemoryClass::Ordinary)?;
    Ok(Arc::new(fee.retain(Node {
        key,
        event,
        left,
        right,
        height,
    })))
}
fn balance(
    key: Key,
    event: EventRef,
    left: Option<NodeRef>,
    right: Option<NodeRef>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<NodeRef, DeliveryDataErrorV1> {
    if height(&left) > height(&right).saturating_add(1) {
        let l = left.as_ref().ok_or_else(limited)?;
        if height(&l.left) >= height(&l.right) {
            let r = node(key, event, l.right.clone(), right, common)?;
            return node(l.key, l.event.clone(), l.left.clone(), Some(r), common);
        }
        let p = l.right.as_ref().ok_or_else(limited)?;
        let l2 = node(
            l.key,
            l.event.clone(),
            l.left.clone(),
            p.left.clone(),
            common,
        )?;
        let r2 = node(key, event, p.right.clone(), right, common)?;
        return node(p.key, p.event.clone(), Some(l2), Some(r2), common);
    }
    if height(&right) > height(&left).saturating_add(1) {
        let r = right.as_ref().ok_or_else(limited)?;
        if height(&r.right) >= height(&r.left) {
            let l = node(key, event, left, r.left.clone(), common)?;
            return node(r.key, r.event.clone(), Some(l), r.right.clone(), common);
        }
        let p = r.left.as_ref().ok_or_else(limited)?;
        let l2 = node(key, event, left, p.left.clone(), common)?;
        let r2 = node(
            r.key,
            r.event.clone(),
            p.right.clone(),
            r.right.clone(),
            common,
        )?;
        return node(p.key, p.event.clone(), Some(l2), Some(r2), common);
    }
    node(key, event, left, right, common)
}
pub(super) fn put(
    root: &Option<NodeRef>,
    key: Key,
    event: EventRef,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<NodeRef, DeliveryDataErrorV1> {
    let Some(n) = root else {
        return node(key, event, None, None, common);
    };
    match cmp(&key, &n.key, common)? {
        Ordering::Equal => Err(invalid("delivery duplicate source position")),
        Ordering::Less => {
            let l = put(&n.left, key, event, common)?;
            balance(n.key, n.event.clone(), Some(l), n.right.clone(), common)
        }
        Ordering::Greater => {
            let r = put(&n.right, key, event, common)?;
            balance(n.key, n.event.clone(), n.left.clone(), Some(r), common)
        }
    }
}
pub(super) fn remove(
    root: &Option<NodeRef>,
    key: &Key,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<NodeRef>, DeliveryDataErrorV1> {
    let Some(n) = root else {
        return Err(invalid("delivery missing evicted source"));
    };
    match cmp(key, &n.key, common)? {
        Ordering::Less => {
            let l = remove(&n.left, key, common)?;
            Ok(Some(balance(
                n.key,
                n.event.clone(),
                l,
                n.right.clone(),
                common,
            )?))
        }
        Ordering::Greater => {
            let r = remove(&n.right, key, common)?;
            Ok(Some(balance(
                n.key,
                n.event.clone(),
                n.left.clone(),
                r,
                common,
            )?))
        }
        Ordering::Equal => {
            if n.left.is_none() {
                return Ok(n.right.clone());
            }
            if n.right.is_none() {
                return Ok(n.left.clone());
            }
            let mut p = n.right.as_ref().ok_or_else(limited)?;
            while let Some(l) = &p.left {
                common.consume_steps(1)?;
                p = l;
            }
            let key = p.key;
            let event = p.event.clone();
            let r = remove(&n.right, &key, common)?;
            Ok(Some(balance(key, event, n.left.clone(), r, common)?))
        }
    }
}
pub(super) fn lower(
    root: &Option<NodeRef>,
    key: &Key,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<EventRef>, DeliveryDataErrorV1> {
    let mut root = root;
    let mut best = None;
    while let Some(n) = root {
        match cmp(&n.key, key, common)? {
            Ordering::Less => root = &n.right,
            _ => {
                best = Some(n.event.clone());
                root = &n.left;
            }
        }
    }
    Ok(best)
}
pub(super) fn range(
    root: &Option<NodeRef>,
    prefix: &[u8],
    from: u64,
    to: u64,
    reverse: bool,
    common: &mut FormalBoundedContextV1<'_>,
    emit: &mut impl FnMut(
        &EventRef,
        &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, DeliveryDataErrorV1>,
) -> Result<bool, DeliveryDataErrorV1> {
    let Some(n) = root else {
        common.consume_steps(1)?;
        return Ok(true);
    };
    common.charge(1, (prefix.len().min(n.key.len as usize)) as u64)?;
    let order = n.key.data().cmp(prefix);
    if order == Ordering::Less {
        return range(&n.right, prefix, from, to, reverse, common, emit);
    }
    if order == Ordering::Greater {
        return range(&n.left, prefix, from, to, reverse, common, emit);
    }
    if n.key.position < from {
        return range(&n.right, prefix, from, to, reverse, common, emit);
    }
    if n.key.position >= to {
        return range(&n.left, prefix, from, to, reverse, common, emit);
    }
    let first = if reverse { &n.right } else { &n.left };
    let last = if reverse { &n.left } else { &n.right };
    if !range(first, prefix, from, to, reverse, common, emit)? {
        return Ok(false);
    }
    if n.key.position >= from && n.key.position < to && !emit(&n.event, common)? {
        return Ok(false);
    }
    range(last, prefix, from, to, reverse, common, emit)
}
