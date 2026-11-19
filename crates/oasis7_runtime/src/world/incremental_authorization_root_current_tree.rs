//! Paid persistent canonical projection. This is DATA, never a source permit.
use super::*;
use std::mem::{align_of, size_of};
use std::sync::Arc;

// Class travels only with allocations made by this module's private factory.
// ReservedValue deliberately carries no allocation-class API.
pub(super) struct Classed<T> {
    _paid: ReservedValue<T>,
    class: MemoryClass,
}
impl<T> std::ops::Deref for Classed<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self._paid
    }
}
impl<T> Classed<T> {
    pub(super) fn uses_budget(&self, memory: &oasis7_proto::memory_budget::MemoryBudget) -> bool {
        self._paid.uses_budget(memory)
    }
    pub(super) fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
pub(super) type ValueRef = Arc<Classed<CanonicalValueV1>>;
pub(super) type TreeRef = Arc<Classed<CanonicalEntryV1>>;
pub(super) type BytesRef = Arc<Classed<Vec<u8>>>;
pub(super) enum CanonicalValueV1 {
    Atom(BytesRef),
    Array { len: u64, entries: Option<TreeRef> },
    Map { len: u64, entries: Option<TreeRef> },
    Tag { tag: u64, value: ValueRef },
}
#[derive(Clone)]
pub(super) enum CanonicalKeyV1 {
    Index(u64),
    Text(BytesRef),
}
pub(super) struct CanonicalEntryV1 {
    pub key: CanonicalKeyV1,
    pub value: ValueRef,
    pub left: Option<TreeRef>,
    pub right: Option<TreeRef>,
    pub height: u32,
}
fn arc_layout<T>() -> Result<usize, AuthorizationRootCacheErrorV1> {
    // ArcInner's two AtomicUsize counters, its data and alignment padding.
    // Each reservation travels inside that very Arc's ReservedValue.
    size_of::<Classed<T>>()
        .checked_add(2 * size_of::<usize>())
        .and_then(|n| n.checked_add(4 * align_of::<Classed<T>>()))
        .ok_or_else(limited)
}
pub(super) fn value(
    value: CanonicalValueV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<ValueRef, AuthorizationRootCacheErrorV1> {
    common.consume_steps(1)?;
    let valid = match &value {
        CanonicalValueV1::Atom(v) => {
            v.uses_budget(common.memory()) && v.allocation_class() == class
        }
        CanonicalValueV1::Array { entries, .. } | CanonicalValueV1::Map { entries, .. } => entries
            .as_ref()
            .is_none_or(|v| v.uses_budget(common.memory()) && v.allocation_class() == class),
        CanonicalValueV1::Tag { value, .. } => {
            value.uses_budget(common.memory()) && value.allocation_class() == class
        }
    };
    if !valid {
        return Err(invalid("canonical value child pool/class"));
    }
    let fee = common
        .memory()
        .reserve(arc_layout::<CanonicalValueV1>()?, class)?;
    Ok(Arc::new(Classed {
        _paid: fee.retain(value),
        class,
    }))
}
pub(super) fn bytes(
    raw: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<BytesRef, AuthorizationRootCacheErrorV1> {
    common.charge(1, raw.len() as u64)?;
    let fee = common.memory().reserve(
        arc_layout::<Vec<u8>>()?
            .checked_add(raw.len())
            .ok_or_else(limited)?,
        class,
    )?;
    let mut out = Vec::new();
    out.try_reserve_exact(raw.len()).map_err(|_| limited())?;
    if out.capacity() > raw.len() {
        return Err(limited());
    }
    out.extend_from_slice(raw);
    Ok(Arc::new(Classed {
        _paid: fee.retain(out),
        class,
    }))
}
fn height(t: &Option<TreeRef>) -> u32 {
    t.as_ref().map_or(0, |t| t.height)
}
fn node(
    key: CanonicalKeyV1,
    value: ValueRef,
    left: Option<TreeRef>,
    right: Option<TreeRef>,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<TreeRef, AuthorizationRootCacheErrorV1> {
    common.consume_steps(1)?;
    if !value.uses_budget(common.memory())
        || value.allocation_class() != class
        || left
            .as_ref()
            .is_some_and(|v| !v.uses_budget(common.memory()) || v.allocation_class() != class)
        || right
            .as_ref()
            .is_some_and(|v| !v.uses_budget(common.memory()) || v.allocation_class() != class)
        || matches!(&key,CanonicalKeyV1::Text(v) if !v.uses_budget(common.memory())||v.allocation_class()!=class)
    {
        return Err(invalid("canonical entry pool/class"));
    }
    let height = height(&left)
        .max(height(&right))
        .checked_add(1)
        .ok_or_else(limited)?;
    if height > 128 {
        return Err(limited());
    }
    let fee = common
        .memory()
        .reserve(arc_layout::<CanonicalEntryV1>()?, class)?;
    Ok(Arc::new(Classed {
        _paid: fee.retain(CanonicalEntryV1 {
            key,
            value,
            left,
            right,
            height,
        }),
        class,
    }))
}
pub(super) fn compare(
    a: &CanonicalKeyV1,
    b: &CanonicalKeyV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<std::cmp::Ordering, AuthorizationRootCacheErrorV1> {
    common.consume_steps(1)?;
    Ok(match (a, b) {
        (CanonicalKeyV1::Index(a), CanonicalKeyV1::Index(b)) => a.cmp(b),
        (CanonicalKeyV1::Text(a), CanonicalKeyV1::Text(b)) => {
            common.consume_read_bytes(a.len().min(b.len()) as u64)?;
            a.len()
                .cmp(&b.len())
                .then_with(|| a.as_slice().cmp(b.as_slice()))
        }
        _ => return Err(invalid("canonical projection key domain")),
    })
}
fn balance(
    key: CanonicalKeyV1,
    value: ValueRef,
    left: Option<TreeRef>,
    right: Option<TreeRef>,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<TreeRef, AuthorizationRootCacheErrorV1> {
    let lh = height(&left);
    let rh = height(&right);
    if lh > rh.saturating_add(1) {
        let l = left.as_ref().ok_or_else(|| invalid("canonical AVL left"))?;
        if height(&l.left) >= height(&l.right) {
            let next = node(key, value, l.right.clone(), right, common, class)?;
            return node(
                l.key.clone(),
                l.value.clone(),
                l.left.clone(),
                Some(next),
                common,
                class,
            );
        }
        let middle = l
            .right
            .as_ref()
            .ok_or_else(|| invalid("canonical AVL left-right"))?;
        let a = node(
            l.key.clone(),
            l.value.clone(),
            l.left.clone(),
            middle.left.clone(),
            common,
            class,
        )?;
        let b = node(key, value, middle.right.clone(), right, common, class)?;
        return node(
            middle.key.clone(),
            middle.value.clone(),
            Some(a),
            Some(b),
            common,
            class,
        );
    }
    if rh > lh.saturating_add(1) {
        let r = right
            .as_ref()
            .ok_or_else(|| invalid("canonical AVL right"))?;
        if height(&r.right) >= height(&r.left) {
            let next = node(key, value, left, r.left.clone(), common, class)?;
            return node(
                r.key.clone(),
                r.value.clone(),
                Some(next),
                r.right.clone(),
                common,
                class,
            );
        }
        let middle = r
            .left
            .as_ref()
            .ok_or_else(|| invalid("canonical AVL right-left"))?;
        let a = node(key, value, left, middle.left.clone(), common, class)?;
        let b = node(
            r.key.clone(),
            r.value.clone(),
            middle.right.clone(),
            r.right.clone(),
            common,
            class,
        )?;
        return node(
            middle.key.clone(),
            middle.value.clone(),
            Some(a),
            Some(b),
            common,
            class,
        );
    }
    node(key, value, left, right, common, class)
}
pub(super) fn find(
    tree: &Option<TreeRef>,
    key: &CanonicalKeyV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<ValueRef>, AuthorizationRootCacheErrorV1> {
    let mut tree = tree.as_ref();
    while let Some(n) = tree {
        match compare(key, &n.key, common)? {
            std::cmp::Ordering::Equal => return Ok(Some(n.value.clone())),
            std::cmp::Ordering::Less => tree = n.left.as_ref(),
            std::cmp::Ordering::Greater => tree = n.right.as_ref(),
        }
    }
    Ok(None)
}
/// Returns whether an entry was added. Replacing a record copies only its AVL
/// search/rotation path; every unchanged charged key/value remains shared.
pub(super) fn put(
    tree: &Option<TreeRef>,
    key: CanonicalKeyV1,
    value: ValueRef,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<(TreeRef, bool), AuthorizationRootCacheErrorV1> {
    let Some(n) = tree else {
        return Ok((node(key, value, None, None, common, class)?, true));
    };
    match compare(&key, &n.key, common)? {
        std::cmp::Ordering::Equal => Ok((
            node(
                n.key.clone(),
                value,
                n.left.clone(),
                n.right.clone(),
                common,
                class,
            )?,
            false,
        )),
        std::cmp::Ordering::Less => {
            let (left, added) = put(&n.left, key, value, common, class)?;
            Ok((
                balance(
                    n.key.clone(),
                    n.value.clone(),
                    Some(left),
                    n.right.clone(),
                    common,
                    class,
                )?,
                added,
            ))
        }
        std::cmp::Ordering::Greater => {
            let (right, added) = put(&n.right, key, value, common, class)?;
            Ok((
                balance(
                    n.key.clone(),
                    n.value.clone(),
                    n.left.clone(),
                    Some(right),
                    common,
                    class,
                )?,
                added,
            ))
        }
    }
}
fn first(
    tree: &TreeRef,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<TreeRef, AuthorizationRootCacheErrorV1> {
    let mut current = tree.clone();
    loop {
        common.consume_steps(1)?;
        let Some(left) = current.left.clone() else {
            return Ok(current);
        };
        current = left;
    }
}
pub(super) fn remove(
    tree: &Option<TreeRef>,
    key: &CanonicalKeyV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<(Option<TreeRef>, bool), AuthorizationRootCacheErrorV1> {
    let Some(n) = tree else {
        return Ok((None, false));
    };
    match compare(key, &n.key, common)? {
        std::cmp::Ordering::Less => {
            let (left, removed) = remove(&n.left, key, common, class)?;
            Ok((
                Some(balance(
                    n.key.clone(),
                    n.value.clone(),
                    left,
                    n.right.clone(),
                    common,
                    class,
                )?),
                removed,
            ))
        }
        std::cmp::Ordering::Greater => {
            let (right, removed) = remove(&n.right, key, common, class)?;
            Ok((
                Some(balance(
                    n.key.clone(),
                    n.value.clone(),
                    n.left.clone(),
                    right,
                    common,
                    class,
                )?),
                removed,
            ))
        }
        std::cmp::Ordering::Equal => match (&n.left, &n.right) {
            (None, _) => Ok((n.right.clone(), true)),
            (_, None) => Ok((n.left.clone(), true)),
            (Some(_), Some(r)) => {
                let next = first(r, common)?;
                let (right, removed) = remove(&n.right, &next.key, common, class)?;
                if !removed {
                    return Err(invalid("canonical AVL successor"));
                }
                Ok((
                    Some(balance(
                        next.key.clone(),
                        next.value.clone(),
                        n.left.clone(),
                        right,
                        common,
                        class,
                    )?),
                    true,
                ))
            }
        },
    }
}
pub(super) fn visit<E>(
    tree: &Option<TreeRef>,
    common: &mut FormalBoundedContextV1<'_>,
    emit: &mut impl FnMut(&CanonicalKeyV1, &ValueRef, &mut FormalBoundedContextV1<'_>) -> Result<(), E>,
) -> Result<(), E>
where
    E: From<AuthorizationRootCacheErrorV1>,
{
    let Some(node) = tree else {
        return Ok(());
    };
    common
        .consume_steps(1)
        .map_err(AuthorizationRootCacheErrorV1::from)?;
    visit(&node.left, common, emit)?;
    emit(&node.key, &node.value, common)?;
    visit(&node.right, common, emit)
}
