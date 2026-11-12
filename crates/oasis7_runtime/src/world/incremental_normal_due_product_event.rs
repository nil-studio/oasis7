//! Closed post-write industrial events, with the original counted sinks.
use super::*;
use crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1;
use crate::world::incremental_normal_system_event::PreparedNormalSystemEventV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_formal_protocol::incremental_execution::ExecutionFence;

macro_rules! contribute_source {
    ($name:ident, $type:ident, $constructor:ident) => {
        #[allow(clippy::too_many_arguments)]
        pub(in crate::world) fn $name<'event, M, V, W, C, F, ME>(
            now: u64,
            source: &'event crate::world::incremental_normal_industry_due::$type,
            common: &mut FormalBoundedContextV1<'_>,
            live: &ExecutionFence,
            tx: &mut ExecutionTransaction<'_, M>,
            events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
            materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        ) -> Result<PersistedWorldEventV1<'event>, BodyMutationError>
        where
            M: MerkleNodeReader,
            V: ValueNodeReader,
            W: ValueNodeWriter,
            C: FnMut() -> bool,
            F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
            ME: FnMut() -> Option<ValueTreeError>,
        {
            tx.contribute(|tx| {
                let cap = PreparedNormalSystemEventV1::$constructor(source, tx, now, common, live)?;
                contribute_prepared_due_event_v1(now, &cap, common, live, tx, events, materials)
            })
        }
    };
}
contribute_source!(
    contribute_due_product_attempt_event_v1,
    PreparedNormalProductAttemptV1,
    from_product_attempt
);
contribute_source!(
    contribute_due_product_recorded_event_v1,
    PreparedNormalProductRecordedV1,
    from_product_recorded
);
contribute_source!(
    contribute_due_product_result_event_v1,
    PreparedNormalProductResultV1,
    from_product_result
);
contribute_source!(
    contribute_due_recipe_failure_event_v1,
    PreparedNormalRecipeFailureV1,
    from_recipe_failure
);
contribute_source!(
    contribute_due_product_module_failure_event_v1,
    PreparedNormalProductModuleFailureV1,
    from_product_module_failure
);
contribute_source!(
    contribute_due_product_cursor_event_v1,
    PreparedNormalProductCursorV1,
    from_product_cursor
);

#[allow(clippy::too_many_arguments)]
fn contribute_prepared_due_event_v1<'event, M, V, W, C, F, ME>(
    now: u64,
    cap: &PreparedNormalSystemEventV1<'event>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    tx: &mut ExecutionTransaction<'_, M>,
    events: &mut ModuleOutputEventContextV1<'_, V, W, C, F>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
) -> Result<PersistedWorldEventV1<'event>, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    ME: FnMut() -> Option<ValueTreeError>,
{
    cap.validate_common(common)?;
    cap.validate(tx, events.mutation.memory, now)?;
    if matches!(&cap.event().body, WorldEventBody::Domain(_)) {
        let guard = tx.guard();
        let mut step = crate::world::incremental_step::IwsStepContext::borrow_counted_at(
            tx,
            now,
            &guard,
            live,
            events.values,
            &mut events.mutation,
            &mut events.original_work,
            events.world_events,
        )?;
        step.append_prepared_normal_system_event_v1(
            cap,
            materials,
            common,
            live,
            events.max_event_bytes,
            &mut events.persist_original,
        )?;
        let identity = step.transaction.guard();
        common
            .consume_steps(0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&identity, live)?;
        return Ok(PersistedWorldEventV1 {
            owner: PersistedEventOwnerV1::Module {
                event: cap.event(),
                identity,
            },
        });
    }
    if !matches!(
        &cap.event().body,
        WorldEventBody::ModuleCallFailed(_)
            | WorldEventBody::ProductValidationDeliveryCursorUpdated(_)
    ) {
        return Err(BodyMutationError::Invalid("industrial global event source"));
    }
    // These originals have no agent mailbox. Their fixed, already-applied
    // source uses the exact allocator/original/counter kernel directly.
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (_, meter, stop) = common.parts();
        let child = meter
            .begin_child()
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let visits = child.remaining_steps() / 2;
        let bytes = child.remaining_read_bytes() / 3;
        events.mutation.io.nodes_remaining = events.mutation.io.nodes_remaining.min(visits);
        events.original_work.visits_remaining = events.original_work.visits_remaining.min(visits);
        events.mutation.io.read_bytes_remaining =
            events.mutation.io.read_bytes_remaining.min(bytes);
        events.mutation.io.write_bytes_remaining =
            events.mutation.io.write_bytes_remaining.min(bytes);
        events.original_work.bytes_remaining = events.original_work.bytes_remaining.min(bytes);
        let before_steps = [
            events.mutation.io.nodes_remaining,
            events.original_work.visits_remaining,
        ];
        let before_bytes = [
            events.mutation.io.read_bytes_remaining,
            events.mutation.io.write_bytes_remaining,
            events.original_work.bytes_remaining,
        ];
        let placeholder = CanonicalCborWorkV1 {
            visits_remaining: 0,
            bytes_remaining: 0,
            max_depth: events.original_work.max_depth,
            max_map_keys: events.original_work.max_map_keys,
            max_key_bytes: events.original_work.max_key_bytes,
        };
        let original_work = std::mem::replace(&mut events.original_work, placeholder);
        let mut scoped = ModuleOutputEventContextV1 {
            values: events.values,
            world_events: events.world_events,
            mutation: BodyMutationContext {
                sink: &mut *events.mutation.sink,
                memory: events.mutation.memory,
                io: &mut *events.mutation.io,
                cancelled: || (events.mutation.cancelled)() || stop(),
            },
            persist_original: &mut events.persist_original,
            max_event_bytes: events.max_event_bytes,
            original_work,
        };
        let result = persist_counted_event_original_v1(now, cap.event(), tx, &mut scoped);
        events.original_work = scoped.original_work;
        drop(scoped.mutation);
        let after_steps = [
            events.mutation.io.nodes_remaining,
            events.original_work.visits_remaining,
        ];
        let after_bytes = [
            events.mutation.io.read_bytes_remaining,
            events.mutation.io.write_bytes_remaining,
            events.original_work.bytes_remaining,
        ];
        let delta = |before: &[u64], after: &[u64]| {
            before
                .iter()
                .zip(after)
                .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
        };
        let Some(steps) = delta(&before_steps, &after_steps) else {
            return result.and(Err(BodyMutationError::Invalid(
                "industrial original step accounting",
            )));
        };
        let Some(bytes) = delta(&before_bytes, &after_bytes) else {
            return result.and(Err(BodyMutationError::Invalid(
                "industrial original byte accounting",
            )));
        };
        let debit = child
            .consume(steps, bytes)
            .map_err(|e| BodyMutationError::Original(Box::new(e)));
        child.finish();
        let persisted = result?;
        debit?;
        common
            .consume_steps(0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(persisted)
    }));
    match attempt {
        Ok(result) => result,
        Err(payload) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> = tx
                .contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(payload)
        }
    }
}
