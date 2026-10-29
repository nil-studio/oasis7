//! Full current value closure, including the data leaf of an empty value.
//! Traversal counts occurrences, never keeps an object-sized visited set.
use super::*;

/// Byte/tree integrity only. No ownership, retention, durability or authority.
#[derive(Debug)]
pub struct ValueClosureSummary {
    root: ValueRootV1,
    node_occurrences: u64,
}
impl ValueClosureSummary {
    pub fn root(&self) -> ValueRootV1 {
        self.root
    }
    /// Includes the descriptor and repeated references in their original order.
    pub fn node_occurrences(&self) -> u64 {
        self.node_occurrences
    }
}
impl<R: ValueNodeReader> ValueReadContext<'_, R> {
    /// Read the authenticated descriptor and every typed descendant. Unlike a
    /// zero-length range this must fetch and check the empty data leaf too.
    /// No output-byte allowance is consumed: the verified bytes are discarded.
    pub fn verify_closure(
        &mut self,
        expected_id: &[u8; 32],
        max_logical_bytes: u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ValueClosureSummary, ValueTreeError> {
        let _workspace = self
            .memory
            .reserve(VALUE_READ_WORKSPACE_BYTES, self.class)?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let root = self.load_root(expected_id)?;
        if root.0.logical_len > max_logical_bytes {
            return Err(ValueTreeError::ResourceLimited("closure logical bytes"));
        }
        // At most fanout-1 siblings per remaining depth plus one current node.
        let capacity = usize::from(VALUE_MAX_DEPTH) * (VALUE_FANOUT - 1) + 1;
        let mut stack = Vec::with_capacity(capacity);
        stack.push(root.0);
        let mut nodes = 1u64;
        let mut bytes = 0u64;
        while let Some(reference) = stack.pop() {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            let page =
                read_verified_node(&reference, self.source, self.memory, self.class, self.io)?;
            nodes = nodes
                .checked_add(1)
                .ok_or(ValueTreeError::ResourceLimited("closure node total"))?;
            match decode_node(&page, &reference)? {
                Node::Leaf(data) => {
                    bytes = bytes
                        .checked_add(data.len() as u64)
                        .ok_or(ValueTreeError::Invalid("closure byte overflow"))?;
                }
                Node::Index(children) => {
                    if stack.len() + children.len() > capacity {
                        return Err(ValueTreeError::Invalid("closure traversal depth"));
                    }
                    stack.extend(children.into_iter().rev());
                }
            }
        }
        if bytes != root.0.logical_len {
            return Err(ValueTreeError::Invalid("closure logical length"));
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(ValueClosureSummary {
            root,
            node_occurrences: nodes,
        })
    }
}
