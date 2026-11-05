//! Pre-accept actual after-owner binding and pure accepted refinement.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFactKindV1,
    incremental_material::{
        CheckedCommitCore, PreparedFactPromotionV1, finish_prepared_fact_promotion_v1,
        prepare_candidate_fact_promotion_v1,
    },
};
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_world_state::incremental_runtime_history_source::CheckedRuntimeHistorySourceV1;
pub enum WorldManifestHashCandidateBaseV1<'a> {
    Accepted(&'a ReservedValue<AcceptedWorldManifestHashV1>),
    Current(&'a ReservedValue<CurrentWorldManifestHashV1>),
}
pub struct PreparedWorldManifestHashPromotionV1 {
    baseline: Arc<ReservedValue<Baseline>>,
    selected: PreparedFactPromotionV1,
    charge: MemoryReservation,
}
pub fn prepare_world_manifest_hash_promotion_v1<M, R, E, C>(
    base: Option<WorldManifestHashCandidateBaseV1<'_>>,
    core: &ReservedValue<CheckedCommitCore>,
    runtime: &ReservedValue<CheckedRuntimeHistorySourceV1>,
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RootWorldManifestHashWorkV1,
) -> Result<PreparedWorldManifestHashPromotionV1, RootWorldManifestHashErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let memory = ctx.values.memory;
    let charge = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let selected = runtime.occurrence();
    if ctx.values.class != MemoryClass::Ordinary
        || !core.uses_budget(memory)
        || !runtime.uses_budget(memory)
        || selected.kind() != ExecutionFactKindV1::Result
        || selected.candidate_record_id() != core.candidate_record_id()
        || selected.body_id() != core.catalog().entry.commit
        || selected.roots_id() != core.body().roots
        || runtime.manifest().roots != core.body().roots
    {
        return Err(invalid().into());
    }
    let old = match (core.parent_record_id(), base) {
        (None, None) if core.roots().height == 0 => None,
        (Some(record), Some(base)) => {
            let (parent, value) = match base {
                WorldManifestHashCandidateBaseV1::Accepted(cache) => {
                    if !cache.uses_budget(memory) || cache.class != MemoryClass::Ordinary {
                        return Err(invalid().into());
                    }
                    (cache.scope, &cache.baseline)
                }
                WorldManifestHashCandidateBaseV1::Current(cache) => {
                    if !cache.uses_budget(memory) {
                        return Err(invalid().into());
                    }
                    (cache.parent, &cache.baseline)
                }
            };
            if parent.record != record
                || Some(parent.body) != core.body().parent_commit
                || core.parent_body().map(|b| b.roots) != Some(parent.roots)
                || !value.uses_budget(memory)
                || value.body.allocation_class() != MemoryClass::Ordinary
            {
                return Err(invalid().into());
            }
            Some(value)
        }
        _ => return Err(invalid().into()),
    };
    let body = open_qualified_artifact_config_body_v1(
        ArtifactConfigFieldV1::Manifest,
        ArtifactConfigRecordKeyV1::Global,
        core.roots(),
        family,
        ctx,
    )?
    .ok_or_else(invalid)?;
    let value = if let Some(old) = old.filter(|b| b.body.root() == body.root()) {
        old.clone()
    } else {
        baseline(
            body,
            ctx.state,
            &mut ctx.values,
            ctx.key_work,
            ctx.semantic_bytes,
            ctx.record_bytes,
            &mut ctx.take_state_error,
            &mut ctx.cancelled,
            work,
        )?
    };
    let selected = prepare_candidate_fact_promotion_v1(selected, memory, &mut ctx.cancelled)
        .map_err(RootWorldManifestHashErrorV1::Formal)?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(PreparedWorldManifestHashPromotionV1 {
        baseline: value,
        selected,
        charge,
    })
}
/// Actual VerifiedCommit refinement plus fixed moves only: no admission,
/// cancellation, page reads or hashes are introduced after acceptance.
pub fn finish_prepared_world_manifest_hash_promotion_v1(
    prepared: PreparedWorldManifestHashPromotionV1,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<AcceptedWorldManifestHashV1>, RootWorldManifestHashErrorV1> {
    let fact = finish_prepared_fact_promotion_v1(prepared.selected, accepted)
        .map_err(RootWorldManifestHashErrorV1::Formal)?;
    let scope = Scope {
        record: fact.head_record_id(),
        body: fact.head_body_id(),
        roots: fact.roots_id(),
    };
    Ok(prepared.charge.retain(AcceptedWorldManifestHashV1 {
        baseline: prepared.baseline,
        scope,
        class: MemoryClass::Ordinary,
    }))
}
