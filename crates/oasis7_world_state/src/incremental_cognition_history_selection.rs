//! Candidate-only selection of the unique storage-view Result. This is not a
//! History closure: detached indexes, heads and originals remain to be checked.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    VerifiedCandidateContextV1, VerifiedFormalCommitV1,
};
use oasis7_formal_protocol::incremental_material::{
    CandidateFactRequestV1, CheckedCandidateFactOccurrenceV1, CheckedCommitCore,
    CheckedFactOriginals, CoreError, FactBudget, OriginalObjectStreamReader,
    VerifiedFactOccurrenceV1, check_candidate_fact_v1, promote_candidate_fact_v1,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    convert::Infallible,
    io::{self, Write},
};

#[derive(Debug)]
pub enum CognitionHistorySelectionErrorV1<E> {
    Value(ValueTreeError),
    Original(E),
    Formal(CoreError<Infallible>),
    Provenance(CoreError<E>),
}
#[path = "incremental_cognition_history_selected_read.rs"]
mod selected_read;
pub use selected_read::recover_selected_cognition_history_view_v1;
impl<E> From<ValueTreeError> for CognitionHistorySelectionErrorV1<E> {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
pub struct CognitionHistorySelectionRequestV1<'a> {
    pub candidate: &'a VerifiedCandidateContextV1,
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub originals: &'a ReservedValue<CheckedFactOriginals>,
}
/// Private candidate binding only; no public constructor or accepted conversion.
pub struct CheckedCognitionHistoryViewV1 {
    manifest: ReservedValue<CognitionHistoryViewV1>,
    derived: Option<CognitionHistoryDerivedRefsV2>,
    occurrence: ReservedValue<CheckedCandidateFactOccurrenceV1>,
}
impl CheckedCognitionHistoryViewV1 {
    pub fn derived(&self) -> Option<&CognitionHistoryDerivedRefsV2> {
        self.derived.as_ref()
    }
    pub fn manifest(&self) -> &CognitionHistoryViewV1 {
        &self.manifest
    }
    pub fn occurrence(&self) -> &ReservedValue<CheckedCandidateFactOccurrenceV1> {
        &self.occurrence
    }
}
/// The unique candidate selection promoted under its exact accepted signature.
/// This is selection provenance only, not index/original semantic closure.
pub struct AcceptedCognitionHistoryViewV1 {
    manifest: ReservedValue<CognitionHistoryViewV1>,
    derived: Option<CognitionHistoryDerivedRefsV2>,
    occurrence: ReservedValue<VerifiedFactOccurrenceV1>,
    class: MemoryClass,
}
impl AcceptedCognitionHistoryViewV1 {
    pub(crate) fn class(&self) -> MemoryClass {
        self.class
    }
    pub fn derived(&self) -> Option<&CognitionHistoryDerivedRefsV2> {
        self.derived.as_ref()
    }
    pub fn manifest(&self) -> &CognitionHistoryViewV1 {
        &self.manifest
    }
    pub fn occurrence(&self) -> &ReservedValue<VerifiedFactOccurrenceV1> {
        &self.occurrence
    }
}
/// Re-admit only fixed selection metadata. Formal owns the occurrence copy;
/// this neither reselects a head nor checks historical page availability.
pub fn readmit_cognition_history_view_ordinary_v1(
    source: &ReservedValue<AcceptedCognitionHistoryViewV1>,
    head: &VerifiedFormalCommitV1,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<
    ReservedValue<AcceptedCognitionHistoryViewV1>,
    CognitionHistorySelectionErrorV1<Infallible>,
> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !source.uses_budget(memory)
        || !source.manifest.uses_budget(memory)
        || source.class != MemoryClass::Recovery
        || source.occurrence.allocation_class() != MemoryClass::Recovery
    {
        return Err(ValueTreeError::Invalid("Cognition History view readmission source").into());
    }
    let charge = memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let manifest_charge = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let occurrence =
        oasis7_formal_protocol::incremental_material::readmit_fact_occurrence_ordinary_v1(
            &source.occurrence,
            head,
            roots,
            memory,
            &mut cancelled,
        )
        .map_err(CognitionHistorySelectionErrorV1::Formal)?;
    if occurrence.roots_id() != source.manifest.roots
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(ValueTreeError::Invalid("Cognition History view readmission binding").into());
    }
    let manifest = manifest_charge.retain((*source.manifest).clone());
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(AcceptedCognitionHistoryViewV1 {
        manifest,
        derived: source.derived.clone(),
        occurrence,
        class: MemoryClass::Ordinary,
    }))
}
pub fn promote_cognition_history_view_v1(
    checked: &ReservedValue<CheckedCognitionHistoryViewV1>,
    accepted: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<
    ReservedValue<AcceptedCognitionHistoryViewV1>,
    CognitionHistorySelectionErrorV1<Infallible>,
> {
    let charge = memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !checked.uses_budget(memory) || !checked.manifest.uses_budget(memory) {
        return Err(ValueTreeError::Invalid("Cognition History promotion pool").into());
    }
    let occurrence =
        promote_candidate_fact_v1(&checked.occurrence, accepted, memory, &mut cancelled)
            .map_err(CognitionHistorySelectionErrorV1::Formal)?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.roots_id() != checked.manifest.roots
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(ValueTreeError::Invalid("Cognition History accepted selection").into());
    }
    // Four fixed StateRoot digest strings and fixed manifest metadata only.
    let manifest = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?
        .retain((*checked.manifest).clone());
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(AcceptedCognitionHistoryViewV1 {
        class: MemoryClass::Ordinary,
        manifest,
        derived: checked.derived.clone(),
        occurrence,
    }))
}
pub fn check_candidate_cognition_history_view_v1<O: OriginalObjectStreamReader>(
    request: &CognitionHistorySelectionRequestV1<'_>,
    objects: &mut O,
    memory: &MemoryBudget,
    budget: &mut FactBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedCognitionHistoryViewV1>, CognitionHistorySelectionErrorV1<O::Error>>
{
    let charge = memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !request.core.uses_budget(memory)
        || !request.originals.uses_budget(memory)
        || !objects.allocation_budget().same_pool(memory)
        || objects.allocation_class() != MemoryClass::Ordinary
    {
        return Err(ValueTreeError::Invalid("Cognition History selection pool").into());
    }
    let roots = request
        .core
        .roots()
        .digest()
        .map_err(|_| ValueTreeError::Invalid("Cognition History selected roots"))?;
    let mut selected = None;
    for (ordinal, fact) in request.core.facts().facts.iter().enumerate() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if fact.kind != ExecutionFactKindV1::Result {
            continue;
        }
        budget.facts_remaining =
            budget
                .facts_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Cognition History Result probes",
                ))?;
        let limit = budget.max_fact_bytes.min(budget.original_bytes_remaining);
        let mut probe = Probe {
            prefix: [0; COGNITION_HISTORY_VIEW_V2_BYTES],
            copied: 0,
            bytes: 0,
            hash: blake3::Hasher::new(),
            limit,
            remaining: &mut budget.original_bytes_remaining,
            cancelled: &mut cancelled,
            stopped: None,
        };
        let observed = objects.stream_original(&fact.id, limit, memory, &mut probe);
        if let Some(error) = probe.stopped.take() {
            return Err(error.into());
        }
        let observed = observed.map_err(CognitionHistorySelectionErrorV1::Original)?;
        if observed != probe.bytes
            || observed > limit
            || probe.hash.finalize().as_bytes() != &fact.id
        {
            return Err(
                ValueTreeError::Invalid("Cognition History Result original identity").into(),
            );
        }
        if probe.copied < COGNITION_HISTORY_VIEW_MAGIC.len()
            || !(probe.prefix.starts_with(COGNITION_HISTORY_VIEW_MAGIC)
                || probe.prefix.starts_with(COGNITION_HISTORY_VIEW_V2_MAGIC))
        {
            continue;
        }
        if selected.is_some() {
            return Err(
                ValueTreeError::Invalid("duplicate Cognition History view manifest").into(),
            );
        }
        if observed > COGNITION_HISTORY_VIEW_V2_BYTES as u64 {
            return Err(
                ValueTreeError::Invalid("Cognition History manifest original length").into(),
            );
        }
        let (manifest, derived) = super::manifest_v2::decode_profile(
            &probe.prefix[..observed as usize],
            memory,
            MemoryClass::Ordinary,
            &mut cancelled,
        )?;
        if manifest.roots != roots {
            return Err(
                ValueTreeError::Invalid("Cognition History manifest RootSet binding").into(),
            );
        }
        let occurrence = check_candidate_fact_v1(
            &CandidateFactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                candidate: request.candidate,
                core: request.core,
                originals: request.originals,
                ordinal: u64::try_from(ordinal)
                    .map_err(|_| ValueTreeError::Invalid("Cognition History Result ordinal"))?,
            },
            memory,
            &mut cancelled,
        )
        .map_err(CognitionHistorySelectionErrorV1::Formal)?;
        if occurrence.location().original != fact.id
            || occurrence.original_bytes() != observed
            || occurrence.roots_id() != roots
        {
            return Err(ValueTreeError::Invalid("Cognition History manifest occurrence").into());
        }
        selected = Some(CheckedCognitionHistoryViewV1 {
            manifest,
            derived,
            occurrence,
        });
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(selected.ok_or(ValueTreeError::Invalid(
        "missing Cognition History view manifest",
    ))?))
}
struct Probe<'a, C> {
    prefix: [u8; COGNITION_HISTORY_VIEW_V2_BYTES],
    copied: usize,
    bytes: u64,
    hash: blake3::Hasher,
    limit: u64,
    remaining: &'a mut u64,
    cancelled: &'a mut C,
    stopped: Option<ValueTreeError>,
}
impl<C: FnMut() -> bool> Write for Probe<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.stopped.is_some() {
            return Err(io::Error::other("Cognition History probe stopped"));
        }
        self.stopped = Some(ValueTreeError::Invalid(
            "Cognition History interrupted probe",
        ));
        if (self.cancelled)() {
            self.stopped = Some(ValueTreeError::Cancelled);
            return Err(io::Error::other("Cognition History probe cancelled"));
        }
        // The public source can pass one arbitrarily large borrowed slice.
        // Return a short write so its write_all loop polls between chunks.
        let bytes = &bytes[..bytes
            .len()
            .min(oasis7_formal_protocol::incremental_value::VALUE_CHUNK_BYTES)];
        if bytes.len() as u64 > self.limit.saturating_sub(self.bytes)
            || bytes.len() as u64 > *self.remaining
        {
            self.stopped = Some(ValueTreeError::ResourceLimited(
                "Cognition History Result bytes",
            ));
            return Err(io::Error::other("Cognition History probe allowance"));
        }
        *self.remaining -= bytes.len() as u64;
        let take = bytes.len().min(self.prefix.len() - self.copied);
        self.prefix[self.copied..self.copied + take].copy_from_slice(&bytes[..take]);
        self.copied += take;
        self.bytes += bytes.len() as u64;
        self.hash.update(bytes);
        if (self.cancelled)() {
            self.stopped = Some(ValueTreeError::Cancelled);
            return Err(io::Error::other("Cognition History probe cancelled"));
        }
        self.stopped = None;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.stopped.is_some() {
            return Err(io::Error::other("Cognition History probe stopped"));
        }
        self.stopped = Some(ValueTreeError::Invalid(
            "Cognition History interrupted flush",
        ));
        if (self.cancelled)() {
            self.stopped = Some(ValueTreeError::Cancelled);
            return Err(io::Error::other("Cognition History probe cancelled"));
        }
        self.stopped = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_formal_protocol::incremental_value::VALUE_CHUNK_BYTES;
    use std::cell::Cell;
    #[test]
    fn a_single_large_source_slice_is_chunked_and_swallowed_errors_remain_sticky() {
        let input_memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let _input = input_memory
            .reserve(32 << 20, MemoryClass::Ordinary)
            .unwrap();
        let input = vec![0x35; 32 << 20];
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let _scratch = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
        let polls = Cell::new(0);
        let mut cancelled = || {
            polls.set(polls.get() + 1);
            polls.get() >= 2
        };
        let mut remaining = 64 << 20;
        let mut probe = Probe {
            prefix: [0; COGNITION_HISTORY_VIEW_V2_BYTES],
            copied: 0,
            bytes: 0,
            hash: blake3::Hasher::new(),
            limit: 64 << 20,
            remaining: &mut remaining,
            cancelled: &mut cancelled,
            stopped: None,
        };
        // This is one borrowed source call, not artificially pre-chunked IO.
        let result = probe.write_all(&input);
        assert!(result.is_err());
        assert_eq!(polls.get(), 2);
        assert_eq!(probe.bytes, VALUE_CHUNK_BYTES as u64);
        assert_eq!(*probe.remaining, (64 << 20) - VALUE_CHUNK_BYTES as u64);
        assert_eq!(
            probe.hash.finalize(),
            blake3::hash(&input[..VALUE_CHUNK_BYTES])
        );
        // A source may catch Write errors and report success. It must not
        // revive the probe or use flush to erase the recorded failure.
        assert!(probe.flush().is_err());
        assert!(probe.write_all(b"retry").is_err());
        assert!(matches!(probe.stopped, Some(ValueTreeError::Cancelled)));
        assert_eq!(probe.bytes, VALUE_CHUNK_BYTES as u64);
    }
}
