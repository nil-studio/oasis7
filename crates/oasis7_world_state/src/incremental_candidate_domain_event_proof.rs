//! Candidate-only concrete Domain semantics, promoted only after exact acceptance.
use super::*;
use oasis7_formal_protocol::incremental_material::CheckedCandidateEventFactOccurrenceV1;

pub struct CandidateDomainEventProofRequestV1<'a> {
    pub event: &'a WorldEvent,
    pub occurrence: &'a ReservedValue<CheckedCandidateEventFactOccurrenceV1>,
    pub expected_candidate_record: [u8; 32],
    pub expected_roots: [u8; 32],
}
/// Deliberately distinct from accepted Domain provenance and not Clone.
pub struct CheckedCandidateDomainEventOccurrenceV1<'a> {
    event: &'a WorldEvent,
    occurrence: &'a ReservedValue<CheckedCandidateEventFactOccurrenceV1>,
}
impl CheckedCandidateDomainEventOccurrenceV1<'_> {
    pub fn event(&self) -> &WorldEvent {
        self.event
    }
    pub fn domain(&self) -> &DomainEvent {
        match &self.event.body {
            WorldEventBody::Domain(domain) => domain,
            _ => unreachable!("private candidate constructor checks Domain"),
        }
    }
    pub fn occurrence(&self) -> &CheckedCandidateEventFactOccurrenceV1 {
        self.occurrence
    }
}

pub fn check_candidate_domain_event_occurrence_v1<'a, C: FnMut() -> bool>(
    request: &CandidateDomainEventProofRequestV1<'a>,
    context: &mut DomainEventProofContextV1<'_, C>,
) -> Result<ReservedValue<CheckedCandidateDomainEventOccurrenceV1<'a>>, CanonicalCborStreamError> {
    use CanonicalCborStreamError as Error;
    if context.poisoned {
        return Err(Error::Poisoned);
    }
    context.poisoned = true;
    if (context.cancelled)() {
        return Err(Error::Cancelled);
    }
    let raw = request.occurrence;
    if !raw.uses_budget(context.memory)
        || raw.reserved_bytes() < std::mem::size_of::<CheckedCandidateEventFactOccurrenceV1>()
    {
        return Err(Error::Invalid("candidate Domain provenance pool/charge"));
    }
    if raw.candidate_record_id() != request.expected_candidate_record
        || raw.roots_id() != request.expected_roots
    {
        return Err(Error::Invalid("candidate Domain record/roots"));
    }
    if !matches!(&request.event.body, WorldEventBody::Domain(_)) {
        return Err(Error::Invalid("candidate Event is not Domain"));
    }
    let charge = check_domain_canonical(
        request.event,
        raw.location().original,
        raw.original_bytes(),
        std::mem::size_of::<CheckedCandidateDomainEventOccurrenceV1<'a>>(),
        context,
    )?;
    context.poisoned = false;
    Ok(charge.retain(CheckedCandidateDomainEventOccurrenceV1 {
        event: request.event,
        occurrence: raw,
    }))
}

pub struct CandidateDomainPromotionRequestV1<'a, 'event> {
    pub checked: &'a ReservedValue<CheckedCandidateDomainEventOccurrenceV1<'event>>,
    pub accepted_occurrence: &'a ReservedValue<VerifiedEventFactOccurrenceV1>,
    pub expected_head_record: [u8; 32],
    pub expected_roots: [u8; 32],
}
/// The accepted raw occurrence requires a real accepted commit. Reuse the
/// already checked immutable event only when every candidate binding matches.
pub fn promote_candidate_domain_event_occurrence_v1<'a, 'event: 'a, C: FnMut() -> bool>(
    request: &CandidateDomainPromotionRequestV1<'a, 'event>,
    context: &mut DomainEventProofContextV1<'_, C>,
) -> Result<ReservedValue<VerifiedDomainEventOccurrenceV1<'a>>, CanonicalCborStreamError> {
    use CanonicalCborStreamError as Error;
    if context.poisoned {
        return Err(Error::Poisoned);
    }
    context.poisoned = true;
    if (context.cancelled)() {
        return Err(Error::Cancelled);
    }
    let checked = request.checked;
    let candidate = checked.occurrence;
    let raw = request.accepted_occurrence;
    if !checked.uses_budget(context.memory)
        || checked.reserved_bytes()
            < std::mem::size_of::<CheckedCandidateDomainEventOccurrenceV1<'_>>()
        || !candidate.uses_budget(context.memory)
        || candidate.reserved_bytes() < std::mem::size_of::<CheckedCandidateEventFactOccurrenceV1>()
        || !raw.uses_budget(context.memory)
        || raw.reserved_bytes() < std::mem::size_of::<VerifiedEventFactOccurrenceV1>()
    {
        return Err(Error::Invalid("candidate Domain promotion pool/charge"));
    }
    if raw.head_record_id() != request.expected_head_record
        || raw.roots_id() != request.expected_roots
        || candidate.candidate_record_id() != raw.head_record_id()
        || candidate.roots_id() != raw.roots_id()
        || candidate.body_id() != raw.head_body_id()
        || candidate.body_id() != raw.event_body_id()
        || candidate.location() != raw.location()
        || candidate.original_bytes() != raw.original_bytes()
    {
        return Err(Error::Invalid(
            "candidate Domain promotion exact acceptance",
        ));
    }
    let work = context.work.as_mut().ok_or(Error::Poisoned)?;
    work.visits_remaining = work
        .visits_remaining
        .checked_sub(1)
        .ok_or(Error::ResourceLimited("candidate Domain promotion work"))?;
    let charge = Allocator {
        memory: context.memory,
        class: context.class,
    }
    .reserve(std::mem::size_of::<VerifiedDomainEventOccurrenceV1<'a>>())?;
    if (context.cancelled)() {
        return Err(Error::Cancelled);
    }
    context.poisoned = false;
    Ok(charge.retain(VerifiedDomainEventOccurrenceV1 {
        event: checked.event,
        occurrence: raw,
    }))
}
