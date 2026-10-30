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
        for page in &delta.new_nodes {
            page.verified_links()
                .map_err(|_| ValueTreeError::Invalid("schedule producer page"))?;
        }
        let io = self
            .io
            .get_mut()
            .map_err(|_| ValueTreeError::Invalid("physical session poisoned"))?;
        persist_nodes(
            self.store,
            MemoryClass::Ordinary,
            io,
            &self.cancelled,
            delta
                .new_nodes
                .iter()
                .map(|n| (n.digest.as_str(), n.bytes.as_slice())),
        )
        .map_err(value_error)
    }
}

impl oasis7_formal_protocol::incremental_state::StatePageWriter for ValuePageSession<'_> {
    fn put_state_pages(
        &mut self,
        delta: &ReservedValue<oasis7_formal_protocol::incremental_state::StateDeltaV1>,
        memory: &MemoryBudget,
    ) -> std::result::Result<(), ValueTreeError> {
        if !self.store.memory.same_pool(memory) || !delta.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("state producer pool"));
        }
        for page in &delta.new_nodes {
            page.verified_links()
                .map_err(|_| ValueTreeError::Invalid("state producer page"))?;
        }
        let io = self
            .io
            .get_mut()
            .map_err(|_| ValueTreeError::Invalid("physical session poisoned"))?;
        persist_nodes(
            self.store,
            MemoryClass::Ordinary,
            io,
            &self.cancelled,
            delta
                .new_nodes
                .iter()
                .map(|n| (n.digest.as_str(), n.bytes.as_slice())),
        )
        .map_err(value_error)
    }
}

#[cfg(all(test, unix))]
#[path = "incremental_producer_page_writers_tests.rs"]
mod tests;
