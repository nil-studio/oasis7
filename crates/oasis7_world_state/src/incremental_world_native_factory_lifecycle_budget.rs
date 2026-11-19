//! Finite joint-index work charged back to the caller's continuing native
//! allowance. Typed factory/industry visits additionally keep their own meters.
use super::*;
use crate::incremental_factory_lifecycle_index::{
    self as index, FactoryLifecycleIndexV1, FactoryLifecycleIndexWorkV1,
};
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("factory lifecycle aggregate index work")
}
fn allowance(
    factories: &ReservedValue<factory::FactoryFamilyQualificationV1>,
    industry: &ReservedValue<industry::IndustryFamilyQualificationV1>,
    work: &WorldNativeCompleteWorkV1,
    semantic_bytes: u64,
) -> Result<FactoryLifecycleIndexWorkV1, ValueTreeError> {
    let max_text = semantic_bytes.max(work.max_payload_bytes);
    let cells = work.legacy_cells_remaining;
    Ok(FactoryLifecycleIndexWorkV1 {
        cells_remaining: cells,
        comparison_bytes_remaining: cells
            .checked_mul(max_text)
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(limited)?,
        pending_entries_remaining: usize::try_from(cells).map_err(|_| limited())?,
        max_factories: usize::try_from(factories.record_counts()[0]).map_err(|_| limited())?,
        max_recipe_jobs: usize::try_from(industry.record_counts()[0]).map_err(|_| limited())?,
        max_text_bytes: usize::try_from(max_text).map_err(|_| limited())?,
        max_pending_records: work.native.max_pending_records,
    })
}
pub(super) fn qualify<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    factories: &ReservedValue<factory::FactoryFamilyQualificationV1>,
    industry: &ReservedValue<industry::IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<FactoryLifecycleIndexV1>, WorldNativeCompleteErrorV1> {
    let mut budget = allowance(factories, industry, work, ctx.semantic_bytes)?;
    let result = index::build_factory_lifecycle_index_v1(
        factories,
        industry,
        roots,
        ctx,
        outer_scan,
        &mut work.factory,
        &mut work.industry,
        &mut budget,
        work.max_payload_bytes,
    );
    work.legacy_cells_remaining = budget.cells_remaining;
    result.map_err(Into::into)
}
pub(super) fn accept<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &WorldNativeCompleteQualificationV1,
    factories: &ReservedValue<factory::FactoryFamilyQualificationV1>,
    industry: &ReservedValue<industry::IndustryFamilyQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<FactoryLifecycleIndexV1>, WorldNativeCompleteErrorV1> {
    let mut budget = allowance(factories, industry, work, ctx.semantic_bytes)?;
    let result = index::accept_factory_lifecycle_index_verified_v1(
        &source.lifecycle_index,
        &source.factory,
        &source.industry,
        factories,
        industry,
        witness,
        ctx,
        &mut budget,
    );
    work.legacy_cells_remaining = budget.cells_remaining;
    result.map_err(Into::into)
}
pub(super) fn readmit<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &WorldNativeCompleteQualificationV1,
    factories: &ReservedValue<factory::FactoryFamilyQualificationV1>,
    industry: &ReservedValue<industry::IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<FactoryLifecycleIndexV1>, WorldNativeCompleteErrorV1> {
    let mut budget = allowance(factories, industry, work, ctx.semantic_bytes)?;
    let result = index::readmit_factory_lifecycle_index_ordinary_v1(
        &source.lifecycle_index,
        factories,
        industry,
        roots,
        ctx.values.memory,
        &mut budget,
        &mut ctx.cancelled,
    );
    work.legacy_cells_remaining = budget.cells_remaining;
    result.map_err(Into::into)
}
