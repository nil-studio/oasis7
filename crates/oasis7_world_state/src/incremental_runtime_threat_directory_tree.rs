use super::*;
pub(super) type NodeRef = Arc<ReservedValue<Node>>;
pub(super) struct Node {
    pub id: Arc<ReservedValue<ThreatKeyRowV1>>,
    pub left: Option<NodeRef>,
    pub right: Option<NodeRef>,
    height: u32,
}
fn height(node: &Option<NodeRef>) -> u32 {
    node.as_ref().map_or(0, |n| n.height)
}
fn node(
    id: Arc<ReservedValue<ThreatKeyRowV1>>,
    left: Option<NodeRef>,
    right: Option<NodeRef>,
    memory: &MemoryBudget,
    work: &mut ThreatKeyDirectoryWorkV1,
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
    id: Arc<ReservedValue<ThreatKeyRowV1>>,
    left: Option<NodeRef>,
    right: Option<NodeRef>,
    memory: &MemoryBudget,
    work: &mut ThreatKeyDirectoryWorkV1,
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
    id: Arc<ReservedValue<ThreatKeyRowV1>>,
    memory: &MemoryBudget,
    work: &mut ThreatKeyDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<(NodeRef, bool), ValueTreeError> {
    let Some(n) = root else {
        return Ok((node(id, None, None, memory, work, c)?, true));
    };
    match compare(&id.key, &n.id.key, work, c)? {
        Ordering::Equal => Ok((node(id,n.left.clone(),n.right.clone(),memory,work,c)?,false)),
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
    id: &[u8],
    memory: &MemoryBudget,
    work: &mut ThreatKeyDirectoryWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<(Option<NodeRef>, bool), ValueTreeError> {
    let Some(n) = root else {
        work.step(0, c)?;
        return Ok((None, false));
    };
    match compare(id, &n.id.key, work, c)? {
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
            let (right, _) = remove(&n.right, &id.key, memory, work, c)?;
            Ok((
                Some(balance(id, n.left.clone(), right, memory, work, c)?),
                true,
            ))
        }
    }
}

pub(super) fn find<'a>(root:&'a Option<NodeRef>,key:&[u8],work:&mut ThreatKeyDirectoryWorkV1,c:&mut impl FnMut()->bool)->Result<Option<&'a ThreatKeyRowV1>,ValueTreeError>{
 let mut at=root.as_ref();while let Some(n)=at {match compare(key,&n.id.key,work,c)? {Ordering::Equal=>return Ok(Some(&n.id)),Ordering::Less=>at=n.left.as_ref(),Ordering::Greater=>at=n.right.as_ref()}}Ok(None)
}
pub(super) fn visit<E:From<ValueTreeError>>(root:&Option<NodeRef>,work:&mut ThreatKeyDirectoryWorkV1,c:&mut impl FnMut()->bool,emit:&mut impl FnMut(&ThreatKeyRowV1,&mut ThreatKeyDirectoryWorkV1)->Result<(),E>)->Result<(),E>{
 if let Some(n)=root {work.step(0,c).map_err(E::from)?;visit(&n.left,work,c,emit)?;emit(&n.id,work)?;visit(&n.right,work,c,emit)?;}Ok(())
}
