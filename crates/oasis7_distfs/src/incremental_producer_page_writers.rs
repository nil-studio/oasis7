//! Persist provisional typed producer pages through the existing bounded batch path.
//! Content persistence does not select or authorize a head.
use super::*;

impl oasis7_formal_protocol::incremental_schedule::SchedulePageWriter for ValuePageSession<'_> {
    fn put_schedule_pages(
        &mut self,
        delta: &ReservedValue<oasis7_formal_protocol::incremental_schedule::ScheduleDeltaV1>,
        memory: &MemoryBudget,
    ) -> std::result::Result<(), ValueTreeError> {
        if !self.store.memory.same_pool(memory) || !delta.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("schedule producer pool"));
        }
        self.check_cancelled().map_err(value_error)?;
        {
            // Validation finishes before the separate pack-metadata phase.
            let _validation = memory.reserve(64 * 1024, MemoryClass::Ordinary)?;
            for page in &delta.new_nodes {
                self.check_cancelled().map_err(value_error)?;
                page.verified_links()
                    .map_err(|_| ValueTreeError::Invalid("schedule producer page"))?;
            }
        }
        // The same caller-owned flag used by physical value reads must guard
        // production schedule page publication as well as put_node.
        let physical = self.caller_cancelled.unwrap_or(&self.cancelled);
        let local_cancelled = &self.cancelled;
        let io = self
            .io
            .get_mut()
            .map_err(|_| ValueTreeError::Invalid("physical session poisoned"))?;
        // The caller's atomic flag and the local session stop both retain
        // physical I/O authority, not only a post-publication success check.
        let result = persist_nodes_profile_cancelled(
            self.store,
            MemoryClass::Ordinary,
            io,
            physical,
            delta
                .new_nodes
                .iter()
                .map(|n| (n.digest.as_str(), n.bytes.as_slice())),
            self.store.compression,
            &mut || local_cancelled.load(Ordering::Acquire),
        )
        .map_err(value_error);
        // Do not turn a physical failure into a cancellation result. But never
        // report a successful batch after the caller or local flag trips.
        if result.is_ok() {
            self.check_cancelled().map_err(value_error)?;
        }
        result
    }
}

impl oasis7_formal_protocol::incremental_state::StatePageWriter for ValuePageSession<'_> {
    fn put_state_pages(
        &mut self,
        delta: &ReservedValue<oasis7_formal_protocol::incremental_state::StateDeltaV1>,
        memory: &MemoryBudget,
    ) -> std::result::Result<(), ValueTreeError> {
        oasis7_formal_protocol::incremental_state::StatePageWriter::put_state_pages_cancelled(
            self,
            delta,
            memory,
            &mut || false,
        )
    }

    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<oasis7_formal_protocol::incremental_state::StateDeltaV1>,
        memory: &MemoryBudget,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> std::result::Result<(), ValueTreeError> {
        if !self.store.memory.same_pool(memory) || !delta.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("state producer pool"));
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        self.check_cancelled().map_err(value_error)?;
        {
            let _validation = memory.reserve(64 * 1024, MemoryClass::Ordinary)?;
            for page in &delta.new_nodes {
                if cancelled() {
                    return Err(ValueTreeError::Cancelled);
                }
                self.check_cancelled().map_err(value_error)?;
                page.verified_links()
                    .map_err(|_| ValueTreeError::Invalid("state producer page"))?;
            }
        }
        // Preserve both cancellation boundaries: the state producer's
        // cooperative callback and the physical session's caller-owned flag.
        let physical = self.caller_cancelled.unwrap_or(&self.cancelled);
        let local_cancelled = &self.cancelled;
        let io = self
            .io
            .get_mut()
            .map_err(|_| ValueTreeError::Invalid("physical session poisoned"))?;
        let result = persist_nodes_profile_cancelled(
            self.store,
            MemoryClass::Ordinary,
            io,
            physical,
            delta
                .new_nodes
                .iter()
                .map(|n| (n.digest.as_str(), n.bytes.as_slice())),
            self.store.compression,
            &mut || cancelled() || local_cancelled.load(Ordering::Acquire),
        )
        .map_err(value_error);
        if result.is_ok() {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            self.check_cancelled().map_err(value_error)?;
        }
        result
    }
}

#[cfg(all(test, unix))]
#[path = "incremental_producer_page_writers_tests.rs"]
mod tests;
