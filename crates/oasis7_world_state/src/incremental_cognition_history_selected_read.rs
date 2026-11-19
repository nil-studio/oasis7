//! Reconstruct current-view selection from the actual selected accepted body.
//! Only this commit's bounded Result set is scanned, never previous views/H.
use super::*;
use oasis7_formal_protocol::{
    incremental_history::{CommitBodyV1, FactSetV1, HISTORY_MAX_HEADER_BYTES},
    incremental_material::{FactReadContextV1, FactRequestV1, verify_fact_occurrence_v1},
    incremental_state::MerkleNodeReader,
};

pub fn recover_selected_cognition_history_view_v1<O, R, C>(
    request: &FactRequestV1<'_>,
    context: &mut FactReadContextV1<'_, O, R, C>,
) -> Result<ReservedValue<AcceptedCognitionHistoryViewV1>, CognitionHistorySelectionErrorV1<O::Error>>
where
    O: OriginalObjectStreamReader,
    R: MerkleNodeReader,
    C: FnMut() -> bool,
{
    let memory = context.memory;
    let class = context.class;
    let charge = memory
        .reserve(16 << 10, class)
        .map_err(ValueTreeError::Memory)?;
    if request.kind != ExecutionFactKindV1::Result {
        return Err(ValueTreeError::Invalid("Cognition selected view kind").into());
    }
    let occurrence = verify_fact_occurrence_v1(request, context)
        .map_err(CognitionHistorySelectionErrorV1::Provenance)?;
    // An authentic historical Result is not authority to select current state.
    if occurrence.allocation_class() != class
        || occurrence.fact_body_id() != occurrence.head_body_id()
    {
        return Err(ValueTreeError::Invalid("Cognition selected view historical body").into());
    }
    let body_bytes = load(
        &occurrence.fact_body_id(),
        HISTORY_MAX_HEADER_BYTES,
        context,
    )?;
    let body = CommitBodyV1::from_canonical_bytes(&body_bytes)
        .map_err(|_| ValueTreeError::Invalid("Cognition selected body format"))?;
    if body.roots != occurrence.roots_id() || body.height != request.roots.height {
        return Err(ValueTreeError::Invalid("Cognition selected body roots").into());
    }
    let fact_bytes = load(&body.facts, 256 << 10, context)?;
    let facts =
        FactSetV1::from_canonical_bytes_in_class(&fact_bytes, memory, class).map_err(|error| {
            CognitionHistorySelectionErrorV1::Provenance(CoreError::Execution(error))
        })?;
    if facts.scope != body.scope || facts.operation != body.operation {
        return Err(ValueTreeError::Invalid("Cognition selected facts binding").into());
    }
    let mut selected = None;
    for (ordinal, fact) in facts.facts.iter().enumerate() {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if fact.kind != ExecutionFactKindV1::Result {
            continue;
        }
        context.fact_budget.facts_remaining = context
            .fact_budget
            .facts_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("Cognition selected probes"))?;
        let limit = context
            .fact_budget
            .max_fact_bytes
            .min(context.fact_budget.original_bytes_remaining);
        let mut probe = Probe {
            prefix: [0; COGNITION_HISTORY_VIEW_V3_BYTES],
            copied: 0,
            bytes: 0,
            hash: blake3::Hasher::new(),
            limit,
            remaining: &mut context.fact_budget.original_bytes_remaining,
            cancelled: &mut context.cancelled,
            stopped: None,
        };
        let observed = context
            .originals
            .stream_original(&fact.id, limit, memory, &mut probe);
        if let Some(error) = probe.stopped.take() {
            return Err(error.into());
        }
        let observed = observed.map_err(CognitionHistorySelectionErrorV1::Original)?;
        if observed != probe.bytes
            || observed > limit
            || probe.hash.finalize().as_bytes() != &fact.id
        {
            return Err(ValueTreeError::Invalid("Cognition selected Result identity").into());
        }
        if !(probe.prefix[..probe.copied].starts_with(COGNITION_HISTORY_VIEW_MAGIC)
            || probe.prefix[..probe.copied].starts_with(COGNITION_HISTORY_VIEW_V2_MAGIC)
            || probe.prefix[..probe.copied].starts_with(COGNITION_HISTORY_VIEW_V3_MAGIC))
        {
            continue;
        }
        if selected.is_some() {
            return Err(
                ValueTreeError::Invalid("duplicate Cognition History view manifest").into(),
            );
        }
        if observed > COGNITION_HISTORY_VIEW_V3_BYTES as u64
            || ordinal as u64 != request.location.ordinal
            || fact.id != request.location.original
            || observed != occurrence.original_bytes()
        {
            return Err(ValueTreeError::Invalid("Cognition selected manifest location").into());
        }
        let (manifest, derived, source_v3) =
            super::super::manifest_v3::decode_profile_with_source_v3(
                &probe.prefix[..observed as usize],
                memory,
                class,
                &mut context.cancelled,
            )?;
        if manifest.roots != body.roots {
            return Err(ValueTreeError::Invalid("Cognition selected manifest roots").into());
        }
        selected = Some((manifest, derived, source_v3));
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let (manifest, derived, source_v3) = selected.ok_or(ValueTreeError::Invalid(
        "missing Cognition History view manifest",
    ))?;
    Ok(charge.retain(AcceptedCognitionHistoryViewV1 {
        class,
        manifest,
        derived,
        source_v3,
        occurrence,
    }))
}

fn load<O, R, C>(
    id: &[u8; 32],
    limit: usize,
    context: &mut FactReadContextV1<'_, O, R, C>,
) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, CognitionHistorySelectionErrorV1<O::Error>>
where
    O: OriginalObjectStreamReader,
    R: MerkleNodeReader,
    C: FnMut() -> bool,
{
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    context.core_budget.objects_remaining =
        context.core_budget.objects_remaining.checked_sub(1).ok_or(
            ValueTreeError::ResourceLimited("Cognition selected headers"),
        )?;
    let limit = limit
        .min(usize::try_from(context.core_budget.original_bytes_remaining).unwrap_or(usize::MAX));
    let bytes = context
        .originals
        .read_original(id, limit, context.memory)
        .map_err(CognitionHistorySelectionErrorV1::Original)?;
    if bytes.len() > limit
        || !bytes.uses_budget(context.memory)
        || bytes.reserved_bytes() < bytes.capacity()
    {
        return Err(ValueTreeError::Invalid("Cognition selected header carrier").into());
    }
    let mut hash = blake3::Hasher::new();
    for part in bytes.chunks(oasis7_formal_protocol::incremental_value::VALUE_CHUNK_BYTES) {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        context.core_budget.original_bytes_remaining = context
            .core_budget
            .original_bytes_remaining
            .checked_sub(part.len() as u64)
            .ok_or(ValueTreeError::ResourceLimited(
                "Cognition selected header bytes",
            ))?;
        hash.update(part);
    }
    if hash.finalize().as_bytes() != id {
        return Err(ValueTreeError::Invalid("Cognition selected header identity").into());
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(bytes)
}
