//! Candidate EventFact-backed mailbox reader. Historical locators are required
//! separately; missing accepted provenance is rejected, never replaced by CAS.
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFactKindV1, incremental_material::*,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::AgentMailboxReferenceV1,
    incremental_world_event_decode::{WorldEventDecodeWorkV1, decode_world_event_original_v1},
    incremental_world_raw_complete::*,
};
use std::sync::Arc;
pub(super) struct CandidateMailboxReader {
    pub(super) decode: WorldEventDecodeWorkV1,
    remaining: u64,
    search_steps: u64,
    memory: MemoryBudget,
    class: MemoryClass,
    _arcs: MemoryReservation,
}
impl CandidateMailboxReader {
    pub(super) fn new(
        decode: WorldEventDecodeWorkV1,
        max_occurrences: u64,
        search_steps: u64,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Self, ValueTreeError> {
        let bytes = usize::try_from(max_occurrences)
            .ok()
            .and_then(|n| n.checked_mul(256))
            .and_then(|n| n.checked_add(512))
            .ok_or(ValueTreeError::Invalid("mailbox Arc admission overflow"))?;
        Ok(Self {
            decode,
            remaining: max_occurrences,
            search_steps,
            memory: memory.clone(),
            class,
            _arcs: memory.reserve(bytes, class)?,
        })
    }
}
impl<O: OriginalObjectStreamReader> MailboxWorldEventOriginalReaderV1<O>
    for CandidateMailboxReader
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
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "endpoint mailbox occurrences",
            ))?;
        let MailboxProvenanceScopeV1::Candidate {
            candidate,
            core,
            originals,
        } = scope
        else {
            return Err(
                ValueTreeError::Invalid("endpoint accepted mailbox locator required").into(),
            );
        };
        if !self.memory.same_pool(memory)
            || self.class != class
            || !objects.allocation_budget().same_pool(memory)
            || objects.allocation_class() != class
        {
            return Err(ValueTreeError::Invalid("endpoint mailbox reader pool/class").into());
        }
        let mut ordinal = None;
        for (index, fact) in core.facts().facts.iter().enumerate() {
            if cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            self.search_steps =
                self.search_steps
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "endpoint mailbox fact search",
                    ))?;
            if fact.kind == ExecutionFactKindV1::Event && fact.id == reference.original {
                ordinal = Some(index);
                break;
            }
        }
        let ordinal = ordinal.ok_or(ValueTreeError::Invalid(
            "endpoint inherited mailbox locator required",
        ))?;
        let occurrence = check_candidate_event_fact_v1(
            &CandidateEventFactRequestV1 {
                candidate,
                core,
                originals,
                ordinal: u64::try_from(ordinal)
                    .map_err(|_| ValueTreeError::Invalid("mailbox ordinal overflow"))?,
            },
            memory,
            &mut *cancelled,
        )
        .map_err(|e| match e {
            CoreError::Cancelled => ValueTreeError::Cancelled,
            CoreError::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
            _ => ValueTreeError::Invalid("endpoint candidate Event occurrence"),
        })?;
        let raw = objects
            .read_original(&reference.original, self.decode.max_original_bytes, memory)
            .map_err(MailboxWorldEventOriginalErrorV1::Source)?;
        if !raw.uses_budget(memory) || raw.len() > self.decode.max_original_bytes {
            return Err(ValueTreeError::Invalid("endpoint mailbox raw carrier pool/length").into());
        }
        let event =
            decode_world_event_original_v1(&raw, memory, class, &mut self.decode, &mut *cancelled)?;
        let token = verify_mailbox_world_event_original_v1::<O::Error>(
            Arc::new(event),
            MailboxWorldEventOccurrenceV1::Candidate(occurrence),
            scope,
            memory,
            class,
            work,
            &mut *cancelled,
        )?;
        Ok(Arc::new(token))
    }
}
