//! Fixed cold complete-evidence consumer. A genuine authenticated local
//! namespace token and full independently reconstructed execution material are
//! both mandatory; neither can stand in for the other's proof.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_material::CheckedCommitCore,
};
use oasis7_world_state::incremental_world_storage_source_cold::AuthenticatedColdWorldStorageSourceV1;

pub(in crate::controlled_authority::formal::endpoint::incremental) fn qualify_cold_complete_data_v1(
    checkpoint: &super::super::super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    core: &ReservedValue<CheckedCommitCore>,
    source: &ReservedValue<AuthenticatedColdWorldStorageSourceV1>,
    material: ReservedValue<CandidateExecutionMaterialV1>,
    pages: ReservedValue<StoreReadWitness>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<CompleteDataClosureEvidenceV1>, FormalError> {
    checkpoint.validate_context(common)?;
    checkpoint.validate_material_binding_v1(common.memory(), core)?;
    common.charge(16, 32 * 12)?;
    let (native, _generation) = pages
        .authenticated_cold_native_binding_v1()
        .ok_or_else(|| invalid("cold complete data requires authenticated cold namespace token"))?;
    let record = core.candidate_record_id();
    let body = core.catalog().entry.commit;
    let roots = core.body().roots;
    if !source.uses_budget(common.memory())
        || !material.uses_budget(common.memory())
        || !pages.uses_budget(common.memory())
        || source.record_id() != record
        || source.body_id() != body
        || source.roots_id() != roots
        || material.candidate_record_id() != record
        || material.body_id() != body
        || material.roots_id() != roots
        || material.manifest_id() != core.execution_artifacts().manifest
        || pages.candidate_record_id() != record
        || pages.body_id() != body
        || pages.roots_id() != roots
        || material.page_namespace_identity() != pages.page_namespace_identity()
        || material.native_namespace_identity() != native
    {
        return Err(invalid(
            "cold complete material/source/namespace conjunction",
        ));
    }
    let cognition = source.results().cognition().occurrence().location();
    let runtime = source.results().runtime_history().occurrence().location();
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<CompleteDataClosureEvidenceV1>() + 512,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("cold complete data admission"))?;
    Ok(fee.retain(CompleteDataClosureEvidenceV1 {
        material,
        pages,
        cognition,
        runtime,
    }))
}
