//! Changed-path traversal for qualified organization inner roots.
//! Equal authenticated subtrees are deliberately never opened. The owner
//! receiver still has to check each changed leaf's schema and relationships.
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, STATE_MAX_PAGE_BYTES, StateEntryV1, StateNamespaceV1, StateNodeWriteV1,
        StatePageLinksV1, StateRootV1, StateScanBudget,
    },
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};

pub struct OrganizationChangedTreeContextV1<'a, R, C> {
    pub source: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub budget: &'a mut StateScanBudget,
    pub cancelled: &'a mut C,
}
impl<R: MerkleNodeReader, C: FnMut() -> bool> OrganizationChangedTreeContextV1<'_, R, C> {
    pub fn visit(
        &mut self,
        before: &StateRootV1,
        after: &StateRootV1,
        mut changed: impl FnMut(
            Option<&StateEntryV1>,
            Option<&StateEntryV1>,
        ) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        self.check()?;
        if self.source.allocation_class() != Some(self.class)
            || !self
                .source
                .allocation_budget()
                .is_some_and(|m| m.same_pool(self.memory))
        {
            return Err(ValueTreeError::Invalid(
                "organization diff source pool/class",
            ));
        }
        for root in [before, after] {
            root.validate()
                .map_err(|_| ValueTreeError::Invalid("organization diff root"))?;
            if root.namespace != StateNamespaceV1::Authorization {
                return Err(ValueTreeError::Invalid("organization diff namespace"));
            }
            if root.leaf_count == 0
                && root
                    != &StateRootV1::empty(root.namespace)
                        .map_err(|_| ValueTreeError::Invalid("organization empty root"))?
            {
                return Err(ValueTreeError::Invalid("organization empty root count"));
            }
        }
        // Fixed-depth recursion retains at most two decoded branch pages per
        // level. Admission covers those strings, frames and the leaf payloads.
        let _workspace = self.memory.reserve(512 * 1024, self.class)?;
        let mut counts = [0u64; 2];
        self.walk(
            (before.leaf_count != 0).then_some(before.node_root.as_str()),
            (after.leaf_count != 0).then_some(after.node_root.as_str()),
            0,
            &mut [0; 32],
            &mut counts,
            &mut changed,
        )?;
        if i128::from(after.leaf_count) - i128::from(before.leaf_count)
            != i128::from(counts[1]) - i128::from(counts[0])
        {
            return Err(ValueTreeError::Invalid("organization diff leaf count"));
        }
        self.check()
    }
    fn check(&mut self) -> Result<(), ValueTreeError> {
        if (self.cancelled)() {
            Err(ValueTreeError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn page(
        &mut self,
        id: Option<&str>,
        depth: usize,
        path: &[u8; 32],
    ) -> Result<Option<StatePageLinksV1>, ValueTreeError> {
        self.check()?;
        let Some(id) = id else {
            return Ok(None);
        };
        if self.budget.nodes_remaining == 0 || self.budget.read_bytes_remaining == 0 {
            return Err(ValueTreeError::ResourceLimited(
                "organization diff node allowance",
            ));
        }
        self.budget.nodes_remaining -= 1;
        let admitted = self
            .budget
            .read_bytes_remaining
            .min(STATE_MAX_PAGE_BYTES as u64) as usize;
        self.budget.read_bytes_remaining -= admitted as u64;
        let result = self.source.read_node(id, admitted);
        self.check()?;
        let page = result
            .map_err(|_| {
                if admitted < STATE_MAX_PAGE_BYTES {
                    ValueTreeError::ResourceLimited("organization diff page allowance")
                } else {
                    ValueTreeError::Invalid("organization diff page read")
                }
            })?
            .ok_or(ValueTreeError::Invalid("organization diff missing page"))?;
        self.check()?;
        let bytes = page.as_ref();
        if bytes.len() > admitted {
            return Err(ValueTreeError::Invalid("organization diff page bound"));
        }
        self.budget.read_bytes_remaining += (admitted - bytes.len()) as u64;
        let _copy = self.memory.reserve(bytes.len(), self.class)?;
        let links = StateNodeWriteV1 {
            digest: id.to_owned(),
            bytes: bytes.to_vec(),
        }
        .verified_links()
        .map_err(|_| ValueTreeError::Invalid("organization diff page integrity"))?;
        match &links {
            StatePageLinksV1::Branch { depth: actual, .. }
                if depth < 256 && usize::from(*actual) == depth => {}
            StatePageLinksV1::Value(entry) if depth == 256 => {
                if entry.namespace != StateNamespaceV1::Authorization
                    || entry
                        .key_digest()
                        .map_err(|_| ValueTreeError::Invalid("organization diff leaf key"))?
                        != *path
                {
                    return Err(ValueTreeError::Invalid("organization diff leaf route"));
                }
                if self.budget.leaves_remaining == 0 {
                    return Err(ValueTreeError::ResourceLimited(
                        "organization diff leaf allowance",
                    ));
                }
                self.budget.leaves_remaining -= 1;
            }
            _ => return Err(ValueTreeError::Invalid("organization diff page depth")),
        }
        self.check()?;
        Ok(Some(links))
    }
    fn walk(
        &mut self,
        before: Option<&str>,
        after: Option<&str>,
        depth: usize,
        path: &mut [u8; 32],
        counts: &mut [u64; 2],
        changed: &mut impl FnMut(
            Option<&StateEntryV1>,
            Option<&StateEntryV1>,
        ) -> Result<(), ValueTreeError>,
    ) -> Result<(), ValueTreeError> {
        self.check()?;
        if before == after {
            return self.check();
        }
        let old = self.page(before, depth, path)?;
        let new = self.page(after, depth, path)?;
        if depth == 256 {
            let leaf = |node| match node {
                Some(StatePageLinksV1::Value(entry)) => Some(entry),
                _ => None,
            };
            let old = leaf(old);
            let new = leaf(new);
            for (count, exists) in counts.iter_mut().zip([old.is_some(), new.is_some()]) {
                *count = count
                    .checked_add(u64::from(exists))
                    .ok_or(ValueTreeError::Invalid("organization diff count overflow"))?;
            }
            changed(old.as_ref(), new.as_ref())?;
            return self.check();
        }
        let children = |node| match node {
            Some(StatePageLinksV1::Branch { children, .. }) => children,
            _ => [None, None],
        };
        let old = children(old);
        let new = children(new);
        let byte = depth / 8;
        let mask = 1 << (7 - depth % 8);
        for side in 0..2 {
            path[byte] = (path[byte] & !mask) | if side == 1 { mask } else { 0 };
            self.walk(
                old[side].as_deref(),
                new[side].as_deref(),
                depth + 1,
                path,
                counts,
                changed,
            )?;
        }
        path[byte] &= !mask;
        self.check()
    }
}
