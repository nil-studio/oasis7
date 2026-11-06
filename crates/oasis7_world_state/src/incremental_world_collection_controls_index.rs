//! Charged process-local AVL group counts, not durable wire or authority.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CollectionControlsGroupMetaV1 {
    pub key: [u8; 32],
    pub present: bool,
    pub members: u64,
}
pub(crate) struct CollectionControlsIndexWorkV1 {
    pub steps_remaining: u64,
}
impl CollectionControlsIndexWorkV1 {
    fn step(&mut self, c: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        if c() {
            return Err(ValueTreeError::Cancelled);
        }
        self.steps_remaining =
            self.steps_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "collection controls index steps",
                ))?;
        Ok(())
    }
}
type Link = Option<Arc<ReservedValue<Node>>>;
struct Node {
    meta: CollectionControlsGroupMetaV1,
    left: Link,
    right: Link,
    height: u32,
    len: usize,
    class: MemoryClass,
}
#[derive(Clone)]
pub(crate) struct CollectionControlsIndexV1 {
    root: Link,
    memory: MemoryBudget,
    class: MemoryClass,
}
fn height(n: &Link) -> u32 {
    n.as_ref().map_or(0, |n| n.height)
}
fn count(n: &Link) -> usize {
    n.as_ref().map_or(0, |n| n.len)
}
impl CollectionControlsIndexV1 {
    pub fn new(memory: &MemoryBudget, class: MemoryClass) -> Self {
        Self {
            root: None,
            memory: memory.clone(),
            class,
        }
    }
    pub fn len(&self) -> usize {
        count(&self.root)
    }
    pub fn class(&self) -> MemoryClass {
        self.class
    }
    pub fn uses_budget(&self, m: &MemoryBudget) -> bool {
        self.memory.same_pool(m)
    }
    fn check(&self, n: &Arc<ReservedValue<Node>>) -> Result<(), ValueTreeError> {
        if !n.uses_budget(&self.memory) || n.class != self.class {
            return Err(ValueTreeError::Invalid(
                "collection controls index pool/class",
            ));
        }
        Ok(())
    }
    fn node(
        &self,
        meta: CollectionControlsGroupMetaV1,
        left: Link,
        right: Link,
        w: &mut CollectionControlsIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Arc<ReservedValue<Node>>, ValueTreeError> {
        w.step(c)?;
        for n in [&left, &right].into_iter().flatten() {
            self.check(n)?;
        }
        let charge = self
            .memory
            .reserve(std::mem::size_of::<Node>() + 128, self.class)?;
        let height = 1 + height(&left).max(height(&right));
        let len = 1usize
            .checked_add(count(&left))
            .and_then(|n| n.checked_add(count(&right)))
            .ok_or(ValueTreeError::ResourceLimited(
                "collection controls index length",
            ))?;
        Ok(Arc::new(charge.retain(Node {
            meta,
            left,
            right,
            height,
            len,
            class: self.class,
        })))
    }
    fn balance(
        &self,
        m: CollectionControlsGroupMetaV1,
        l: Link,
        r: Link,
        w: &mut CollectionControlsIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Arc<ReservedValue<Node>>, ValueTreeError> {
        if height(&l) > height(&r) + 1 {
            let a = l.as_ref().unwrap();
            if height(&a.left) >= height(&a.right) {
                let right = self.node(m, a.right.clone(), r, w, c)?;
                return self.node(a.meta, a.left.clone(), Some(right), w, c);
            }
            let b = a.right.as_ref().unwrap();
            let left = self.node(a.meta, a.left.clone(), b.left.clone(), w, c)?;
            let right = self.node(m, b.right.clone(), r, w, c)?;
            return self.node(b.meta, Some(left), Some(right), w, c);
        }
        if height(&r) > height(&l) + 1 {
            let a = r.as_ref().unwrap();
            if height(&a.right) >= height(&a.left) {
                let left = self.node(m, l, a.left.clone(), w, c)?;
                return self.node(a.meta, Some(left), a.right.clone(), w, c);
            }
            let b = a.left.as_ref().unwrap();
            let left = self.node(m, l, b.left.clone(), w, c)?;
            let right = self.node(a.meta, b.right.clone(), a.right.clone(), w, c)?;
            return self.node(b.meta, Some(left), Some(right), w, c);
        }
        self.node(m, l, r, w, c)
    }
    fn update(
        &self,
        n: &Link,
        key: [u8; 32],
        value: Option<CollectionControlsGroupMetaV1>,
        w: &mut CollectionControlsIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Link, ValueTreeError> {
        w.step(c)?;
        let Some(n) = n else {
            return value.map(|m| self.node(m, None, None, w, c)).transpose();
        };
        self.check(n)?;
        let (m, l, r) = if key < n.meta.key {
            (
                n.meta,
                self.update(&n.left, key, value, w, c)?,
                n.right.clone(),
            )
        } else if key > n.meta.key {
            (
                n.meta,
                n.left.clone(),
                self.update(&n.right, key, value, w, c)?,
            )
        } else if let Some(m) = value {
            (m, n.left.clone(), n.right.clone())
        } else {
            if n.left.is_none() {
                return Ok(n.right.clone());
            }
            if n.right.is_none() {
                return Ok(n.left.clone());
            }
            let mut next = n.right.as_ref().unwrap();
            loop {
                w.step(c)?;
                self.check(next)?;
                match &next.left {
                    Some(l) => next = l,
                    None => break,
                }
            }
            let m = next.meta;
            (m, n.left.clone(), self.update(&n.right, m.key, None, w, c)?)
        };
        Ok(Some(self.balance(m, l, r, w, c)?))
    }
    pub fn insert(
        &self,
        m: CollectionControlsGroupMetaV1,
        w: &mut CollectionControlsIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        Ok(Self {
            root: self.update(&self.root, m.key, Some(m), w, c)?,
            memory: self.memory.clone(),
            class: self.class,
        })
    }
    pub fn remove(
        &self,
        p: [u8; 32],
        w: &mut CollectionControlsIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        Ok(Self {
            root: self.update(&self.root, p, None, w, c)?,
            memory: self.memory.clone(),
            class: self.class,
        })
    }
    pub fn get(
        &self,
        p: [u8; 32],
        w: &mut CollectionControlsIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Option<CollectionControlsGroupMetaV1>, ValueTreeError> {
        let mut n = &self.root;
        while let Some(a) = n {
            w.step(c)?;
            self.check(a)?;
            if p == a.meta.key {
                return Ok(Some(a.meta));
            }
            n = if p < a.meta.key { &a.left } else { &a.right };
        }
        Ok(None)
    }
    pub fn visit_page(
        &self,
        after: Option<[u8; 32]>,
        limit: usize,
        w: &mut CollectionControlsIndexWorkV1,
        c: &mut impl FnMut() -> bool,
        mut emit: impl FnMut(CollectionControlsGroupMetaV1) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        fn visit(
            index: &CollectionControlsIndexV1,
            n: &Link,
            after: Option<[u8; 32]>,
            left: &mut usize,
            w: &mut CollectionControlsIndexWorkV1,
            c: &mut impl FnMut() -> bool,
            emit: &mut impl FnMut(CollectionControlsGroupMetaV1) -> Result<(), ValueTreeError>,
        ) -> Result<(), ValueTreeError> {
            if *left == 0 {
                return Ok(());
            }
            let Some(n) = n else { return Ok(()) };
            w.step(c)?;
            index.check(n)?;
            if after.is_none_or(|p| n.meta.key > p) {
                visit(index, &n.left, after, left, w, c, emit)?;
                if *left == 0 {
                    return Ok(());
                }
                emit(n.meta)?;
                *left -= 1;
            }
            visit(index, &n.right, after, left, w, c, emit)
        }
        visit(self, &self.root, after, &mut { limit }, w, c, &mut emit)
    }
    pub fn readmit(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        w: &mut CollectionControlsIndexWorkV1,
        c: &mut impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        if !self.uses_budget(memory) || class != MemoryClass::Ordinary {
            return Err(ValueTreeError::Invalid(
                "collection controls index readmission pool/class",
            ));
        }
        fn copy(
            src: &CollectionControlsIndexV1,
            dst: &CollectionControlsIndexV1,
            n: &Link,
            w: &mut CollectionControlsIndexWorkV1,
            c: &mut impl FnMut() -> bool,
        ) -> Result<Link, ValueTreeError> {
            let Some(n) = n else { return Ok(None) };
            w.step(c)?;
            src.check(n)?;
            let l = copy(src, dst, &n.left, w, c)?;
            let r = copy(src, dst, &n.right, w, c)?;
            Ok(Some(dst.node(n.meta, l, r, w, c)?))
        }
        let mut dst = Self::new(memory, class);
        dst.root = copy(self, &dst, &self.root, w, c)?;
        Ok(dst)
    }
}
