//! One fixed inventory for this stage's actual IO/JSON/Original/hash lanes.
use super::*;
fn lanes<M, R, E, C, W, CW>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    w: &mut CurrentCognitionExpiryWorkV1,
    visit: &mut impl FnMut(&mut u64, bool),
) {
    for n in [
        &mut ctx.values.io.nodes_remaining,
        &mut write.io.nodes_remaining,
        &mut w.read.cells_remaining,
        &mut w.read.json_nodes_remaining,
        &mut w.write.cells_remaining,
        &mut w.write.json_nodes_remaining,
        &mut w.index.cells_remaining,
        &mut w.cbor.nodes_remaining,
        &mut w.cbor.source_reads_remaining,
        &mut w.hash.hash_nodes_remaining,
        &mut w.original.visits_remaining,
    ] {
        visit(n, false)
    }
    for n in [
        &mut ctx.values.io.read_bytes_remaining,
        &mut ctx.values.io.write_bytes_remaining,
        &mut ctx.key_work.bytes_remaining,
        &mut write.io.read_bytes_remaining,
        &mut write.io.write_bytes_remaining,
        &mut write.key_work.bytes_remaining,
        &mut w.cbor.bytes_remaining,
        &mut w.cbor.key_bytes_remaining,
        &mut w.hash.input_bytes_remaining,
        &mut w.original.bytes_remaining,
    ] {
        visit(n, true)
    }
}
pub(in crate::world) fn paid<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
    T,
>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        &mut MemberRecordWriteContext<'_, W, CW>,
        &mut CurrentCognitionExpiryWorkV1,
    ) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        common.charge(1, 8192).map_err(formal)?;
        let _metadata = common
            .memory()
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if !ctx.transaction.uses_budget(common.memory())
            || !ctx.values.memory.same_pool(common.memory())
            || !write.memory.same_pool(common.memory())
        {
            return Err(bad().into());
        }
        let (_, meter, stop) = common.parts();
        if stop() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let lease = meter.begin_child().map_err(formal)?;
        let mut saved = [0u64; 21];
        let mut i = 0;
        let mut steps = 0;
        let mut bytes = 0;
        lanes(ctx, write, work, &mut |n, b| {
            saved[i] = *n;
            i += 1;
            if b { bytes += 1 } else { steps += 1 }
        });
        let s = lease.remaining_steps() / steps;
        let b = lease.remaining_read_bytes() / bytes;
        i = 0;
        lanes(ctx, write, work, &mut |n, byte| {
            *n = (*n).min(if byte { b } else { s });
            i += 1
        });
        let call = run(ctx, write, work);
        let mut used_s = 0u64;
        let mut used_b = 0u64;
        let mut invalid = false;
        i = 0;
        lanes(ctx, write, work, &mut |n, byte| {
            let cap = saved[i].min(if byte { b } else { s });
            let used = cap.saturating_sub(*n);
            invalid |= *n > cap;
            if byte {
                used_b = used_b.saturating_add(used)
            } else {
                used_s = used_s.saturating_add(used)
            }
            *n = saved[i] - used;
            i += 1
        });
        let debit = lease.consume(used_s, used_b);
        lease.finish();
        let accounting = debit
            .map_err(formal)
            .and_then(|_| if invalid { Err(bad().into()) } else { Ok(()) });
        match (call, accounting) {
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(value), Ok(())) => {
                if stop() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(value)
                }
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
        Err(payload) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(payload)
        }
    }
}
