//! The fixed ALL-intent source lanes share the original Common allowance.
use super::*;
use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1;
pub(in crate::world::incremental_normal_cognition_service) fn with_intent_aggregate_v2<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    T,
>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut CurrentCognitionServiceWorkV2<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        &mut CurrentCognitionServiceWorkV2<'_>,
    ) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        common.charge(8, 16384).map_err(formal)?;
        if !ctx.values.memory.same_pool(common.memory())
            || !ctx.transaction.uses_budget(common.memory())
        {
            return Err(bad().into());
        }
        let (_, meter, stop) = common.parts();
        if stop() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let lease = meter.begin_child().map_err(formal)?;
        fn lanes<
            M: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        >(
            ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
            work: &mut CurrentCognitionServiceWorkV2<'_>,
            f: &mut impl FnMut(&mut u64, bool),
        ) {
            for lane in [
                &mut ctx.values.io.nodes_remaining,
                &mut work.authority.scan.nodes_remaining,
                &mut work.authority.scan.leaves_remaining,
                &mut work.authority.cells_remaining,
                &mut *work.authority_cells,
            ] {
                f(lane, false)
            }
            for lane in [
                &mut ctx.values.io.read_bytes_remaining,
                &mut ctx.key_work.bytes_remaining,
                &mut work.authority.scan.read_bytes_remaining,
            ] {
                f(lane, true)
            }
        }
        let steps = lease.remaining_steps() / 6;
        let bytes = lease.remaining_read_bytes() / 3;
        let mut original = [0; 8];
        let mut caps = [0; 8];
        let mut i = 0;
        lanes(ctx, work, &mut |n, b| {
            original[i] = *n;
            *n = (*n).min(if b { bytes } else { steps });
            caps[i] = *n;
            i += 1;
        });
        let old_pending = *work.pending_entries;
        let pending_cap = old_pending.min(usize::try_from(steps).unwrap_or(usize::MAX));
        *work.pending_entries = pending_cap;
        let call = run(ctx, work);
        let mut used_steps = 0u64;
        let mut used_bytes = 0u64;
        let mut broken = false;
        let mut i = 0;
        lanes(ctx, work, &mut |n, b| {
            broken |= *n > caps[i];
            let used = caps[i].saturating_sub(*n);
            *n = original[i] - used;
            i += 1;
            let total = if b { &mut used_bytes } else { &mut used_steps };
            if let Some(next) = total.checked_add(used) {
                *total = next
            } else {
                broken = true
            }
        });
        broken |= *work.pending_entries > pending_cap;
        let used_pending = pending_cap.saturating_sub(*work.pending_entries);
        *work.pending_entries = old_pending - used_pending;
        used_steps = used_steps
            .checked_add(used_pending as u64)
            .ok_or_else(bad)?;
        let debit = lease.consume(used_steps, used_bytes).map_err(formal);
        lease.finish();
        match call {
            Err(error) => Err(error),
            Ok(value) => {
                debit?;
                if broken {
                    return Err(bad().into());
                }
                if stop() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                Ok(value)
            }
        }
    }));
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            Err(error)
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
