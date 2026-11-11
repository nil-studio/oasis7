//! An untrusted locator only selects a real authenticated Event occurrence.
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1, incremental_execution::RootSetV1,
    incremental_material::*, incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_world_event_decode::{WorldEventDecodeWorkV1, decode_world_event_original_v1},
    incremental_world_raw_complete::*,
};
use std::sync::Arc;
/// The caller must retain its prepaid Arc metadata admission for this token's
/// use, as the candidate reader does. No locator is treated as provenance.
#[allow(clippy::too_many_arguments)]
pub(super) fn historical_mailbox<O, R, C>(
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    scope: &MailboxProvenanceScopeV1<'_>,
    expected: [u8; 32],
    hint: EventFactLocationV1,
    objects: &mut O,
    pages: &R,
    memory: &MemoryBudget,
    class: MemoryClass,
    core_budget: &mut CoreBudget,
    fact_budget: &mut FactBudget,
    decode: &mut WorldEventDecodeWorkV1,
    work: &mut MailboxWorldEventOriginalWorkV1,
    cancelled: &mut C,
) -> Result<
    ReservedValue<CheckedMailboxWorldEventOriginalV1>,
    MailboxWorldEventOriginalErrorV1<O::Error>,
>
where
    O: OriginalObjectStreamReader,
    R: MerkleNodeReader,
    C: FnMut() -> bool,
{
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if hint.original != expected {
        return Err(ValueTreeError::Invalid("mailbox untrusted locator original mismatch").into());
    }
    let occurrence = verify_event_fact_occurrence_v1(
        &EventFactRequestV1 {
            head,
            roots,
            location: hint,
        },
        &mut EventFactReadContextV1 {
            originals: objects,
            catalog_pages: pages,
            memory,
            class,
            core_budget,
            fact_budget,
            cancelled: &mut *cancelled,
        },
    )
    .map_err(|e| match e {
        CoreError::Object(e) => MailboxWorldEventOriginalErrorV1::Source(e),
        CoreError::Cancelled => ValueTreeError::Cancelled.into(),
        CoreError::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason).into(),
        _ => ValueTreeError::Invalid("mailbox actual historical Event proof").into(),
    })?;
    let raw = objects
        .read_original(&expected, decode.max_original_bytes, memory)
        .map_err(MailboxWorldEventOriginalErrorV1::Source)?;
    if !raw.uses_budget(memory) || raw.len() > decode.max_original_bytes {
        return Err(ValueTreeError::Invalid("mailbox historical raw carrier").into());
    }
    let event = decode_world_event_original_v1(&raw, memory, class, decode, &mut *cancelled)?;
    verify_mailbox_world_event_original_v1::<O::Error>(
        Arc::new(event),
        MailboxWorldEventOccurrenceV1::Accepted(occurrence),
        scope,
        memory,
        class,
        work,
        &mut *cancelled,
    )
}
