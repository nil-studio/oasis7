//! Persistent complete registry-record physical keys. Only the original
//! qualification and verified-transition kernels populate this DATA directory.
use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cmp::Ordering, sync::Arc};
type Link = Option<Arc<ReservedValue<Node>>>;
struct Node {
    key: Arc<ReservedValue<Vec<u8>>>,
    priority: [u8; 32],
    left: Link,
    right: Link,
    height: u16,
}
/// Recovery-backed nodes retain their Ordinary covering reservation in this
/// shared backing; every path-copy successor inherits it before parent Drop.
pub(in crate::incremental_runtime_artifact_config) struct RegistryRecordKeysV1 {
    root: Link,
    len: u64,
    recovery_bytes: usize,
    coverage: Option<Arc<MemoryReservation>>,
    fee: MemoryReservation,
}
impl std::fmt::Debug for RegistryRecordKeysV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryRecordKeysV1")
            .field("len", &self.len)
            .finish()
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("artifact registry key directory")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("artifact registry key directory")
}
fn charge(
    work: &mut ArtifactConfigReadWorkV1,
    keys: &mut SemanticKeyBudget,
    bytes: usize,
    c: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if c() {
        return Err(ValueTreeError::Cancelled);
    }
    work.cell()?;
    keys.bytes_remaining = keys
        .bytes_remaining
        .checked_sub(u64::try_from(bytes).map_err(|_| limited())?)
        .ok_or_else(limited)?;
    Ok(())
}
fn compare(
    a: &[u8],
    b: &[u8],
    work: &mut ArtifactConfigReadWorkV1,
    keys: &mut SemanticKeyBudget,
    c: &mut impl FnMut() -> bool,
) -> Result<Ordering, ValueTreeError> {
    charge(
        work,
        keys,
        a.len().min(b.len()).checked_add(1).ok_or_else(limited)?,
        c,
    )?;
    Ok(a.cmp(b))
}
fn arc_bytes<T>() -> Result<usize, ValueTreeError> {
    std::mem::size_of::<ReservedValue<T>>()
        .checked_add(
            2 * std::mem::size_of::<usize>() + 4 * std::mem::align_of::<ReservedValue<T>>(),
        )
        .ok_or_else(limited)
}
fn make(
    key: Arc<ReservedValue<Vec<u8>>>,
    priority: [u8; 32],
    left: Link,
    right: Link,
    memory: &MemoryBudget,
    class: MemoryClass,
    allocated: &mut usize,
    work: &mut ArtifactConfigReadWorkV1,
    keys: &mut SemanticKeyBudget,
    c: &mut impl FnMut() -> bool,
) -> Result<Arc<ReservedValue<Node>>, ValueTreeError> {
    charge(work, keys, 0, c)?;
    if !key.uses_budget(memory)
        || left.as_ref().is_some_and(|n| !n.uses_budget(memory))
        || right.as_ref().is_some_and(|n| !n.uses_budget(memory))
    {
        return Err(invalid());
    }
    let height = left
        .as_ref()
        .map_or(0, |n| n.height)
        .max(right.as_ref().map_or(0, |n| n.height))
        .checked_add(1)
        .ok_or_else(limited)?;
    if height > 128 {
        return Err(limited());
    }
    let n = arc_bytes::<Node>()?;
    let fee = memory.reserve(n, class)?;
    if class == MemoryClass::Recovery {
        *allocated = allocated.checked_add(n).ok_or_else(limited)?;
    }
    Ok(Arc::new(fee.retain(Node {
        key,
        priority,
        left,
        right,
        height,
    })))
}
fn earlier(
    a: &Node,
    b: &Node,
    work: &mut ArtifactConfigReadWorkV1,
    keys: &mut SemanticKeyBudget,
    c: &mut impl FnMut() -> bool,
) -> Result<bool, ValueTreeError> {
    charge(work, keys, 32, c)?;
    Ok(a.priority < b.priority
        || (a.priority == b.priority && compare(&a.key, &b.key, work, keys, c)? == Ordering::Less))
}
fn put(
    root: &Link,
    key: Arc<ReservedValue<Vec<u8>>>,
    priority: [u8; 32],
    memory: &MemoryBudget,
    class: MemoryClass,
    allocated: &mut usize,
    work: &mut ArtifactConfigReadWorkV1,
    keys: &mut SemanticKeyBudget,
    c: &mut impl FnMut() -> bool,
) -> Result<Arc<ReservedValue<Node>>, ValueTreeError> {
    let Some(n) = root else {
        return make(
            key, priority, None, None, memory, class, allocated, work, keys, c,
        );
    };
    match compare(&key, &n.key, work, keys, c)? {
        Ordering::Equal => Err(invalid()),
        Ordering::Less => {
            let l = put(
                &n.left, key, priority, memory, class, allocated, work, keys, c,
            )?;
            if earlier(&l, n, work, keys, c)? {
                let r = make(
                    n.key.clone(),
                    n.priority,
                    l.right.clone(),
                    n.right.clone(),
                    memory,
                    class,
                    allocated,
                    work,
                    keys,
                    c,
                )?;
                make(
                    l.key.clone(),
                    l.priority,
                    l.left.clone(),
                    Some(r),
                    memory,
                    class,
                    allocated,
                    work,
                    keys,
                    c,
                )
            } else {
                make(
                    n.key.clone(),
                    n.priority,
                    Some(l),
                    n.right.clone(),
                    memory,
                    class,
                    allocated,
                    work,
                    keys,
                    c,
                )
            }
        }
        Ordering::Greater => {
            let r = put(
                &n.right, key, priority, memory, class, allocated, work, keys, c,
            )?;
            if earlier(&r, n, work, keys, c)? {
                let l = make(
                    n.key.clone(),
                    n.priority,
                    n.left.clone(),
                    r.left.clone(),
                    memory,
                    class,
                    allocated,
                    work,
                    keys,
                    c,
                )?;
                make(
                    r.key.clone(),
                    r.priority,
                    Some(l),
                    r.right.clone(),
                    memory,
                    class,
                    allocated,
                    work,
                    keys,
                    c,
                )
            } else {
                make(
                    n.key.clone(),
                    n.priority,
                    n.left.clone(),
                    Some(r),
                    memory,
                    class,
                    allocated,
                    work,
                    keys,
                    c,
                )
            }
        }
    }
}
fn join(
    left: &Link,
    right: &Link,
    memory: &MemoryBudget,
    allocated: &mut usize,
    work: &mut ArtifactConfigReadWorkV1,
    keys: &mut SemanticKeyBudget,
    c: &mut impl FnMut() -> bool,
) -> Result<Link, ValueTreeError> {
    charge(work, keys, 0, c)?;
    match (left, right) {
        (None, _) => Ok(right.clone()),
        (_, None) => Ok(left.clone()),
        (Some(l), Some(r)) => {
            if earlier(l, r, work, keys, c)? {
                let next = join(&l.right, right, memory, allocated, work, keys, c)?;
                Ok(Some(make(
                    l.key.clone(),
                    l.priority,
                    l.left.clone(),
                    next,
                    memory,
                    MemoryClass::Ordinary,
                    allocated,
                    work,
                    keys,
                    c,
                )?))
            } else {
                let next = join(left, &r.left, memory, allocated, work, keys, c)?;
                Ok(Some(make(
                    r.key.clone(),
                    r.priority,
                    next,
                    r.right.clone(),
                    memory,
                    MemoryClass::Ordinary,
                    allocated,
                    work,
                    keys,
                    c,
                )?))
            }
        }
    }
}
fn remove(
    root: &Link,
    key: &[u8],
    memory: &MemoryBudget,
    allocated: &mut usize,
    work: &mut ArtifactConfigReadWorkV1,
    keys: &mut SemanticKeyBudget,
    c: &mut impl FnMut() -> bool,
) -> Result<Link, ValueTreeError> {
    let n = root.as_ref().ok_or_else(invalid)?;
    match compare(key, &n.key, work, keys, c)? {
        Ordering::Equal => join(&n.left, &n.right, memory, allocated, work, keys, c),
        Ordering::Less => {
            let left = remove(&n.left, key, memory, allocated, work, keys, c)?;
            Ok(Some(make(
                n.key.clone(),
                n.priority,
                left,
                n.right.clone(),
                memory,
                MemoryClass::Ordinary,
                allocated,
                work,
                keys,
                c,
            )?))
        }
        Ordering::Greater => {
            let right = remove(&n.right, key, memory, allocated, work, keys, c)?;
            Ok(Some(make(
                n.key.clone(),
                n.priority,
                n.left.clone(),
                right,
                memory,
                MemoryClass::Ordinary,
                allocated,
                work,
                keys,
                c,
            )?))
        }
    }
}
impl RegistryRecordKeysV1 {
    pub(super) fn empty(memory: &MemoryBudget, class: MemoryClass) -> Result<Self, ValueTreeError> {
        let bytes = arc_bytes::<Self>()?;
        Ok(Self {
            root: None,
            len: 0,
            recovery_bytes: if class == MemoryClass::Recovery {
                bytes
            } else {
                0
            },
            coverage: None,
            fee: memory.reserve(bytes, class)?,
        })
    }
    pub(super) fn len(&self) -> u64 {
        self.len
    }
    pub(super) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.fee.uses_budget(memory)
            && self
                .coverage
                .as_ref()
                .is_none_or(|fee| fee.uses_budget(memory))
    }
    pub(super) fn contains(
        &self,
        key: &[u8],
        work: &mut ArtifactConfigReadWorkV1,
        keys: &mut SemanticKeyBudget,
        c: &mut impl FnMut() -> bool,
    ) -> Result<bool, ValueTreeError> {
        let mut at = self.root.as_ref();
        while let Some(n) = at {
            match compare(key, &n.key, work, keys, c)? {
                Ordering::Equal => return Ok(true),
                Ordering::Less => at = n.left.as_ref(),
                Ordering::Greater => at = n.right.as_ref(),
            }
        }
        Ok(false)
    }
    pub(super) fn insert(
        &mut self,
        key: &[u8],
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut ArtifactConfigReadWorkV1,
        keys: &mut SemanticKeyBudget,
        c: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        charge(work, keys, key.len().checked_mul(2).ok_or_else(limited)?, c)?;
        let bytes = arc_bytes::<Vec<u8>>()?
            .checked_add(key.len())
            .ok_or_else(limited)?;
        let fee = memory.reserve(bytes, class)?;
        let id = Arc::new(fee.retain(key.to_vec()));
        let priority = *blake3::hash(key).as_bytes();
        let mut allocated = if class == MemoryClass::Recovery {
            bytes
        } else {
            0
        };
        let root = put(
            &self.root,
            id,
            priority,
            memory,
            class,
            &mut allocated,
            work,
            keys,
            c,
        )?;
        self.root = Some(root);
        self.len = self.len.checked_add(1).ok_or_else(limited)?;
        self.recovery_bytes = self
            .recovery_bytes
            .checked_add(allocated)
            .ok_or_else(limited)?;
        Ok(())
    }
    pub(in crate::incremental_runtime_artifact_config) fn ordinary(
        &self,
        memory: &MemoryBudget,
    ) -> Result<Self, ValueTreeError> {
        if !self.fee.uses_budget(memory) {
            return Err(invalid());
        }
        let coverage = match &self.coverage {
            Some(fee) => Some(fee.clone()),
            None if self.recovery_bytes != 0 => {
                let bytes = self
                    .recovery_bytes
                    .checked_add(
                        2 * std::mem::size_of::<usize>()
                            + 4 * std::mem::align_of::<MemoryReservation>(),
                    )
                    .ok_or_else(limited)?;
                Some(Arc::new(memory.reserve(bytes, MemoryClass::Ordinary)?))
            }
            None => None,
        };
        Ok(Self {
            root: self.root.clone(),
            len: self.len,
            recovery_bytes: self.recovery_bytes,
            coverage,
            fee: memory.reserve(arc_bytes::<Self>()?, MemoryClass::Ordinary)?,
        })
    }
    pub(in crate::incremental_runtime_artifact_config) fn change(
        &mut self,
        key: &[u8],
        before: bool,
        after: bool,
        memory: &MemoryBudget,
        work: &mut ArtifactConfigReadWorkV1,
        keys: &mut SemanticKeyBudget,
        c: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if self.contains(key, work, keys, c)? != before {
            return Err(invalid());
        }
        match (before, after) {
            (false, true) => self.insert(key, memory, MemoryClass::Ordinary, work, keys, c)?,
            (true, false) => {
                self.root = remove(&self.root, key, memory, &mut 0, work, keys, c)?;
                self.len = self.len.checked_sub(1).ok_or_else(invalid)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn visit<E: From<ValueTreeError>>(
        &self,
        work: &mut ArtifactConfigReadWorkV1,
        keys: &mut SemanticKeyBudget,
        c: &mut impl FnMut() -> bool,
        emit: &mut impl FnMut(
            &[u8],
            &mut ArtifactConfigReadWorkV1,
            &mut SemanticKeyBudget,
        ) -> Result<(), E>,
    ) -> Result<(), E> {
        fn walk<E: From<ValueTreeError>>(
            root: &Link,
            work: &mut ArtifactConfigReadWorkV1,
            keys: &mut SemanticKeyBudget,
            c: &mut impl FnMut() -> bool,
            emit: &mut impl FnMut(
                &[u8],
                &mut ArtifactConfigReadWorkV1,
                &mut SemanticKeyBudget,
            ) -> Result<(), E>,
        ) -> Result<(), E> {
            if let Some(n) = root {
                charge(work, keys, 0, c)?;
                walk(&n.left, work, keys, c, emit)?;
                emit(&n.key, work, keys)?;
                walk(&n.right, work, keys, c, emit)?
            }
            Ok(())
        }
        walk(&self.root, work, keys, c, emit)
    }
}
