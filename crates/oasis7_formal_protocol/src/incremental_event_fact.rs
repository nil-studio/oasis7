//! Authenticated historical Event occurrences. This is raw provenance, not a
//! WorldEvent decoder, complete execution closure, authority grant or GC root.
use super::*;

#[path = "incremental_event_history_extension.rs"]
mod extension;
pub use extension::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventFactLocationV1 {
    pub height: u64,
    pub ordinal: u64,
    pub original: [u8; 32],
}
pub struct EventFactRequestV1<'a> {
    pub head: &'a VerifiedFormalCommitV1,
    pub roots: &'a RootSetV1,
    pub location: EventFactLocationV1,
}
pub struct EventFactReadContextV1<'a, O, R, C> {
    pub originals: &'a mut O,
    pub catalog_pages: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub core_budget: &'a mut CoreBudget,
    pub fact_budget: &'a mut FactBudget,
    pub cancelled: C,
}
/// Private construction; no caller-supplied success flag or original bytes can
/// mint this proof. Its exact accepted head must accompany any later consumer.
#[derive(Debug)]
pub struct VerifiedEventFactOccurrenceV1 {
    head_record: [u8; 32],
    roots: [u8; 32],
    head_body: [u8; 32],
    event_body: [u8; 32],
    location: EventFactLocationV1,
    original_bytes: u64,
}
impl VerifiedEventFactOccurrenceV1 {
    pub fn head_record_id(&self) -> [u8; 32] {
        self.head_record
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn head_body_id(&self) -> [u8; 32] {
        self.head_body
    }
    pub fn event_body_id(&self) -> [u8; 32] {
        self.event_body
    }
    pub fn location(&self) -> EventFactLocationV1 {
        self.location
    }
    pub fn original_bytes(&self) -> u64 {
        self.original_bytes
    }
}

fn event_head_binding<E>(
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
) -> Result<([u8; 32], [u8; 32]), CoreError<E>> {
    let record = head.record();
    record.validate().map_err(BindingError::from)?;
    if !incremental::is_iws_record(record.schema_version) {
        return Err(CoreError::Invalid(
            "event history requires IWS accepted head",
        ));
    }
    let roots_id = roots.digest()?;
    if role(record, ArtifactRole::IncrementalRootSet)? != roots_id
        || record.after_state_root != hex::encode(roots_id)
        || record.world.world_id != roots.world_id
        || record.world.genesis_digest != hex::encode(roots.genesis_config)
        || roots.height.checked_add(1) != Some(record.position)
    {
        return Err(CoreError::Invalid("event history head/root binding"));
    }
    Ok((roots_id, role(record, ArtifactRole::IncrementalCommitBody)?))
}

pub struct CurrentEventFactRequestV1<'a> {
    pub head: &'a VerifiedFormalCommitV1,
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub originals: &'a ReservedValue<CheckedFactOriginals>,
    pub ordinal: u64,
}

/// Bind an actually observed current occurrence to its exact accepted head.
/// No original IO is repeated: the private observation was minted only after
/// stream/hash/length verification of every occurrence in this same FactSet.
/// Neither prepared content alone nor a caller-reported length is sufficient.
pub fn bind_checked_current_event_fact_v1(
    request: &CurrentEventFactRequestV1<'_>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<VerifiedEventFactOccurrenceV1>, CoreError<std::convert::Infallible>> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if !request.core.uses_budget(memory)
        || !request.originals.uses_budget(memory)
        || !request.core.facts.uses_budget(memory)
        || request.core.reserved_bytes() < std::mem::size_of::<CheckedCommitCore>()
        || request.originals.reserved_bytes() < std::mem::size_of::<CheckedFactOriginals>()
    {
        return Err(CoreError::Invalid("current event observation pool/charge"));
    }
    let mut workspace = memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    let core = &request.core;
    let (roots_id, body_id) = event_head_binding(request.head, &core.roots)?;
    let record = request.head.record();
    if core.body.digest()? != body_id
        || core.body.roots != roots_id
        || !core.body.scope.matches(&core.roots)
        || core.body.height != core.roots.height
        || hex::encode(core.body.operation) != record.request_digest
        || core.body.history != role(record, ArtifactRole::IncrementalHistoryPrefix)?
        || request.originals.body_id() != body_id
        || request.originals.fact_set_id() != core.body.facts
    {
        return Err(CoreError::Invalid(
            "current event observation head/body/facts",
        ));
    }
    let ordinal = usize::try_from(request.ordinal)
        .map_err(|_| CoreError::Invalid("current event ordinal overflow"))?;
    let fact = core
        .facts
        .facts
        .get(ordinal)
        .filter(|fact| fact.kind == ExecutionFactKindV1::Event)
        .ok_or(CoreError::Invalid("current event fact occurrence"))?;
    let observed = request
        .originals
        .checked_occurrence(ordinal, memory)
        .map_err(CoreError::Invalid)?
        .ok_or(CoreError::Invalid("current event observation missing"))?;
    if observed.kind != fact.kind || observed.id != fact.id {
        return Err(CoreError::Invalid("current event observed identity"));
    }
    let head_record = crate::incremental_history::wire::parse_hash(
        &record.record_id().map_err(BindingError::from)?,
    )?;
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    let charge = workspace
        .split(std::mem::size_of::<VerifiedEventFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("current event summary accounting"))?;
    Ok(charge.retain(VerifiedEventFactOccurrenceV1 {
        head_record,
        roots: roots_id,
        head_body: body_id,
        event_body: body_id,
        location: EventFactLocationV1 {
            height: core.roots.height,
            ordinal: request.ordinal,
            original: fact.id,
        },
        original_bytes: observed.bytes,
    }))
}

