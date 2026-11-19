//! Locator hints select, but cannot authenticate, historical mailbox originals.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_material::*, incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::AgentMailboxReferenceV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1, incremental_world_raw_complete::*,
};
use std::sync::Arc;
pub(super) struct HistoricalMailboxReader<'a, P, L> {
    pub(super) head: &'a VerifiedFormalCommitV1,
    pub(super) roots: &'a RootSetV1,
    pub(super) pages: &'a P,
    pub(super) hint: L,
    pub(super) core: &'a mut CoreBudget,
    pub(super) facts: &'a mut FactBudget,
    pub(super) decode: WorldEventDecodeWorkV1,
    remaining: u64,
    memory: MemoryBudget,
    class: MemoryClass,
    _arcs: MemoryReservation,
}
impl<'a, P, L> HistoricalMailboxReader<'a, P, L> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        head: &'a VerifiedFormalCommitV1,
        roots: &'a RootSetV1,
        pages: &'a P,
        hint: L,
        core: &'a mut CoreBudget,
        facts: &'a mut FactBudget,
        decode: WorldEventDecodeWorkV1,
        occurrences: u64,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, ValueTreeError> {
        let bytes = usize::try_from(occurrences)
            .ok()
            .and_then(|n| n.checked_mul(256))
            .and_then(|n| n.checked_add(512))
            .ok_or(ValueTreeError::Invalid("mailbox historical Arc accounting"))?;
        Ok(Self {
            head,
            roots,
            pages,
            hint,
            core,
            facts,
            decode,
            remaining: occurrences,
            memory: memory.clone(),
            class,
            _arcs: memory.reserve(bytes, class)?,
        })
    }
}
impl<O, P, L> MailboxWorldEventOriginalReaderV1<O> for HistoricalMailboxReader<'_, P, L>
where
    O: OriginalObjectStreamReader,
    P: MerkleNodeReader,
    L: FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
{
    fn checked_world_event(
        &mut self,
        reference: AgentMailboxReferenceV1<'_>,
        scope: &MailboxProvenanceScopeV1<'_>,
        objects: &mut O,
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut MailboxWorldEventOriginalWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<
        Arc<ReservedValue<CheckedMailboxWorldEventOriginalV1>>,
        MailboxWorldEventOriginalErrorV1<O::Error>,
    > {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !memory.same_pool(&self.memory) || class != self.class {
            return Err(ValueTreeError::Invalid("mailbox historical pool/class").into());
        }
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "mailbox historical occurrences",
            ))?;
        let hint = (self.hint)(&reference.original)
            .map_err(|_| ValueTreeError::Invalid("mailbox historical locator unavailable"))?;
        let token = super::mailbox_history::historical_mailbox(
            self.head,
            self.roots,
            scope,
            reference.original,
            hint,
            objects,
            self.pages,
            memory,
            class,
            self.core,
            self.facts,
            &mut self.decode,
            work,
            &mut || cancelled(),
        )?;
        Ok(Arc::new(token))
    }
}
