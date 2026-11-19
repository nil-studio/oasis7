//! Full original scheduler restoration, including legacy digest hydration.
//! Selected output is moved only after every original bucket was validated.
use super::*;
use crate::cognition_scheduler::{CognitionScheduler, SchedulerPaidErrorV1};
use oasis7_world_state::ContinuationStatusV1;
pub(super) fn current_in_flight(
    view: &values::View<'_, '_, '_>,
    continuations: Vec<AgentContinuation>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<Vec<SchedulerWakeV1>, BodyMutationError> {
    let Some(row) = view.optional(&["cognition", "scheduler_state"])? else {
        return Ok(Vec::new());
    };
    if view.is_null(&row)? {
        return Ok(Vec::new());
    }
    // The original getter tests scheduler presence before validating any active
    // continuation, and validates all active continuations, across every agent.
    let mut active = Vec::new();
    for continuation in continuations {
        view.facts.work(1, 0)?;
        if matches!(
            continuation.status,
            ContinuationStatusV1::Completed
                | ContinuationStatusV1::Cancelled
                | ContinuationStatusV1::Invalidated
                | ContinuationStatusV1::Expired
                | ContinuationStatusV1::Rejected
        ) {
            continue;
        }
        // The only new owned strings are the fixed status digest and an original
        // fixed error note. Keep their real graph admission in the outer keeper.
        view.facts.before_owned(4096)?;
        let result={let mut common=view.facts.common.borrow_mut();crate::world::normal_bootstrap_current::hash::with_current_meter_v1(&mut common,canonical,|source|{
    crate::world::cognition_persistence::normalize_cognition_continuation_with_work_v1(continuation,source,&mut |n|oasis7_wasm_abi::CanonicalCborAllocator::charge_work(source,1,n as u64))
  })?}.map_err(paid)?;
        let continuation = match result {
            Ok(v) => v,
            Err(error) => return Err(view.business(format_args!("{error:?}"))),
        };
        values::push(view.facts, &mut active, continuation)?;
    }
    let mut state = match view.decode::<serde_json::Value>(&row)? {
        Ok(v) => v,
        Err(error) => return Err(view.business(format_args!("{error}"))),
    };
    crate::world::cognition_orchestration::hydrate_scheduler_digest_fields_with_work_v1(
        &mut state,
        &active,
        &mut |s, b, heap| {
            view.facts.work(s, b)?;
            if heap > 0 {
                view.facts.before_owned(heap)?
            }
            Ok::<_, BodyMutationError>(())
        },
    )?;
    view.before_clone(&state)?;
    let result = CognitionScheduler::from_snapshot_ref_paid_v1(&state, &mut |n| {
        view.facts.work(1, n)?;
        view.facts.before_owned(n)
    });
    let scheduler = match result {
        Ok(v) => v,
        Err(SchedulerPaidErrorV1::Work(e)) => return Err(e),
        Err(SchedulerPaidErrorV1::Scheduler(error)) => {
            return Err(view.business(format_args!(
                "{:?}",
                crate::WorldError::DistributedValidationFailed {
                    reason: view
                        .format(format_args!("cognition scheduler failed: {}", error.code()))?
                }
            )));
        }
    };
    scheduler.into_in_flight_wakes_with_work_v1(&mut |count| {
        let bytes = count
            .checked_mul(size_of::<SchedulerWakeV1>())
            .ok_or_else(|| invalid("Cognition in-flight output layout"))?;
        view.facts.work(count as u64, bytes)?;
        view.facts.before_owned(bytes)
    })
}