/// The height/ordinal is only an untrusted lookup hint. In-prefix missing
/// catalog content is corruption, never authenticated absence. This does not
/// decode WorldEvent or assume that all Event bytes have that particular type.
pub fn verify_event_fact_occurrence_v1<
    O: OriginalObjectStreamReader,
    R: MerkleNodeReader,
    C: FnMut() -> bool,
>(
    request: &EventFactRequestV1<'_>,
    context: &mut EventFactReadContextV1<'_, O, R, C>,
) -> Result<ReservedValue<VerifiedEventFactOccurrenceV1>, CoreError<O::Error>> {
    let EventFactReadContextV1 {
        originals,
        catalog_pages,
        memory,
        class,
        core_budget,
        fact_budget,
        cancelled,
    } = context;
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if originals.allocation_class() != *class
        || !originals.allocation_budget().same_pool(memory)
        || catalog_pages.allocation_class() != Some(*class)
        || catalog_pages
            .allocation_budget()
            .is_none_or(|p| !p.same_pool(memory))
    {
        return Err(CoreError::Invalid("event history source pool/class"));
    }
    // Covers the bounded header decoders, catalog path metadata and summary.
    let mut workspace = memory
        .reserve(128 * 1024, *class)
        .map_err(ExecutionError::Memory)?;
    let record = request.head.record();
    let (roots_id, head_body) = event_head_binding(request.head, request.roots)?;
    if request.location.height > request.roots.height {
        return Err(CoreError::Invalid("event history head/root binding"));
    }
    let body = CommitBodyV1::from_canonical_bytes(&load(
        *originals,
        &head_body,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        core_budget,
        cancelled,
    )?)?;
    if !body.scope.matches(request.roots)
        || body.height != request.roots.height
        || body.roots != roots_id
        || hex::encode(body.operation) != record.request_digest
        || body.history != role(record, ArtifactRole::IncrementalHistoryPrefix)?
    {
        return Err(CoreError::Invalid("event history accepted body binding"));
    }
    let (event_body, body) = if request.location.height == body.height {
        (head_body, body)
    } else {
        let entry = with_pages(*catalog_pages, core_budget, cancelled, |source, _| {
            body.catalog_before
                .lookup(CatalogQueryV1::Height(request.location.height), source)
                .map_err(CoreError::Execution)
        })?
        .ok_or(CoreError::Invalid(
            "event history height absent from accepted prefix",
        ))?;
        let old = CommitBodyV1::from_canonical_bytes(&load(
            *originals,
            &entry.commit,
            HISTORY_MAX_HEADER_BYTES,
            memory,
            core_budget,
            cancelled,
        )?)?;
        if old.scope != body.scope
            || old.height != entry.height
            || old.parent_commit != entry.parent_commit
        {
            return Err(CoreError::Invalid("event history catalog/body binding"));
        }
        (entry.commit, old)
    };
    let prefix = HistoryPrefixV1::from_canonical_bytes(&load(
        *originals,
        &body.history,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        core_budget,
        cancelled,
    )?)?;
    let segment = HistorySegmentV1::from_canonical_bytes(&load(
        *originals,
        &prefix.segment,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        core_budget,
        cancelled,
    )?)?;
    if prefix.scope != body.scope
        || prefix.count
            != body
                .height
                .checked_add(1)
                .ok_or(CoreError::Invalid("event history height overflow"))?
        || segment.scope != body.scope
        || segment.height != body.height
        || segment.parent_commit != body.parent_commit
        || segment.operation != body.operation
        || segment.after_roots != body.roots
        || segment.transition != body.transition
        || segment.facts != body.facts
    {
        return Err(CoreError::Invalid("event history prefix/segment binding"));
    }
    let facts = FactSetV1::from_canonical_bytes_in_class(
        &load(
            *originals,
            &body.facts,
            256 * 1024,
            memory,
            core_budget,
            cancelled,
        )?,
        memory,
        *class,
    )?;
    let ordinal = usize::try_from(request.location.ordinal)
        .map_err(|_| CoreError::Invalid("event fact ordinal overflow"))?;
    if facts.scope != body.scope
        || facts.operation != body.operation
        || facts.facts.get(ordinal).is_none_or(|fact| {
            fact.kind != ExecutionFactKindV1::Event || fact.id != request.location.original
        })
    {
        return Err(CoreError::Invalid("event fact occurrence binding"));
    }
    if fact_budget.facts_remaining == 0 {
        return Err(CoreError::ResourceLimited(
            "event fact occurrence allowance",
        ));
    }
    fact_budget.facts_remaining -= 1;
    let admitted = fact_budget
        .max_fact_bytes
        .min(fact_budget.original_bytes_remaining);
    fact_budget.original_bytes_remaining -= admitted;
    let bytes = fact_material::verify_original_stream_observed(
        *originals,
        &request.location.original,
        admitted,
        memory,
        *class,
        cancelled,
        |_| {},
    )?;
    fact_budget.original_bytes_remaining += admitted - bytes;
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    let head_record = crate::incremental_history::wire::parse_hash(
        &record.record_id().map_err(BindingError::from)?,
    )?;
    let charge = workspace
        .split(std::mem::size_of::<VerifiedEventFactOccurrenceV1>())
        .map_err(|_| CoreError::Invalid("event history summary accounting"))?;
    Ok(charge.retain(VerifiedEventFactOccurrenceV1 {
        head_record,
        roots: roots_id,
        head_body,
        event_body,
        location: request.location,
        original_bytes: bytes,
    }))
}
