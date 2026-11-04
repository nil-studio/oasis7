//! Kind-bound raw Fact provenance. No content semantics, business grant or durability.
use super::*;
type Error = CoreError<std::convert::Infallible>;
pub type FactLocationV1 = EventFactLocationV1;
pub type FactReadContextV1<'a, O, R, C> = EventFactReadContextV1<'a, O, R, C>;
pub type FactHistoryExtensionRequestV1<'a> = EventHistoryExtensionRequestV1<'a>;
pub type VerifiedFactHistoryExtensionV1 = VerifiedEventHistoryExtensionV1;

pub struct FactRequestV1<'a> {
    pub kind: ExecutionFactKindV1,
    pub head: &'a VerifiedFormalCommitV1,
    pub roots: &'a RootSetV1,
    pub location: FactLocationV1,
}
pub struct CandidateFactRequestV1<'a> {
    pub kind: ExecutionFactKindV1,
    pub candidate: &'a VerifiedCandidateContextV1,
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub originals: &'a ReservedValue<CheckedFactOriginals>,
    pub ordinal: u64,
}
pub struct CurrentFactRequestV1<'a> {
    pub kind: ExecutionFactKindV1,
    pub head: &'a VerifiedFormalCommitV1,
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub originals: &'a ReservedValue<CheckedFactOriginals>,
    pub ordinal: u64,
}
#[derive(Debug)]
pub struct VerifiedFactOccurrenceV1 {
    binding: FactBinding,
}
#[derive(Debug)]
pub struct CheckedCandidateFactOccurrenceV1 {
    binding: FactBinding,
}
impl VerifiedFactOccurrenceV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.binding.class
    }
    pub fn kind(&self) -> ExecutionFactKindV1 {
        self.binding.kind
    }
    pub fn head_record_id(&self) -> [u8; 32] {
        self.binding.record
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.binding.roots
    }
    pub fn head_body_id(&self) -> [u8; 32] {
        self.binding.head_body
    }
    pub fn fact_body_id(&self) -> [u8; 32] {
        self.binding.fact_body
    }
    pub fn location(&self) -> FactLocationV1 {
        self.binding.location
    }
    pub fn original_bytes(&self) -> u64 {
        self.binding.original_bytes
    }
}
impl CheckedCandidateFactOccurrenceV1 {
    pub fn kind(&self) -> ExecutionFactKindV1 {
        self.binding.kind
    }
    pub fn candidate_record_id(&self) -> [u8; 32] {
        self.binding.record
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.binding.roots
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.binding.head_body
    }
    pub fn location(&self) -> FactLocationV1 {
        self.binding.location
    }
    pub fn original_bytes(&self) -> u64 {
        self.binding.original_bytes
    }
}
fn current_binding(kind: ExecutionFactKindV1, value: CurrentEventFactBinding) -> FactBinding {
    FactBinding {
        record: value.record,
        roots: value.roots,
        head_body: value.body,
        fact_body: value.body,
        kind,
        location: value.location,
        original_bytes: value.original_bytes,
        class: MemoryClass::Ordinary,
    }
}
/// Requires the exact authenticated ordinal kind and its actual observed bytes.
pub fn check_candidate_fact_v1(
    request: &CandidateFactRequestV1<'_>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedCandidateFactOccurrenceV1>, Error> {
    let (mut workspace, value) = checked_current_fact_binding(
        request.candidate.record(),
        request.core,
        request.originals,
        request.ordinal,
        request.kind,
        memory,
        &mut cancelled,
    )?;
    let charge = workspace
        .split(std::mem::size_of::<CheckedCandidateFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("candidate fact accounting"))?;
    Ok(charge.retain(CheckedCandidateFactOccurrenceV1 {
        binding: current_binding(request.kind, value),
    }))
}
pub fn bind_checked_current_fact_v1(
    request: &CurrentFactRequestV1<'_>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedFactOccurrenceV1>, Error> {
    let (mut workspace, value) = checked_current_fact_binding(
        request.head.record(),
        request.core,
        request.originals,
        request.ordinal,
        request.kind,
        memory,
        &mut cancelled,
    )?;
    let charge = workspace
        .split(std::mem::size_of::<VerifiedFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("current fact accounting"))?;
    Ok(charge.retain(VerifiedFactOccurrenceV1 {
        binding: current_binding(request.kind, value),
    }))
}
/// Prepaid output admission only; this is not an accepted fact or commit.
pub struct PreparedFactPromotionV1 {
    binding: FactBinding,
    charge: oasis7_proto::memory_budget::MemoryReservation,
}
/// Must run before formal acceptance while Ordinary admission can still fail.
pub fn prepare_candidate_fact_promotion_v1(
    checked: &ReservedValue<CheckedCandidateFactOccurrenceV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedFactPromotionV1, Error> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.binding.class != MemoryClass::Ordinary
        || checked.reserved_bytes() < std::mem::size_of::<CheckedCandidateFactOccurrenceV1>()
    {
        return Err(CoreError::Invalid("candidate fact pool/charge"));
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<VerifiedFactOccurrenceV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| CoreError::ResourceLimited("fact promotion admission"))?;
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(PreparedFactPromotionV1 {
        binding: checked.binding,
        charge,
    })
}
/// Fixed-field refinement after genuine formal acceptance. No admission, IO,
/// cancellation, dynamic encoding or digest computation occurs here.
pub fn finish_prepared_fact_promotion_v1(
    prepared: PreparedFactPromotionV1,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<VerifiedFactOccurrenceV1>, Error> {
    let record = accepted.verified_record_id();
    if record != prepared.binding.record {
        return Err(CoreError::Invalid("candidate fact accepted head"));
    }
    Ok(prepared.charge.retain(VerifiedFactOccurrenceV1 {
        binding: prepared.binding,
    }))
}
pub fn promote_candidate_fact_v1(
    checked: &ReservedValue<CheckedCandidateFactOccurrenceV1>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedFactOccurrenceV1>, Error> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.binding.class != MemoryClass::Ordinary
        || checked.reserved_bytes() < std::mem::size_of::<CheckedCandidateFactOccurrenceV1>()
    {
        return Err(CoreError::Invalid("candidate fact pool/charge"));
    }
    let (mut workspace, record) = candidate::candidate_promotion_binding(
        checked.binding.record,
        accepted,
        memory,
        &mut cancelled,
    )?;
    let charge = workspace
        .split(std::mem::size_of::<VerifiedFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("fact promotion accounting"))?;
    Ok(charge.retain(VerifiedFactOccurrenceV1 {
        binding: FactBinding {
            record,
            ..checked.binding
        },
    }))
}
pub fn verify_fact_occurrence_v1<
    O: OriginalObjectStreamReader,
    R: MerkleNodeReader,
    C: FnMut() -> bool,
>(
    request: &FactRequestV1<'_>,
    context: &mut FactReadContextV1<'_, O, R, C>,
) -> Result<ReservedValue<VerifiedFactOccurrenceV1>, CoreError<O::Error>> {
    let (mut workspace, binding) = verify_fact_binding(
        &EventFactRequestV1 {
            head: request.head,
            roots: request.roots,
            location: request.location,
        },
        request.kind,
        context,
    )?;
    let charge = workspace
        .split(std::mem::size_of::<VerifiedFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("fact history accounting"))?;
    Ok(charge.retain(VerifiedFactOccurrenceV1 { binding }))
}
pub fn verify_fact_history_extension_v1(
    request: &FactHistoryExtensionRequestV1<'_>,
    memory: &MemoryBudget,
    cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedFactHistoryExtensionV1>, Error> {
    verify_event_history_extension_v1(request, memory, cancelled)
}
pub fn inherit_fact_occurrence_v1(
    occurrence: &ReservedValue<VerifiedFactOccurrenceV1>,
    extension: &ReservedValue<VerifiedFactHistoryExtensionV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedFactOccurrenceV1>, Error> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if !occurrence.uses_budget(memory)
        || occurrence.binding.class != MemoryClass::Ordinary
        || !extension.uses_budget(memory)
        || occurrence.reserved_bytes() < std::mem::size_of::<VerifiedFactOccurrenceV1>()
        || extension.reserved_bytes() < std::mem::size_of::<VerifiedFactHistoryExtensionV1>()
    {
        return Err(CoreError::Invalid("fact inheritance pool/charge"));
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<VerifiedFactOccurrenceV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ExecutionError::Memory)?;
    let binding = extension::inherit_fact_binding(occurrence.binding, extension, &mut cancelled)?;
    Ok(charge.retain(VerifiedFactOccurrenceV1 { binding }))
}

/// Copy only authenticated fixed metadata into newly admitted Ordinary credit.
/// Exact accepted head/RootSet must be unchanged. No provenance is re-created
/// from public getters, and no original/catalog/history IO is performed.
pub fn readmit_fact_occurrence_ordinary_v1(
    occurrence: &ReservedValue<VerifiedFactOccurrenceV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedFactOccurrenceV1>, Error> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if !occurrence.uses_budget(memory)
        || occurrence.reserved_bytes() < std::mem::size_of::<VerifiedFactOccurrenceV1>()
    {
        return Err(CoreError::Invalid("fact readmission pool/charge"));
    }
    let (mut workspace, binding) =
        readmit_fact_binding_ordinary(occurrence.binding, head, roots, memory, &mut cancelled)?;
    let charge = workspace
        .split(std::mem::size_of::<VerifiedFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("fact readmission accounting"))?;
    Ok(charge.retain(VerifiedFactOccurrenceV1 { binding }))
}
