//! Actual parent-to-Current Factory/Recipe directory admission. The resident
//! cache is immutable; every call folds the transaction's genuine changed rows.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_factory_directory_records::{
        AcceptedFactoryRecipeDirectoryV1, CurrentFactoryRecipeDirectoryV1,
        FactoryRecipeDirectoryWorkV1, open_current_factory_recipe_directory_v1,
    },
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_world_state::{
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::IndustryTransitionReadContextV1,
};
fn limit() -> BodyMutationError {
    ValueTreeError::ResourceLimited("industrial Current directory work").into()
}
fn map(
    e: crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1,
) -> BodyMutationError {
    use crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1 as E;
    match e {
        E::Execution(e) => e.into(),
        E::Value(e) => e.into(),
        E::Body(e) => e,
        E::Overlay(e) => match e {
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(
                e,
            ) => e.into(),
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
                e.into()
            }
            e => BodyMutationError::Original(Box::new(e)),
        },
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn open_current_due_records_v1<'q, M, P, V, W, D, E, C, PE, PC, CW, ME>(
    accepted: &ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    directory_work: &mut FactoryRecipeDirectoryWorkV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &FactoryScheduleSourcesV1<'q>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentFactoryRecipeDirectoryV1<'q>>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    directory_work.comparisons_remaining = directory_work
        .comparisons_remaining
        .min(common.work().remaining_read_bytes());
    directory_work.pending_entries_remaining = directory_work
        .pending_entries_remaining
        .min(usize::try_from(common.work().remaining_steps()).unwrap_or(usize::MAX));
    let before = (
        directory_work.comparisons_remaining,
        directory_work.pending_entries_remaining,
    );
    crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(
        step,
        read,
        parent,
        write,
        materials,
        work,
        common,
        live,
        |step, read, parent, _write, _materials, work, tail| {
            let mut factory = FactoryTransitionReadContextV1 {
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                max_payload_bytes: read.record_bytes,
                cancelled: &mut read.cancelled,
            };
            let mut recipe = IndustryTransitionReadContextV1 {
                values: ValueReadContext {
                    source: parent.values.source,
                    memory: parent.values.memory,
                    class: parent.values.class,
                    io: &mut *parent.values.io,
                },
                key_work: &mut *parent.key_work,
                semantic_bytes: parent.semantic_bytes,
                record_bytes: parent.record_bytes,
                max_payload_bytes: parent.record_bytes,
                cancelled: &mut parent.cancelled,
            };
            let guard = step.transaction.guard();
            let result = open_current_factory_recipe_directory_v1(
                accepted,
                sources.lifecycle.factories,
                sources.lifecycle.industry,
                step.transaction,
                &mut factory,
                &mut recipe,
                &mut work.lifecycle.factory,
                &mut work.lifecycle.industry,
                directory_work,
                &guard,
                live,
            )
            .map_err(map);
            let bytes = before
                .0
                .checked_sub(directory_work.comparisons_remaining)
                .ok_or_else(limit)?;
            let entries = before
                .1
                .checked_sub(directory_work.pending_entries_remaining)
                .ok_or_else(limit)?;
            let account: Result<(), BodyMutationError> = (|| {
                tail.charge(usize::try_from(bytes).map_err(|_| limit())?)?;
                for _ in 0..entries {
                    tail.charge_record()?;
                }
                Ok(())
            })();
            match result {
                Err(e) => Err(e),
                Ok(value) => {
                    account?;
                    Ok(value)
                }
            }
        },
    )
}
