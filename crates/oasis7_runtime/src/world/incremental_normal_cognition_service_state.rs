//! Actual selected Current records, with the original cheap service gate.
use super::*;
use oasis7_world_state::{
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_normal_cognition_current::{
        CurrentCognitionJsonRecordV1, read_current_cognition_json_record_v1,
    },
};
pub(super) struct CurrentServiceStateV2 {
    pub(super) scheduler: Option<ReservedValue<CurrentCognitionJsonRecordV1>>,
    pub(super) continuations: Option<ReservedValue<CurrentCognitionJsonRecordV1>>,
    pub(super) contexts: Option<ReservedValue<CurrentCognitionJsonRecordV1>>,
    pub(super) evaluations: Option<ReservedValue<CurrentCognitionJsonRecordV1>>,
}
impl CurrentServiceStateV2 {
    pub(super) fn has_active_or_pending(&self) -> bool {
        self.scheduler.as_ref().is_some_and(|state| {
            let value = state.value();
            !value.is_null()
                && (value
                    .get("active")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|rows| !rows.is_empty())
                    || value
                        .get("backpressure")
                        .and_then(serde_json::Value::as_object)
                        .is_some_and(|rows| !rows.is_empty()))
        })
    }
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => formal(e),
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(super) fn read_current_cognition_service_state_v2(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, '_>,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentServiceStateV2, RootModuleExecutionErrorV1<O::Error>> {
        let scheduler = oasis7_current_read(
            self,
            sources,
            CognitionJsonFieldV1::SchedulerState,
            work,
            common,
            live_owner,
        )?;
        let mut state = CurrentServiceStateV2 {
            scheduler,
            continuations: None,
            contexts: None,
            evaluations: None,
        };
        if !state.has_active_or_pending() {
            return Ok(state);
        }
        state.continuations = oasis7_current_read(
            self,
            sources,
            CognitionJsonFieldV1::Continuations,
            work,
            common,
            live_owner,
        )?;
        state.contexts = oasis7_current_read(
            self,
            sources,
            CognitionJsonFieldV1::ContinuationContexts,
            work,
            common,
            live_owner,
        )?;
        state.evaluations = oasis7_current_read(
            self,
            sources,
            CognitionJsonFieldV1::ContinuationEvaluations,
            work,
            common,
            live_owner,
        )?;
        Ok(state)
    }
}
pub(super) fn oasis7_current_read<M, R, E, C, W, F, O>(
    root: &mut RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>,
    sources: &CurrentCognitionServiceSourcesV2<'_, '_>,
    field: CognitionJsonFieldV1,
    work: &mut CurrentCognitionServiceWorkV2<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
) -> Result<Option<ReservedValue<CurrentCognitionJsonRecordV1>>, RootModuleExecutionErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    crate::world::incremental_normal_cognition_expiry::with_current_cognition_io_v1(
        root.ctx,
        root.members,
        work.expiry,
        common,
        |ctx, _, work| {
            let guard = ctx.transaction.guard();
            read_current_cognition_json_record_v1(
                field,
                sources.current,
                sources.parent.refs().roots,
                ctx,
                &mut work.read,
                work.max_selected_heap,
                &guard,
                &live_owner(),
            )
            .map_err(overlay)
        },
    )
    .map_err(RootModuleExecutionErrorV1::Body)
}
