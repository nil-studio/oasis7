//! Closed routing of every Result in the actual accepted head's current FactSet.
//! No historical FactSet scan, caller-made Core, or accepted permission mint.
use crate::incremental_world_result_router::{
    WorldResultProfileV1, WorldResultRouterErrorV1, WorldResultRouterWorkV1,
    classify_world_result_original_v1, read_world_router_original_v1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{EXECUTION_MAX_FACTS, ExecutionError, ExecutionFactKindV1, RootSetV1},
    incremental_history::{CommitBodyV1, FactSetV1, HISTORY_MAX_HEADER_BYTES},
    incremental_material::{
        CoreBudget, CoreError, FactLocationV1, FactReadContextV1, FactRequestV1,
        OriginalObjectStreamReader, VerifiedFactOccurrenceV1, verify_fact_occurrence_v1,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub struct AcceptedWorldResultRouterRequestV1<'a> {
    pub head: &'a VerifiedFormalCommitV1,
    pub roots: &'a RootSetV1,
    /// Already recovered actual current Runtime selected Result occurrence.
    pub anchor: &'a ReservedValue<VerifiedFactOccurrenceV1>,
}
pub struct AcceptedRoutedWorldResultV1 {
    occurrence: ReservedValue<VerifiedFactOccurrenceV1>,
    profile: WorldResultProfileV1,
}
impl AcceptedRoutedWorldResultV1 {
    pub fn occurrence(&self) -> &ReservedValue<VerifiedFactOccurrenceV1> {
        &self.occurrence
    }
    pub fn profile(&self) -> WorldResultProfileV1 {
        self.profile
    }
}
pub struct AcceptedWorldResultRoutingV1 {
    class: MemoryClass,
    rows: ReservedValue<Vec<AcceptedRoutedWorldResultV1>>,
    body: ReservedValue<CommitBodyV1>,
    record: [u8; 32],
    body_id: [u8; 32],
    roots: [u8; 32],
    cognition: usize,
    runtime: usize,
}
impl AcceptedWorldResultRoutingV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn rows(&self) -> &[AcceptedRoutedWorldResultV1] {
        &self.rows
    }
    pub fn cognition(&self) -> &AcceptedRoutedWorldResultV1 {
        &self.rows[self.cognition]
    }
    pub fn runtime_history(&self) -> &AcceptedRoutedWorldResultV1 {
        &self.rows[self.runtime]
    }
    pub fn body(&self) -> &CommitBodyV1 {
        &self.body
    }
    pub fn record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.body_id
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
}
#[derive(Debug)]
pub enum AcceptedWorldResultRouterErrorV1<E> {
    Value(ValueTreeError),
    Original(E),
    Formal(CoreError<E>),
    Execution(ExecutionError),
}
impl<E> From<ValueTreeError> for AcceptedWorldResultRouterErrorV1<E> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl<E> From<ExecutionError> for AcceptedWorldResultRouterErrorV1<E> {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("accepted World Result dispatch")
}
fn stream_error<E>(e: WorldResultRouterErrorV1<E>) -> AcceptedWorldResultRouterErrorV1<E> {
    match e {
        WorldResultRouterErrorV1::Value(e) => AcceptedWorldResultRouterErrorV1::Value(e),
        WorldResultRouterErrorV1::Original(e) => AcceptedWorldResultRouterErrorV1::Original(e),
        WorldResultRouterErrorV1::Formal(_) => AcceptedWorldResultRouterErrorV1::Value(invalid()),
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Real accepted head, actual original/catalog IO and continuous budgets remain explicit."
)]
pub fn route_accepted_world_results_v1<O, R, C>(
    request: &AcceptedWorldResultRouterRequestV1<'_>,
    objects: &mut O,
    catalog_pages: &R,
    memory: &MemoryBudget,
    class: MemoryClass,
    core_budget: &mut CoreBudget,
    work: &mut WorldResultRouterWorkV1,
    mut cancelled: C,
) -> Result<ReservedValue<AcceptedWorldResultRoutingV1>, AcceptedWorldResultRouterErrorV1<O::Error>>
where
    O: OriginalObjectStreamReader,
    R: MerkleNodeReader,
    C: FnMut() -> bool,
{
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let outer = memory
        .reserve(8192, class)
        .map_err(ValueTreeError::Memory)?;
    let roots = request.roots.digest()?;
    let record = request.head.verified_record_id();
    let anchor = request.anchor;
    if !anchor.uses_budget(memory)
        || anchor.allocation_class() != class
        || anchor.reserved_bytes() < std::mem::size_of::<VerifiedFactOccurrenceV1>()
        || anchor.kind() != ExecutionFactKindV1::Result
        || anchor.head_record_id() != record
        || anchor.roots_id() != roots
        || anchor.head_body_id() != anchor.fact_body_id()
        || anchor.location().height != request.roots.height
        || objects.allocation_class() != class
        || !objects.allocation_budget().same_pool(memory)
        || catalog_pages.allocation_class() != Some(class)
        || catalog_pages
            .allocation_budget()
            .is_none_or(|p| !p.same_pool(memory))
    {
        return Err(invalid().into());
    }
    let body_id = anchor.head_body_id();
    // Header decode can allocate bounded scope/catalog strings: admit the full
    // public header bound before decode, retaining a separate owned body phase.
    let body_charge = memory
        .reserve(
            HISTORY_MAX_HEADER_BYTES
                .checked_mul(8)
                .ok_or_else(invalid)?,
            class,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut load =
        |id: &[u8; 32], bound: usize, objects: &mut O| {
            core_budget.objects_remaining = core_budget.objects_remaining.checked_sub(1).ok_or(
                ValueTreeError::ResourceLimited("accepted router core objects"),
            )?;
            let bound = bound
                .min(usize::try_from(core_budget.original_bytes_remaining).unwrap_or(usize::MAX));
            if bound == 0 {
                return Err(AcceptedWorldResultRouterErrorV1::Value(
                    ValueTreeError::ResourceLimited("accepted router core bytes"),
                ));
            }
            read_world_router_original_v1(
                objects,
                id,
                bound,
                false,
                memory,
                class,
                &mut core_budget.original_bytes_remaining,
                &mut cancelled,
            )
            .map_err(stream_error)
        };
    let raw_body = load(&body_id, HISTORY_MAX_HEADER_BYTES, objects)?;
    let body = body_charge.retain(CommitBodyV1::from_canonical_bytes(&raw_body)?);
    drop(raw_body);
    if !body.scope.matches(request.roots)
        || body.height != request.roots.height
        || body.roots != roots
        || hex::encode(body.operation) != request.head.record().request_digest
    {
        return Err(invalid().into());
    }
    let raw_facts = load(&body.facts, 256 * 1024, objects)?;
    let facts = FactSetV1::from_canonical_bytes_in_class(&raw_facts, memory, class)?;
    drop(raw_facts);
    if facts.scope != body.scope
        || facts.operation != body.operation
        || facts.facts.len() > EXECUTION_MAX_FACTS
    {
        return Err(invalid().into());
    }
    let anchor_index = usize::try_from(anchor.location().ordinal).map_err(|_| invalid())?;
    if facts
        .facts
        .get(anchor_index)
        .is_none_or(|f| f.kind != ExecutionFactKindV1::Result || f.id != anchor.location().original)
    {
        return Err(invalid().into());
    }
    let count = facts
        .facts
        .iter()
        .filter(|f| f.kind == ExecutionFactKindV1::Result)
        .count();
    let rows_charge = memory
        .reserve(
            count
                .checked_mul(std::mem::size_of::<AcceptedRoutedWorldResultV1>())
                .and_then(|n| n.checked_add(1024))
                .ok_or_else(invalid)?,
            class,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut rows = Vec::with_capacity(count);
    let mut cognition = None;
    let mut runtime = None;
    for (ordinal, fact) in facts.facts.iter().enumerate() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if fact.kind != ExecutionFactKindV1::Result {
            continue;
        }
        let occurrence = verify_fact_occurrence_v1(
            &FactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                head: request.head,
                roots: request.roots,
                location: FactLocationV1 {
                    height: body.height,
                    ordinal: ordinal as u64,
                    original: fact.id,
                },
            },
            &mut FactReadContextV1 {
                originals: objects,
                catalog_pages,
                memory,
                class,
                core_budget,
                fact_budget: &mut work.facts,
                cancelled: &mut cancelled,
            },
        )
        .map_err(AcceptedWorldResultRouterErrorV1::Formal)?;
        let length = occurrence.original_bytes();
        if length > work.facts.max_fact_bytes {
            return Err(ValueTreeError::ResourceLimited("accepted Result original limit").into());
        }
        let length = usize::try_from(length).map_err(|_| invalid())?;
        let raw = read_world_router_original_v1(
            objects,
            &fact.id,
            length,
            true,
            memory,
            class,
            &mut work.facts.original_bytes_remaining,
            &mut cancelled,
        )
        .map_err(stream_error)?;
        let profile = classify_world_result_original_v1(
            &raw,
            fact.id,
            roots,
            memory,
            class,
            work,
            &mut cancelled,
        )?;
        match profile {
            WorldResultProfileV1::CognitionHistoryV3 if cognition.replace(rows.len()).is_some() => {
                return Err(invalid().into());
            }
            WorldResultProfileV1::RuntimeHistoryV1 if runtime.replace(rows.len()).is_some() => {
                return Err(invalid().into());
            }
            _ => {}
        }
        if rows.len() >= count {
            return Err(invalid().into());
        }
        rows.push(AcceptedRoutedWorldResultV1 {
            occurrence,
            profile,
        });
    }
    let cognition = cognition.ok_or_else(invalid)?;
    let runtime = runtime.ok_or_else(invalid)?;
    if rows[runtime].occurrence.location() != anchor.location() {
        return Err(invalid().into());
    }
    if rows.len() != count {
        return Err(invalid().into());
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(outer.retain(AcceptedWorldResultRoutingV1 {
        class,
        rows: rows_charge.retain(rows),
        body,
        record,
        body_id,
        roots,
        cognition,
        runtime,
    }))
}

/// Explicit Recovery -> Ordinary admission. Every row is re-admitted through
/// the real Formal refinement; the body and vectors are independently cloned
/// under Ordinary charges. No Recovery Arc/carrier is retained.
pub fn readmit_accepted_world_result_routing_ordinary_v1<C: FnMut() -> bool>(
    source: &ReservedValue<AcceptedWorldResultRoutingV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: C,
) -> Result<
    ReservedValue<AcceptedWorldResultRoutingV1>,
    AcceptedWorldResultRouterErrorV1<std::convert::Infallible>,
> {
    use oasis7_formal_protocol::incremental_material::readmit_fact_occurrence_ordinary_v1;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let outer = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if !source.uses_budget(memory)
        || source.allocation_class() != MemoryClass::Recovery
        || source.reserved_bytes() < 8192
        || !source.rows.uses_budget(memory)
        || !source.body.uses_budget(memory)
        || source.record != head.verified_record_id()
        || source.roots != roots.digest()?
        || source.body.roots != source.roots
        || source.body.height != roots.height
        || !source.body.scope.matches(roots)
        || source.rows.len() > EXECUTION_MAX_FACTS
        || source.cognition >= source.rows.len()
        || source.runtime >= source.rows.len()
    {
        return Err(invalid().into());
    }
    let rows_charge = memory
        .reserve(
            source
                .rows
                .len()
                .checked_mul(std::mem::size_of::<AcceptedRoutedWorldResultV1>())
                .and_then(|n| n.checked_add(1024))
                .ok_or_else(invalid)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let body_charge = memory
        .reserve(HISTORY_MAX_HEADER_BYTES * 8, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    // Exhaustive fixed wire structure guard: future fields need a new owned
    // allocation/readmission audit, not an unnoticed deep Arc clone.
    let CommitBodyV1 {
        scope: _,
        height: _,
        operation: _,
        parent_commit: _,
        authority: _,
        catalog_before: _,
        roots: _,
        transition: _,
        history: _,
        facts: _,
        checkpoint: _,
        execution_environment: _,
    } = &*source.body;
    let mut rows = Vec::with_capacity(source.rows.len());
    for row in source.rows.iter() {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if row.occurrence.allocation_class() != MemoryClass::Recovery
            || row.occurrence.head_body_id() != source.body_id
            || row.occurrence.fact_body_id() != source.body_id
            || row.occurrence.location().height != roots.height
            || row.occurrence.kind() != ExecutionFactKindV1::Result
        {
            return Err(invalid().into());
        }
        let occurrence = readmit_fact_occurrence_ordinary_v1(
            &row.occurrence,
            head,
            roots,
            memory,
            &mut cancelled,
        )
        .map_err(AcceptedWorldResultRouterErrorV1::Formal)?;
        rows.push(AcceptedRoutedWorldResultV1 {
            occurrence,
            profile: row.profile,
        });
    }
    let body = body_charge.retain((*source.body).clone());
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(outer.retain(AcceptedWorldResultRoutingV1 {
        class: MemoryClass::Ordinary,
        rows: rows_charge.retain(rows),
        body,
        record: source.record,
        body_id: source.body_id,
        roots: source.roots,
        cognition: source.cognition,
        runtime: source.runtime,
    }))
}
