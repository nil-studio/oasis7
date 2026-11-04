//! Complete industry records, each retained under its original outer map key.
use crate::{
    incremental_field_member::*,
    incremental_projection::FieldShape,
    incremental_typed_record_codec::*,
    incremental_world_native::{
        WorldNativeFieldSpecV1, prepare_world_native_metadata_v1, prepare_world_native_record_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
#[path = "incremental_world_native_industry_dto.rs"]
mod dto;
macro_rules! field {
    ($marker:ident,$name:literal,$columns:literal,$shape:ident,$key:ty,$value:ty) => {
        pub struct $marker;
        impl WorldNativeFieldSpecV1 for $marker {
            type Key = $key;
            type Value = $value;
            const NAME: &'static str = $name;
            const COLUMNS: &'static str = $columns;
            const SHAPE: FieldShape = FieldShape::$shape;
        }
    };
}
field!(
    ProductValidationReceiptsFieldV1,
    "product_validation_receipts",
    "product_validation_receipts.columns",
    Map,
    u64,
    Vec<crate::ProductValidationReceiptV1>
);
field!(
    ProductProfilesFieldV1,
    "product_profiles",
    "product_profiles.columns",
    Map,
    String,
    oasis7_wasm_abi::ProductProfileV1
);
field!(
    LatestProductValidationFieldV1,
    "latest_product_validation",
    "latest_product_validation.columns",
    Value,
    (),
    Option<crate::LastProductValidationState>
);
field!(
    RecipeProfilesFieldV1,
    "recipe_profiles",
    "recipe_profiles.columns",
    Map,
    String,
    oasis7_wasm_abi::RecipeProfileV1
);
field!(
    FactoryProfilesFieldV1,
    "factory_profiles",
    "factory_profiles.columns",
    Map,
    String,
    oasis7_wasm_abi::FactoryProfileV1
);
field!(
    FactoryConstructionReceiptsFieldV1,
    "factory_construction_receipts",
    "factory_construction_receipts.columns",
    Map,
    String,
    oasis7_agent_api::world_quote::FactoryBuildPowerObligationV1
);
field!(
    ProductValidationAttemptsFieldV1,
    "product_validation_attempts",
    "product_validation_attempts.columns",
    Map,
    u64,
    Vec<crate::ProductValidationAttemptV1>
);
field!(
    FactoryRecycleReceiptsFieldV1,
    "factory_recycle_receipts",
    "factory_recycle_receipts.columns",
    Map,
    String,
    crate::FactoryRecycleReceiptV1
);
field!(
    IndustryProgressFieldV1,
    "industry_progress",
    "industry_progress.columns",
    Value,
    (),
    crate::IndustryProgressState
);

/// Contributes all nine original field markers and individual original records.
/// Empty maps retain metadata; both original Value fields always retain Unit
/// outer keys, including `latest_product_validation = None`.
pub fn prepare_world_native_industry_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &crate::WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut TypedRecordWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    prepare_world_native_metadata_v1::<ProductValidationReceiptsFieldV1>(ctx, records, sink)?;
    for (key, value) in &source.product_validation_receipts {
        prepare_world_native_record_v1::<ProductValidationReceiptsFieldV1>(
            key, value, ctx, records, work, sink,
        )?;
    }
    prepare_world_native_metadata_v1::<ProductProfilesFieldV1>(ctx, records, sink)?;
    for (key, value) in &source.product_profiles {
        prepare_world_native_record_v1::<ProductProfilesFieldV1>(
            key, value, ctx, records, work, sink,
        )?;
    }
    prepare_world_native_metadata_v1::<LatestProductValidationFieldV1>(ctx, records, sink)?;
    prepare_world_native_record_v1::<LatestProductValidationFieldV1>(
        &(),
        &source.latest_product_validation,
        ctx,
        records,
        work,
        sink,
    )?;
    prepare_world_native_metadata_v1::<RecipeProfilesFieldV1>(ctx, records, sink)?;
    for (key, value) in &source.recipe_profiles {
        prepare_world_native_record_v1::<RecipeProfilesFieldV1>(
            key, value, ctx, records, work, sink,
        )?;
    }
    prepare_world_native_metadata_v1::<FactoryProfilesFieldV1>(ctx, records, sink)?;
    for (key, value) in &source.factory_profiles {
        prepare_world_native_record_v1::<FactoryProfilesFieldV1>(
            key, value, ctx, records, work, sink,
        )?;
    }
    prepare_world_native_metadata_v1::<FactoryConstructionReceiptsFieldV1>(ctx, records, sink)?;
    for (key, value) in &source.factory_construction_receipts {
        prepare_world_native_record_v1::<FactoryConstructionReceiptsFieldV1>(
            key, value, ctx, records, work, sink,
        )?;
    }
    prepare_world_native_metadata_v1::<ProductValidationAttemptsFieldV1>(ctx, records, sink)?;
    for (key, value) in &source.product_validation_attempts {
        prepare_world_native_record_v1::<ProductValidationAttemptsFieldV1>(
            key, value, ctx, records, work, sink,
        )?;
    }
    prepare_world_native_metadata_v1::<FactoryRecycleReceiptsFieldV1>(ctx, records, sink)?;
    for (key, value) in &source.factory_recycle_receipts {
        prepare_world_native_record_v1::<FactoryRecycleReceiptsFieldV1>(
            key, value, ctx, records, work, sink,
        )?;
    }
    prepare_world_native_metadata_v1::<IndustryProgressFieldV1>(ctx, records, sink)?;
    prepare_world_native_record_v1::<IndustryProgressFieldV1>(
        &(),
        &source.industry_progress,
        ctx,
        records,
        work,
        sink,
    )?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_world_native_industry_test_profiles.rs"]
pub(crate) mod test_profiles;

#[cfg(test)]
#[path = "incremental_world_native_industry_fixture.rs"]
pub(crate) mod fixture;

#[cfg(test)]
#[path = "incremental_world_native_industry_tests.rs"]
mod tests;
