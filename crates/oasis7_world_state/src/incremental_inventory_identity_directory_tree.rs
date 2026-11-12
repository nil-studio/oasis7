use super::*;
pub(super) type NodeRef = Arc<ReservedValue<Node>>;
pub(super) struct Node {
    pub id: Arc<ReservedValue<Identity>>,
    pub left: Option<NodeRef>,
    pub right: Option<NodeRef>,
    height: u32,
}
fn height(node: &Option<NodeRef>) -> u32 {
    node.as_ref().map_or(0, |n| n.height)
}
fn node(
    id: Arc<ReservedValue<Identity>>,
    left: Option<NodeRef>,
    right: Option<NodeRef>,
    memory: &MemoryBudget,
    work: &mut InventoryIdentityDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<NodeRef, ValueTreeError> {
    work.step(0, c)?;
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
    let fee = memory.reserve(arc_heap::<Node>()?, MemoryClass::Ordinary)?;
    Ok(Arc::new(fee.retain(Node {
        id,
        left,
        right,
        height,
    })))
}
fn balance(
    id: Arc<ReservedValue<Identity>>,
    left: Option<NodeRef>,
    right: Option<NodeRef>,
    memory: &MemoryBudget,
    work: &mut InventoryIdentityDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<NodeRef, ValueTreeError> {
    if height(&left) > height(&right).saturating_add(1) {
        let l = left.as_ref().ok_or_else(invalid)?;
        if height(&l.left) >= height(&l.right) {
            let r = node(id, l.right.clone(), right, memory, work, c)?;
            return node(l.id.clone(), l.left.clone(), Some(r), memory, work, c);
        }
        let pivot = l.right.as_ref().ok_or_else(invalid)?;
        let l2 = node(
            l.id.clone(),
            l.left.clone(),
            pivot.left.clone(),
            memory,
            work,
            c,
        )?;
        let r2 = node(id, pivot.right.clone(), right, memory, work, c)?;
        return node(pivot.id.clone(), Some(l2), Some(r2), memory, work, c);
    }
    if height(&right) > height(&left).saturating_add(1) {
        let r = right.as_ref().ok_or_else(invalid)?;
        if height(&r.right) >= height(&r.left) {
            let l = node(id, left, r.left.clone(), memory, work, c)?;
            return node(r.id.clone(), Some(l), r.right.clone(), memory, work, c);
        }
        let pivot = r.left.as_ref().ok_or_else(invalid)?;
        let l2 = node(id, left, pivot.left.clone(), memory, work, c)?;
        let r2 = node(
            r.id.clone(),
            pivot.right.clone(),
            r.right.clone(),
            memory,
            work,
            c,
        )?;
        return node(pivot.id.clone(), Some(l2), Some(r2), memory, work, c);
    }
    node(id, left, right, memory, work, c)
}
pub(super) fn put(
    root: &Option<NodeRef>,
    id: Arc<ReservedValue<Identity>>,
    memory: &MemoryBudget,
    work: &mut InventoryIdentityDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<(NodeRef, bool), ValueTreeError> {
    let Some(n) = root else {
        return Ok((node(id, None, None, memory, work, c)?, true));
    };
    match compare(id.borrowed(), n.id.borrowed(), work, c)? {
        Ordering::Equal => Ok((n.clone(), false)),
        Ordering::Less => {
            let (left, new) = put(&n.left, id, memory, work, c)?;
            Ok((
                balance(n.id.clone(), Some(left), n.right.clone(), memory, work, c)?,
                new,
            ))
        }
        Ordering::Greater => {
            let (right, new) = put(&n.right, id, memory, work, c)?;
            Ok((
                balance(n.id.clone(), n.left.clone(), Some(right), memory, work, c)?,
                new,
            ))
        }
    }
}
pub(super) fn remove(
    root: &Option<NodeRef>,
    id: IdentityRef<'_>,
    memory: &MemoryBudget,
    work: &mut InventoryIdentityDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<(Option<NodeRef>, bool), ValueTreeError> {
    let Some(n) = root else {
        work.step(0, c)?;
        return Ok((None, false));
    };
    match compare(id, n.id.borrowed(), work, c)? {
        Ordering::Less => {
            let (left, removed) = remove(&n.left, id, memory, work, c)?;
            Ok((
                Some(balance(
                    n.id.clone(),
                    left,
                    n.right.clone(),
                    memory,
                    work,
                    c,
                )?),
                removed,
            ))
        }
        Ordering::Greater => {
            let (right, removed) = remove(&n.right, id, memory, work, c)?;
            Ok((
                Some(balance(
                    n.id.clone(),
                    n.left.clone(),
                    right,
                    memory,
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
                work.step(0, c)?;
                successor = left;
            }
            let id = successor.id.clone();
            let (right, _) = remove(&n.right, id.borrowed(), memory, work, c)?;
            Ok((
                Some(balance(id, n.left.clone(), right, memory, work, c)?),
                true,
            ))
        }
    }
}
pub(super) fn ledger(
    root: &Option<NodeRef>,
    ledger: Option<LedgerKeyRefV1<'_>>,
    work: &mut InventoryIdentityDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
    emit: &mut impl FnMut(
        &Identity,
        &mut InventoryIdentityDirectoryWorkV1,
        &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    let Some(n) = root else {
        work.step(0, c)?;
        return Ok(());
    };
    let comparison = compare(
        IdentityRef {
            ledger,
            material: None,
        },
        IdentityRef {
            ledger: n.id.borrowed().ledger,
            material: None,
        },
        work,
        c,
    )?;
    if comparison != Ordering::Greater {
        self::ledger(&n.left, ledger, work, c, emit)?;
    }
    if comparison == Ordering::Equal {
        emit(&n.id, work, c)?;
    }
    if comparison != Ordering::Less {
        self::ledger(&n.right, ledger, work, c, emit)?;
    }
    Ok(())
}
