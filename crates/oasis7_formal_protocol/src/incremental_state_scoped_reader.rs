//! Finite reader scope without an implicit walk or invented allowances.
use super::*;
impl<R: MerkleNodeReader> StateScanContext<'_, R> {
    /// The callback may perform guarded point reads and child walks. All work
    /// uses these existing allowances. No root/tree traversal happens here.
    /// A swallowed reader failure cannot turn into a successful scope.
    pub fn with_reader<T, E, C: FnMut() -> bool>(
        &mut self,
        cancelled: C,
        visit: impl FnOnce(&StateScanReader<'_, R, C>) -> Result<T, E>,
    ) -> Result<T, StateScanError<E>> {
        let cancelled = RefCell::new(cancelled);
        if cancelled.borrow_mut()() {
            return Err(StateScanError::Cancelled);
        }
        let _workspace = match self.memory.reserve(128 << 10, self.class) {
            Ok(charge) => charge,
            Err(e) => {
                if cancelled.borrow_mut()() {
                    return Err(StateScanError::Cancelled);
                }
                return Err(StateScanError::Memory(e));
            }
        };
        if cancelled.borrow_mut()() {
            return Err(StateScanError::Cancelled);
        }
        if self.source.allocation_class() != Some(self.class)
            || self
                .source
                .allocation_budget()
                .is_none_or(|m| !m.same_pool(self.memory))
        {
            return Err(StateScanError::Invalid("state scan source pool/class"));
        }
        if cancelled.borrow_mut()() {
            return Err(StateScanError::Cancelled);
        }
        let stop = Cell::new(None);
        let failed = Cell::new(false);
        let reader = StateScanReader {
            source: self.source,
            memory: self.memory,
            class: self.class,
            nodes: Cell::from_mut(&mut self.io.nodes_remaining),
            bytes: Cell::from_mut(&mut self.io.read_bytes_remaining),
            leaves: Cell::from_mut(&mut self.io.leaves_remaining),
            cancelled: &cancelled,
            stop: &stop,
            failed: Some(&failed),
        };
        let result = visit(&reader);
        if cancelled.borrow_mut()() {
            return Err(StateScanError::Cancelled);
        }
        match stop.get() {
            Some(Stop::Cancelled) => return Err(StateScanError::Cancelled),
            Some(Stop::Limited) => return Err(StateScanError::ResourceLimited),
            None => (),
        }
        if result.is_ok() && failed.get() {
            return Err(StateScanError::Invalid(
                "scoped state reader failure ignored",
            ));
        }
        result.map_err(StateScanError::Visitor)
    }
}
