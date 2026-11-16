use super::*;
pub(super) type NodeRef = Arc<ReservedValue<Node>>;
pub(super) struct Node {
    pub id: Arc<ReservedValue<ReleaseRowV1>>,
    pub left: Option<NodeRef>,
    pub right: Option<NodeRef>,
    height: u32,
    pub bytes: usize,
}
fn height(node: &Option<NodeRef>) -> u32 {
    node.as_ref().map_or(0, |n| n.height)
}
fn node(
    id: Arc<ReservedValue<ReleaseRowV1>>,
    left: Option<NodeRef>,
    right: Option<NodeRef>,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut u64,
    c: &mut impl FnMut() -> bool,
) -> Result<NodeRef, ValueTreeError> {
    step(work, c)?;
    if !id.uses_budget(memory)
        || left.as_ref().is_some_and(|n| !n.uses_budget(memory))
        || right.as_ref().is_some_and(|n| !n.uses_budget(memory))
    {
        return Err(invalid());
    }
    let height = height(&left)
        .max(height(&right))
        .checked_add(1)
        .ok_or_else(invalid)?;
    if height > 128 {
        return Err(invalid());
    }
    let fee = memory.reserve(std::mem::size_of::<Node>() + 128, class)?;
    let bytes = fee
        .bytes()
        .checked_add(id.bytes)
        .and_then(|n| n.checked_add(left.as_ref().map_or(0, |n| n.bytes)))
        .and_then(|n| n.checked_add(right.as_ref().map_or(0, |n| n.bytes)))
        .ok_or_else(invalid)?;
    Ok(Arc::new(fee.retain(Node {
        id,
        left,
        right,
        height,
        bytes,
    })))
}
fn balance(
    id: Arc<ReservedValue<ReleaseRowV1>>,
    left: Option<NodeRef>,
    right: Option<NodeRef>,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut u64,
    c: &mut impl FnMut() -> bool,
) -> Result<NodeRef, ValueTreeError> {
    if height(&left) > height(&right).saturating_add(1) {
        let l = left.as_ref().ok_or_else(invalid)?;
        if height(&l.left) >= height(&l.right) {
            let r = node(id, l.right.clone(), right, memory, class, work, c)?;
            return node(
                l.id.clone(),
                l.left.clone(),
                Some(r),
                memory,
                class,
                work,
                c,
            );
        }
        let pivot = l.right.as_ref().ok_or_else(invalid)?;
        let l2 = node(
            l.id.clone(),
            l.left.clone(),
            pivot.left.clone(),
            memory,
            class,
            work,
            c,
        )?;
        let r2 = node(id, pivot.right.clone(), right, memory, class, work, c)?;
        return node(pivot.id.clone(), Some(l2), Some(r2), memory, class, work, c);
    }
    if height(&right) > height(&left).saturating_add(1) {
        let r = right.as_ref().ok_or_else(invalid)?;
        if height(&r.right) >= height(&r.left) {
            let l = node(id, left, r.left.clone(), memory, class, work, c)?;
            return node(
                r.id.clone(),
                Some(l),
                r.right.clone(),
                memory,
                class,
                work,
                c,
            );
        }
        let pivot = r.left.as_ref().ok_or_else(invalid)?;
        let l2 = node(id, left, pivot.left.clone(), memory, class, work, c)?;
        let r2 = node(
            r.id.clone(),
            pivot.right.clone(),
            r.right.clone(),
            memory,
            class,
            work,
            c,
        )?;
        return node(pivot.id.clone(), Some(l2), Some(r2), memory, class, work, c);
    }
    node(id, left, right, memory, class, work, c)
}
pub(super) fn put(
    root: &Option<NodeRef>,
    id: Arc<ReservedValue<ReleaseRowV1>>,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut u64,
    c: &mut impl FnMut() -> bool,
) -> Result<(NodeRef, bool), ValueTreeError> {
    let Some(n) = root else {
        return Ok((node(id, None, None, memory, class, work, c)?, true));
    };
    match compare(id.key, n.id.key, work, c)? {
        Ordering::Equal => Ok((
            node(id, n.left.clone(), n.right.clone(), memory, class, work, c)?,
            false,
        )),
        Ordering::Less => {
            let (left, new) = put(&n.left, id, memory, class, work, c)?;
            Ok((
                balance(
                    n.id.clone(),
                    Some(left),
                    n.right.clone(),
                    memory,
                    class,
                    work,
                    c,
                )?,
                new,
            ))
        }
        Ordering::Greater => {
            let (right, new) = put(&n.right, id, memory, class, work, c)?;
            Ok((
                balance(
                    n.id.clone(),
                    n.left.clone(),
                    Some(right),
                    memory,
                    class,
                    work,
                    c,
                )?,
                new,
            ))
        }
    }
}
pub(super) fn remove(
    root: &Option<NodeRef>,
    id: u64,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut u64,
    c: &mut impl FnMut() -> bool,
) -> Result<(Option<NodeRef>, bool), ValueTreeError> {
    let Some(n) = root else {
        step(work, c)?;
        return Ok((None, false));
    };
    match compare(id, n.id.key, work, c)? {
        Ordering::Less => {
            let (left, removed) = remove(&n.left, id, memory, class, work, c)?;
            Ok((
                Some(balance(
                    n.id.clone(),
                    left,
                    n.right.clone(),
                    memory,
                    class,
                    work,
                    c,
                )?),
                removed,
            ))
        }
        Ordering::Greater => {
            let (right, removed) = remove(&n.right, id, memory, class, work, c)?;
            Ok((
                Some(balance(
                    n.id.clone(),
                    n.left.clone(),
                    right,
                    memory,
                    class,
                    work,
                    c,
                )?),
                removed,
            ))
        }
        Ordering::Equal => {
            if n.left.is_none() {
                return Ok((n.right.clone(), true));
            }
            if n.right.is_none() {
                return Ok((n.left.clone(), true));
            }
            let mut successor = n.right.as_ref().ok_or_else(invalid)?;
            while let Some(left) = &successor.left {
                step(work, c)?;
                successor = left;
            }
            let id = successor.id.clone();
            let (right, _) = remove(&n.right, id.key, memory, class, work, c)?;
            Ok((
                Some(balance(id, n.left.clone(), right, memory, class, work, c)?),
                true,
            ))
        }
    }
}

pub(super) fn find<'a>(
    root: &'a Option<NodeRef>,
    key: u64,
    work: &mut u64,
    c: &mut impl FnMut() -> bool,
) -> Result<Option<&'a ReleaseRowV1>, ValueTreeError> {
    let mut at = root.as_ref();
    while let Some(n) = at {
        match compare(key, n.id.key, work, c)? {
            Ordering::Equal => return Ok(Some(&n.id)),
            Ordering::Less => at = n.left.as_ref(),
            Ordering::Greater => at = n.right.as_ref(),
        }
    }
    Ok(None)
}
pub(super) fn visit<E: From<ValueTreeError>>(
    root: &Option<NodeRef>,
    work: &mut u64,
    c: &mut impl FnMut() -> bool,
    emit: &mut impl FnMut(&ReleaseRowV1, &mut u64) -> Result<(), E>,
) -> Result<(), E> {
    if let Some(n) = root {
        step(work, c).map_err(E::from)?;
        visit(&n.left, work, c, emit)?;
        emit(&n.id, work)?;
        visit(&n.right, work, c, emit)?;
    }
    Ok(())
}
