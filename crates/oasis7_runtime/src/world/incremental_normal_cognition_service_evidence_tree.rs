//! Private persistent AVL for authenticated evidence metadata. Keys and values
//! carry no authority; only the enclosing actual-source producer can update it.
use super::*;
use std::{cmp::Ordering, sync::Arc};
pub(in crate::world) type Link = Option<Arc<ReservedValue<Node>>>;
pub(in crate::world) type Key = Arc<ReservedValue<Vec<u8>>>;
#[derive(Clone)]
pub(in crate::world) enum Data {
    Count(u64),
    Pair(Key),
}
pub(in crate::world) struct Node {
    key: Key,
    data: Data,
    left: Link,
    right: Link,
    height: u32,
}
fn height(node: &Link) -> u32 {
    node.as_ref().map_or(0, |node| node.height)
}
fn copy_link(node: &Link) -> Link {
    node.as_ref().map(Arc::clone)
}
fn make(
    key: &Key,
    data: &Data,
    left: Link,
    right: Link,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Arc<ReservedValue<Node>>, BodyMutationError> {
    common
        .charge(1, std::mem::size_of::<Node>() as u64)
        .map_err(formal)?;
    let fee = common
        .memory()
        .reserve(std::mem::size_of::<Node>() + 64, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let height = height(&left)
        .max(height(&right))
        .checked_add(1)
        .ok_or_else(bad)?;
    Ok(Arc::new(fee.retain(Node {
        key: Arc::clone(key),
        data: data.clone(),
        left,
        right,
        height,
    })))
}
fn compare(
    key: &[u8],
    node: &Arc<ReservedValue<Node>>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Ordering, BodyMutationError> {
    common
        .charge(1, key.len().min(node.key.len()) as u64 + 1)
        .map_err(formal)?;
    if !node.uses_budget(common.memory()) || !node.key.uses_budget(common.memory()) {
        return Err(bad().into());
    }
    Ok(key.cmp(node.key.as_slice()))
}
pub(in crate::world) fn key(
    bytes: &[&[u8]],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Key, BodyMutationError> {
    let len = bytes
        .iter()
        .try_fold(0usize, |n, b| n.checked_add(b.len()).ok_or_else(bad))?;
    common
        .charge(bytes.len() as u64, len as u64)
        .map_err(formal)?;
    let fee = common
        .memory()
        .reserve(
            len.checked_add(std::mem::size_of::<Vec<u8>>() + 64)
                .ok_or_else(bad)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut value = Vec::new();
    value
        .try_reserve_exact(len)
        .map_err(|_| ValueTreeError::ResourceLimited("evidence key allocation"))?;
    if value.capacity() > len {
        return Err(ValueTreeError::ResourceLimited("evidence key capacity").into());
    }
    for part in bytes {
        value.extend_from_slice(part)
    }
    Ok(Arc::new(fee.retain(value)))
}
pub(in crate::world) fn get(
    tree: &Link,
    key: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<Data>, BodyMutationError> {
    let mut node = tree.as_ref();
    while let Some(current) = node {
        match compare(key, current, common)? {
            Ordering::Equal => return Ok(Some(current.data.clone())),
            Ordering::Less => node = current.left.as_ref(),
            Ordering::Greater => node = current.right.as_ref(),
        }
    }
    Ok(None)
}
fn balance(
    key: &Key,
    data: &Data,
    left: Link,
    right: Link,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Arc<ReservedValue<Node>>, BodyMutationError> {
    let lh = height(&left);
    let rh = height(&right);
    if lh > rh.saturating_add(1) {
        let child = left.as_ref().ok_or_else(bad)?;
        if height(&child.left) >= height(&child.right) {
            let new_right = make(key, data, copy_link(&child.right), right, common)?;
            return make(
                &child.key,
                &child.data,
                copy_link(&child.left),
                Some(new_right),
                common,
            );
        }
        let middle = child.right.as_ref().ok_or_else(bad)?;
        let a = make(
            &child.key,
            &child.data,
            copy_link(&child.left),
            copy_link(&middle.left),
            common,
        )?;
        let b = make(key, data, copy_link(&middle.right), right, common)?;
        return make(&middle.key, &middle.data, Some(a), Some(b), common);
    }
    if rh > lh.saturating_add(1) {
        let child = right.as_ref().ok_or_else(bad)?;
        if height(&child.right) >= height(&child.left) {
            let new_left = make(key, data, left, copy_link(&child.left), common)?;
            return make(
                &child.key,
                &child.data,
                Some(new_left),
                copy_link(&child.right),
                common,
            );
        }
        let middle = child.left.as_ref().ok_or_else(bad)?;
        let a = make(key, data, left, copy_link(&middle.left), common)?;
        let b = make(
            &child.key,
            &child.data,
            copy_link(&middle.right),
            copy_link(&child.right),
            common,
        )?;
        return make(&middle.key, &middle.data, Some(a), Some(b), common);
    }
    make(key, data, left, right, common)
}
pub(in crate::world) fn put(
    tree: &Link,
    key: &Key,
    data: Data,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Link, BodyMutationError> {
    let Some(node) = tree else {
        return make(key, &data, None, None, common).map(Some);
    };
    match compare(key, node, common)? {
        Ordering::Equal => make(
            &node.key,
            &data,
            copy_link(&node.left),
            copy_link(&node.right),
            common,
        )
        .map(Some),
        Ordering::Less => {
            let left = put(&node.left, key, data, common)?;
            balance(&node.key, &node.data, left, copy_link(&node.right), common).map(Some)
        }
        Ordering::Greater => {
            let right = put(&node.right, key, data, common)?;
            balance(&node.key, &node.data, copy_link(&node.left), right, common).map(Some)
        }
    }
}
fn take_min(
    tree: &Arc<ReservedValue<Node>>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(Key, Data, Link), BodyMutationError> {
    common.charge(1, 0).map_err(formal)?;
    if let Some(left) = &tree.left {
        let (key, data, next) = take_min(left, common)?;
        let rest = balance(&tree.key, &tree.data, next, copy_link(&tree.right), common)?;
        Ok((key, data, Some(rest)))
    } else {
        Ok((
            Arc::clone(&tree.key),
            tree.data.clone(),
            copy_link(&tree.right),
        ))
    }
}
pub(in crate::world) fn remove(
    tree: &Link,
    key: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Link, BodyMutationError> {
    let Some(node) = tree else {
        return Err(bad().into());
    };
    match compare(key, node, common)? {
        Ordering::Less => {
            let left = remove(&node.left, key, common)?;
            balance(&node.key, &node.data, left, copy_link(&node.right), common).map(Some)
        }
        Ordering::Greater => {
            let right = remove(&node.right, key, common)?;
            balance(&node.key, &node.data, copy_link(&node.left), right, common).map(Some)
        }
        Ordering::Equal => match (&node.left, &node.right) {
            (None, _) => Ok(copy_link(&node.right)),
            (_, None) => Ok(copy_link(&node.left)),
            (Some(_), Some(right)) => {
                let (key, data, next) = take_min(right, common)?;
                balance(&key, &data, copy_link(&node.left), next, common).map(Some)
            }
        },
    }
}

/// First ordered key at or above the supplied cut; no history scan or key copy.
pub(in crate::world) fn lower_bound(
    tree: &Link,
    key: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<(Key, Data)>, BodyMutationError> {
    let mut node = tree.as_ref();
    let mut selected = None;
    while let Some(current) = node {
        match compare(key, current, common)? {
            Ordering::Equal => return Ok(Some((Arc::clone(&current.key), current.data.clone()))),
            Ordering::Less => {
                selected = Some(current);
                node = current.left.as_ref();
            }
            Ordering::Greater => node = current.right.as_ref(),
        }
    }
    Ok(selected.map(|n| (Arc::clone(&n.key), n.data.clone())))
}
