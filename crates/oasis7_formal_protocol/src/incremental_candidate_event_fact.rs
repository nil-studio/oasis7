//! Provisional current Event observations for candidate owner validation.
//! This type cannot be passed as accepted history or Domain provenance.
use super::*;

pub struct CandidateEventFactRequestV1<'a> {
    pub candidate: &'a VerifiedCandidateContextV1,
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub originals: &'a ReservedValue<CheckedFactOriginals>,
    pub ordinal: u64,
}

#[derive(Debug)]
pub struct CheckedCandidateEventFactOccurrenceV1 {
    candidate_record: [u8; 32],
    roots: [u8; 32],
    head_body: [u8; 32],
    event_body: [u8; 32],
    location: EventFactLocationV1,
    original_bytes: u64,
}
impl CheckedCandidateEventFactOccurrenceV1 {
    pub fn candidate_record_id(&self) -> [u8; 32] {
        self.candidate_record
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.head_body
    }
    pub fn location(&self) -> EventFactLocationV1 {
        self.location
    }
    pub fn original_bytes(&self) -> u64 {
        self.original_bytes
    }
}

/// Authenticate current observed bytes under the exact signed candidate core.
/// This provides no accepted-head, business or durability qualification.
pub fn check_candidate_event_fact_v1(
    request: &CandidateEventFactRequestV1<'_>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedCandidateEventFactOccurrenceV1>, CoreError<std::convert::Infallible>>
{
    let (mut workspace, binding) = checked_current_event_fact_binding(
        request.candidate.record(),
        request.core,
        request.originals,
        request.ordinal,
        memory,
        &mut cancelled,
    )?;
    let charge = workspace
        .split(std::mem::size_of::<CheckedCandidateEventFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("candidate event summary accounting"))?;
    Ok(charge.retain(CheckedCandidateEventFactOccurrenceV1 {
        candidate_record: binding.record,
        roots: binding.roots,
        head_body: binding.body,
        event_body: binding.body,
        location: binding.location,
        original_bytes: binding.original_bytes,
    }))
}
/// Acceptance must identify exactly the record checked before acceptance.
/// This does no source IO and cannot promote a prepared body by itself.
pub fn promote_candidate_event_fact_v1(
    checked: &ReservedValue<CheckedCandidateEventFactOccurrenceV1>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedEventFactOccurrenceV1>, CoreError<std::convert::Infallible>> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < std::mem::size_of::<CheckedCandidateEventFactOccurrenceV1>()
    {
        return Err(CoreError::Invalid("candidate event promotion pool/charge"));
    }
    let mut workspace = memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    let id = crate::incremental_history::wire::parse_hash(
        &accepted.record().record_id().map_err(BindingError::from)?,
    )?;
    if id != checked.candidate_record {
        return Err(CoreError::Invalid(
            "candidate event promotion accepted record",
        ));
    }
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    let charge = workspace
        .split(std::mem::size_of::<VerifiedEventFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("candidate event promotion accounting"))?;
    Ok(charge.retain(VerifiedEventFactOccurrenceV1 {
        head_record: id,
        roots: checked.roots,
        head_body: checked.head_body,
        event_body: checked.event_body,
        location: checked.location,
        original_bytes: checked.original_bytes,
    }))
}
